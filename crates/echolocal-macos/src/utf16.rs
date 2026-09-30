/// Split UTF-16 text into chunks of at most `max` units that never separate
/// a surrogate pair.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn utf16_chunks(units: &[u16], max: usize) -> Vec<&[u16]> {
    let max = max.max(2);
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < units.len() {
        let mut end = (start + max).min(units.len());
        if end < units.len() && (0xD800..0xDC00).contains(&units[end - 1]) {
            end -= 1;
        }
        chunks.push(&units[start..end]);
        start = end;
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_respect_surrogates() {
        let text = "a".repeat(19) + "👍" + "b";
        let units: Vec<u16> = text.encode_utf16().collect();
        let chunks = utf16_chunks(&units, 20);
        assert_eq!(chunks[0].len(), 19);
        assert_eq!(chunks[1].len(), 3);
        assert_eq!(chunks.concat(), units);
    }

    #[test]
    fn chunks_cover_everything() {
        let text = "Grüße aus Köln 👋 — ".repeat(10);
        let units: Vec<u16> = text.encode_utf16().collect();
        let chunks = utf16_chunks(&units, 20);
        assert!(chunks.iter().all(|c| c.len() <= 20 && !c.is_empty()));
        assert_eq!(String::from_utf16(&chunks.concat()).unwrap(), text);
        assert!(utf16_chunks(&[], 20).is_empty());
    }
}
