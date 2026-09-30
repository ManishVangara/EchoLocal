//! Push-to-talk shortcut recording and display.
//!
//! Shortcuts are stored as strings in the format of the `handy-keys` crate
//! the app uses for global hotkeys: modifier names (`Cmd`, `Opt`, `Ctrl`,
//! `Shift`, `Fn`, or side-specific `OptRight`, `CmdLeft`, …) and an optional
//! key joined with `+`, e.g. `OptRight` or `Ctrl+Opt+Space`. Holding a single
//! modifier (like Right ⌥) is a valid shortcut.

/// Modifier bits, laid out exactly like `handy_keys::Modifiers`.
pub mod mods {
    pub const CMD_LEFT: u32 = 1 << 0;
    pub const SHIFT_LEFT: u32 = 1 << 1;
    pub const CTRL_LEFT: u32 = 1 << 2;
    pub const OPT_LEFT: u32 = 1 << 3;
    pub const FN: u32 = 1 << 4;
    pub const CMD_RIGHT: u32 = 1 << 5;
    pub const SHIFT_RIGHT: u32 = 1 << 6;
    pub const CTRL_RIGHT: u32 = 1 << 7;
    pub const OPT_RIGHT: u32 = 1 << 8;

    pub const CMD: u32 = CMD_LEFT | CMD_RIGHT;
    pub const SHIFT: u32 = SHIFT_LEFT | SHIFT_RIGHT;
    pub const CTRL: u32 = CTRL_LEFT | CTRL_RIGHT;
    pub const OPT: u32 = OPT_LEFT | OPT_RIGHT;

    /// Make every side-specific modifier match either side ("either ⌥").
    pub fn either_side(bits: u32) -> u32 {
        let mut out = bits & FN;
        for group in [CMD, SHIFT, CTRL, OPT] {
            if bits & group != 0 {
                out |= group;
            }
        }
        out
    }
}

/// Outcome of feeding one key event to a [`ShortcutCapture`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureStep<K> {
    /// Still recording; `modifiers` are held right now (for live display).
    Pending { modifiers: u32 },
    /// A shortcut was chosen.
    Done { modifiers: u32, key: Option<K> },
    /// Escape pressed on its own: keep the old shortcut.
    Cancelled,
}

/// "Press the shortcut you want" logic, independent of the key library.
///
/// - A key pressed with modifiers held → that combination, matching either
///   side of each modifier (⌥Space works with both ⌥ keys).
/// - Modifiers pressed and released with no other key → a modifier-only
///   shortcut that keeps the exact side (Right ⌥ alone).
/// - Escape alone → cancel.
#[derive(Debug, Default)]
pub struct ShortcutCapture {
    peak: u32,
}

impl ShortcutCapture {
    pub fn new() -> Self {
        Self::default()
    }

    /// `modifiers`: modifiers held after this event. `key`: the non-modifier
    /// key of this event, if any. `is_escape`: whether that key is Escape.
    pub fn on_event<K>(
        &mut self,
        modifiers: u32,
        key: Option<K>,
        key_down: bool,
        is_escape: bool,
    ) -> CaptureStep<K> {
        if let Some(key) = key {
            if !key_down {
                return CaptureStep::Pending { modifiers };
            }
            if is_escape && modifiers == 0 {
                return CaptureStep::Cancelled;
            }
            let modifiers = mods::either_side(modifiers);
            self.peak = 0;
            return CaptureStep::Done {
                modifiers,
                key: Some(key),
            };
        }
        // A modifier went down or up.
        self.peak |= modifiers;
        if modifiers == 0 && self.peak != 0 {
            let chosen = self.peak;
            self.peak = 0;
            return CaptureStep::Done {
                modifiers: chosen,
                key: None,
            };
        }
        CaptureStep::Pending { modifiers }
    }
}

/// Reject shortcuts that would break normal typing: a key without modifiers
/// is only allowed for function keys (F1–F24).
pub fn check_combo(modifiers: u32, key: Option<&str>) -> Result<(), &'static str> {
    match key {
        None if modifiers == 0 => Err("Press a key or hold a modifier"),
        None => Ok(()),
        Some(key) if modifiers == 0 => {
            let is_function_key = key.len() >= 2
                && key.starts_with('F')
                && key[1..].chars().all(|c| c.is_ascii_digit());
            if is_function_key {
                Ok(())
            } else {
                Err("Add a modifier (⌃ ⌥ ⇧ ⌘ fn), use a function key, or hold a single modifier")
            }
        }
        Some(_) => Ok(()),
    }
}

fn modifier_label(token: &str) -> Option<&'static str> {
    Some(match token.to_ascii_lowercase().replace('_', "").as_str() {
        "cmd" | "command" | "super" | "meta" | "cmdorctrl" | "commandorcontrol" => "⌘",
        "cmdleft" | "commandleft" | "lcmd" => "Left ⌘",
        "cmdright" | "commandright" | "rcmd" => "Right ⌘",
        "opt" | "option" | "alt" => "⌥",
        "optleft" | "optionleft" | "altleft" | "lopt" | "loption" => "Left ⌥",
        "optright" | "optionright" | "altright" | "ropt" | "roption" => "Right ⌥",
        "ctrl" | "control" => "⌃",
        "ctrlleft" | "controlleft" | "lctrl" => "Left ⌃",
        "ctrlright" | "controlright" | "rctrl" => "Right ⌃",
        "shift" => "⇧",
        "shiftleft" | "lshift" => "Left ⇧",
        "shiftright" | "rshift" => "Right ⇧",
        "fn" | "function" => "fn",
        _ => return None,
    })
}

fn key_label(token: &str) -> String {
    match token.to_ascii_lowercase().as_str() {
        "space" => "Space".into(),
        "return" | "enter" => "↩".into(),
        "tab" => "⇥".into(),
        "escape" | "esc" => "⎋".into(),
        "delete" | "backspace" => "⌫".into(),
        "forwarddelete" => "⌦".into(),
        "left" => "←".into(),
        "right" => "→".into(),
        "up" => "↑".into(),
        "down" => "↓".into(),
        _ if token.chars().count() == 1 => token.to_uppercase(),
        _ => token.to_string(),
    }
}

/// Human-readable label, e.g. `OptRight` → `Right ⌥`, `Ctrl+Opt+Space` → `⌃ ⌥ Space`.
pub fn shortcut_label(shortcut: &str) -> String {
    let parts: Vec<String> = shortcut
        .split('+')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| modifier_label(p).map_or_else(|| key_label(p), str::to_string))
        .collect();
    if parts.is_empty() {
        "Not set".into()
    } else {
        parts.join(" ")
    }
}

/// Whether the shortcut consists of modifiers only (e.g. hold Right ⌥).
pub fn is_modifier_only(shortcut: &str) -> bool {
    let mut parts = shortcut
        .split('+')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .peekable();
    parts.peek().is_some() && parts.all(|p| modifier_label(p).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mods::*;

    #[test]
    fn labels() {
        assert_eq!(shortcut_label("OptRight"), "Right ⌥");
        assert_eq!(shortcut_label("Alt+Space"), "⌥ Space");
        assert_eq!(shortcut_label("Ctrl+Opt+Cmd+k"), "⌃ ⌥ ⌘ K");
        assert_eq!(shortcut_label("Fn"), "fn");
        assert_eq!(shortcut_label("F5"), "F5");
        assert_eq!(shortcut_label("Shift+Return"), "⇧ ↩");
        assert_eq!(shortcut_label(""), "Not set");
    }

    #[test]
    fn modifier_only_detection() {
        assert!(is_modifier_only("OptRight"));
        assert!(is_modifier_only("Ctrl+Opt"));
        assert!(is_modifier_only("Fn"));
        assert!(!is_modifier_only("Alt+Space"));
        assert!(!is_modifier_only("F5"));
        assert!(!is_modifier_only(""));
    }

    #[test]
    fn either_side_widens_each_group() {
        assert_eq!(either_side(OPT_RIGHT), OPT);
        assert_eq!(either_side(CMD_LEFT | SHIFT_RIGHT | FN), CMD | SHIFT | FN);
        assert_eq!(either_side(0), 0);
    }

    #[test]
    fn captures_key_combo_matching_either_side() {
        let mut c = ShortcutCapture::new();
        assert_eq!(
            c.on_event::<&str>(OPT_RIGHT, None, true, false),
            CaptureStep::Pending {
                modifiers: OPT_RIGHT
            }
        );
        assert_eq!(
            c.on_event(OPT_RIGHT, Some("Space"), true, false),
            CaptureStep::Done {
                modifiers: OPT,
                key: Some("Space")
            }
        );
    }

    #[test]
    fn captures_single_modifier_with_its_side() {
        let mut c = ShortcutCapture::new();
        c.on_event::<&str>(OPT_RIGHT, None, true, false);
        assert_eq!(
            c.on_event::<&str>(0, None, false, false),
            CaptureStep::Done {
                modifiers: OPT_RIGHT,
                key: None
            }
        );
    }

    #[test]
    fn captures_modifier_chord_at_its_peak() {
        let mut c = ShortcutCapture::new();
        c.on_event::<&str>(CTRL_LEFT, None, true, false);
        c.on_event::<&str>(CTRL_LEFT | OPT_LEFT, None, true, false);
        c.on_event::<&str>(OPT_LEFT, None, false, false);
        assert_eq!(
            c.on_event::<&str>(0, None, false, false),
            CaptureStep::Done {
                modifiers: CTRL_LEFT | OPT_LEFT,
                key: None
            }
        );
    }

    #[test]
    fn plain_key_and_escape() {
        let mut c = ShortcutCapture::new();
        assert_eq!(
            c.on_event(0, Some("F5"), true, false),
            CaptureStep::Done {
                modifiers: 0,
                key: Some("F5")
            }
        );
        assert_eq!(
            c.on_event(0, Some("Escape"), true, true),
            CaptureStep::Cancelled::<&str>
        );
        // Escape with a modifier is a real combination.
        assert_eq!(
            c.on_event(CTRL_LEFT, Some("Escape"), true, true),
            CaptureStep::Done {
                modifiers: CTRL,
                key: Some("Escape")
            }
        );
    }

    #[test]
    fn combo_rules() {
        assert!(check_combo(OPT, Some("Space")).is_ok());
        assert!(check_combo(OPT_RIGHT, None).is_ok());
        assert!(check_combo(0, Some("F5")).is_ok());
        assert!(check_combo(0, Some("F13")).is_ok());
        assert!(check_combo(0, Some("A")).is_err());
        assert!(check_combo(0, Some("Space")).is_err());
        assert!(check_combo(0, Some("F")).is_err());
        assert!(check_combo(0, None).is_err());
    }

    #[test]
    fn key_up_is_ignored() {
        let mut c = ShortcutCapture::new();
        assert_eq!(
            c.on_event(0, Some("A"), false, false),
            CaptureStep::Pending { modifiers: 0 }
        );
    }
}
