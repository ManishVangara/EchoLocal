//! Transcript normalization before insertion.

/// Trim and collapse whitespace. Returns an empty string for transcripts that
/// contain no letters or digits (e.g. a lone "." from background noise).
pub fn normalize_transcript(raw: &str) -> String {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().any(char::is_alphanumeric) {
        collapsed
    } else {
        String::new()
    }
}

/// Add a separating space when inserting right after a word or sentence, so
/// consecutive dictations don't run together ("Hello.World").
pub fn join_with_preceding(preceding: Option<char>, text: &str) -> String {
    let needs_space = match (preceding, text.chars().next()) {
        (Some(prev), Some(first)) => {
            !prev.is_whitespace()
                && !matches!(prev, '(' | '[' | '{' | '"' | '\'' | '/' | '@' | '#' | '-')
                && !matches!(first, '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}')
        }
        _ => false,
    };
    if needs_space {
        format!(" {text}")
    } else {
        text.to_string()
    }
}

/// Length in UTF-16 code units, the unit macOS text APIs count in.
pub fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_whitespace() {
        assert_eq!(normalize_transcript("  hello \n  world  "), "hello world");
    }

    #[test]
    fn drops_punctuation_only_output() {
        assert_eq!(normalize_transcript(" . "), "");
        assert_eq!(normalize_transcript(""), "");
        assert_eq!(normalize_transcript("Ok."), "Ok.");
        assert_eq!(normalize_transcript("日本"), "日本");
    }

    #[test]
    fn joins_with_context() {
        assert_eq!(join_with_preceding(None, "Hi."), "Hi.");
        assert_eq!(join_with_preceding(Some(' '), "Hi."), "Hi.");
        assert_eq!(join_with_preceding(Some('.'), "Next."), " Next.");
        assert_eq!(join_with_preceding(Some('d'), "and more"), " and more");
        assert_eq!(join_with_preceding(Some('('), "aside"), "aside");
        assert_eq!(join_with_preceding(Some('d'), ", then"), ", then");
        assert_eq!(join_with_preceding(Some('\n'), "Line"), "Line");
    }

    #[test]
    fn counts_utf16() {
        assert_eq!(utf16_len("héllo"), 5);
        assert_eq!(utf16_len("👍"), 2);
    }
}
