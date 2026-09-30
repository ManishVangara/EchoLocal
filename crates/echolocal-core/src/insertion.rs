//! Platform-independent text insertion policy.
//!
//! The macOS crate performs the actual insertion; this module decides which
//! methods to try for a target, and judges from before/after snapshots of the
//! focused field whether an attempt landed.
//!
//! Fallback order:
//! 1. Accessibility — set the focused element's selected text. Only used for
//!    native text controls in apps where it is known to behave; in browsers,
//!    Electron apps and terminals it can "succeed" without the app noticing.
//! 2. Key events — synthesized Unicode keyboard events posted to the target
//!    process. Layout-independent and works in terminals and Electron apps.
//! 3. Clipboard paste — save clipboard, paste with ⌘V, restore clipboard.
//! 4. Clipboard only — leave the text on the clipboard and tell the user.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InsertionMethod {
    Accessibility,
    KeyEvents,
    ClipboardPaste,
    ClipboardOnly,
}

/// Apps where Accessibility writes are unreliable or bypass the app's own
/// input handling. Matched by bundle identifier prefix.
const AX_UNRELIABLE_BUNDLE_PREFIXES: &[&str] = &[
    // Terminals
    "com.apple.Terminal",
    "com.googlecode.iterm2",
    "com.mitchellh.ghostty",
    "dev.warp.",
    "net.kovidgoyal.kitty",
    "org.alacritty",
    "io.alacritty",
    "com.github.wez.wezterm",
    // Browsers (web content; controlled inputs ignore AX value changes)
    "com.apple.Safari",
    "com.google.Chrome",
    "org.chromium.",
    "com.brave.Browser",
    "company.thebrowser.",
    "com.microsoft.edgemac",
    "org.mozilla.",
    "com.operasoftware.",
    "com.vivaldi.",
    // Electron / web-view based editors and chat apps
    "com.microsoft.VSCode",
    "com.todesktop.",
    "com.tinyspeck.slackmacgap",
    "com.hnc.Discord",
    "notion.id",
    "md.obsidian",
    "com.openai.chat",
    "com.anthropic.claudefordesktop",
    "com.figma.",
    "com.linear",
    "com.microsoft.teams",
];

/// Roles of native editable text controls.
const EDITABLE_TEXT_ROLES: &[&str] = &["AXTextField", "AXTextArea", "AXSearchField", "AXComboBox"];

/// What is known about the insertion target when dictation starts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetInfo {
    pub bundle_id: Option<String>,
    /// Accessibility role of the focused element, if one was found.
    pub role: Option<String>,
    /// Whether the focused element's selected text attribute is settable.
    pub selected_text_settable: bool,
    /// Password fields must never receive AX writes or clipboard content.
    pub is_secure: bool,
}

pub fn accessibility_is_reliable(target: &TargetInfo) -> bool {
    let role_ok = target
        .role
        .as_deref()
        .is_some_and(|role| EDITABLE_TEXT_ROLES.contains(&role));
    let app_ok = !target.bundle_id.as_deref().is_some_and(|id| {
        AX_UNRELIABLE_BUNDLE_PREFIXES
            .iter()
            .any(|prefix| id.starts_with(prefix))
    });
    role_ok && app_ok && target.selected_text_settable
}

/// Methods to try, in order.
pub fn insertion_plan(target: &TargetInfo) -> Vec<InsertionMethod> {
    use InsertionMethod::*;
    if target.is_secure {
        // Typing into a password field is what the user asked for; never put
        // the text on the clipboard or read the field back.
        return vec![KeyEvents];
    }
    let mut plan = Vec::with_capacity(4);
    if accessibility_is_reliable(target) {
        plan.push(Accessibility);
    }
    plan.extend([KeyEvents, ClipboardPaste, ClipboardOnly]);
    plan
}

/// Observable state of the focused text field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldSnapshot {
    /// Identity of the focused element (e.g. its CFHash); compare only.
    pub element_id: u64,
    pub pid: i32,
    pub value: Option<String>,
    /// In UTF-16 code units.
    pub char_count: Option<i64>,
    /// Selection as (location, length) in UTF-16 code units.
    pub selection: Option<(i64, i64)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Landed {
    Yes,
    No,
    /// Not enough information; the caller must not retry (risk of doubling).
    Unknown,
}

/// Whether `inserted` landed, judged from snapshots taken before and after.
///
/// Returns [`Landed::No`] only when the same element is focused and every
/// observable property is unchanged — the one case where retrying with the
/// next method cannot duplicate text.
pub fn verify(before: &FieldSnapshot, after: &FieldSnapshot, inserted_utf16_len: usize) -> Landed {
    if before.element_id != after.element_id || before.pid != after.pid {
        return Landed::Unknown;
    }
    if let (Some(b), Some(a)) = (before.char_count, after.char_count) {
        let replaced = before.selection.map_or(0, |(_, len)| len);
        if a - b == inserted_utf16_len as i64 - replaced {
            return Landed::Yes;
        }
    }
    let value_known = before.value.is_some() && after.value.is_some();
    let count_known = before.char_count.is_some() && after.char_count.is_some();
    if !value_known && !count_known {
        return Landed::Unknown;
    }
    let unchanged = (!value_known || before.value == after.value)
        && (!count_known || before.char_count == after.char_count)
        && before.selection == after.selection;
    if unchanged {
        Landed::No
    } else {
        Landed::Unknown
    }
}

/// The character just before the caret, used to decide on a leading space.
pub fn char_before_selection(snapshot: &FieldSnapshot) -> Option<char> {
    let value = snapshot.value.as_ref()?;
    let (location, _) = snapshot.selection?;
    if location <= 0 {
        return None;
    }
    let utf16: Vec<u16> = value.encode_utf16().take(location as usize).collect();
    char::decode_utf16(
        utf16
            .iter()
            .rev()
            .copied()
            .take(2)
            .collect::<Vec<_>>()
            .into_iter()
            .rev(),
    )
    .filter_map(Result::ok)
    .last()
}

#[cfg(test)]
mod tests {
    use super::*;
    use InsertionMethod::*;

    fn target(bundle: &str, role: &str) -> TargetInfo {
        TargetInfo {
            bundle_id: Some(bundle.into()),
            role: Some(role.into()),
            selected_text_settable: true,
            is_secure: false,
        }
    }

    #[test]
    fn native_fields_use_accessibility_first() {
        let plan = insertion_plan(&target("com.apple.mail", "AXTextArea"));
        assert_eq!(
            plan,
            vec![Accessibility, KeyEvents, ClipboardPaste, ClipboardOnly]
        );
    }

    #[test]
    fn terminals_browsers_and_electron_skip_accessibility() {
        for bundle in [
            "com.apple.Terminal",
            "com.microsoft.VSCode",
            "com.todesktop.230313mzl4w4u92",
            "com.google.Chrome",
            "com.tinyspeck.slackmacgap",
            "com.openai.chat",
        ] {
            assert_eq!(
                insertion_plan(&target(bundle, "AXTextArea"))[0],
                KeyEvents,
                "{bundle}"
            );
        }
    }

    #[test]
    fn non_text_roles_and_unsettable_fields_skip_accessibility() {
        assert_eq!(
            insertion_plan(&target("com.apple.mail", "AXWebArea"))[0],
            KeyEvents
        );
        let mut t = target("com.apple.mail", "AXTextArea");
        t.selected_text_settable = false;
        assert_eq!(insertion_plan(&t)[0], KeyEvents);
        assert_eq!(insertion_plan(&TargetInfo::default())[0], KeyEvents);
    }

    #[test]
    fn secure_fields_never_touch_clipboard() {
        let mut t = target("com.apple.mail", "AXTextField");
        t.is_secure = true;
        assert_eq!(insertion_plan(&t), vec![KeyEvents]);
    }

    fn snap(value: Option<&str>, count: Option<i64>, sel: Option<(i64, i64)>) -> FieldSnapshot {
        FieldSnapshot {
            element_id: 1,
            pid: 42,
            value: value.map(Into::into),
            char_count: count,
            selection: sel,
        }
    }

    #[test]
    fn verify_detects_landed_text() {
        let before = snap(Some("ab"), Some(2), Some((2, 0)));
        let after = snap(Some("ab hi"), Some(5), Some((5, 0)));
        assert_eq!(verify(&before, &after, 3), Landed::Yes);
    }

    #[test]
    fn verify_accounts_for_replaced_selection() {
        let before = snap(Some("hello"), Some(5), Some((0, 5)));
        let after = snap(Some("yo"), Some(2), Some((2, 0)));
        assert_eq!(verify(&before, &after, 2), Landed::Yes);
    }

    #[test]
    fn verify_reports_not_landed_only_when_nothing_changed() {
        let before = snap(Some("ab"), Some(2), Some((2, 0)));
        assert_eq!(verify(&before, &before.clone(), 3), Landed::No);

        let moved = snap(Some("ab"), Some(2), Some((1, 0)));
        assert_eq!(verify(&before, &moved, 3), Landed::Unknown);

        let mut other = before.clone();
        other.element_id = 2;
        assert_eq!(verify(&before, &other, 3), Landed::Unknown);

        let blind = snap(None, None, None);
        assert_eq!(verify(&blind, &blind.clone(), 3), Landed::Unknown);
    }

    #[test]
    fn finds_char_before_caret() {
        assert_eq!(
            char_before_selection(&snap(Some("ab"), None, Some((2, 0)))),
            Some('b')
        );
        assert_eq!(
            char_before_selection(&snap(Some("ab"), None, Some((0, 0)))),
            None
        );
        assert_eq!(
            char_before_selection(&snap(Some("a👍c"), None, Some((3, 0)))),
            Some('👍')
        );
        assert_eq!(char_before_selection(&snap(None, None, Some((3, 0)))), None);
    }
}
