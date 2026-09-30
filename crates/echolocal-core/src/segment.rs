//! Where to split a long dictation so earlier parts can be transcribed while
//! the user is still speaking.
//!
//! Cuts are placed inside pauses found by the VAD, so no word is split and the
//! pieces' transcripts can simply be joined. Nothing is cut until a piece is
//! long enough to give Parakeet good context; if someone talks without
//! pausing, the piece is cut at its quietest point once it reaches a maximum
//! length, keeping the audio left for release (the "tail") bounded.

use crate::audio::VAD_FRAME_SAMPLES;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentConfig {
    /// Shortest piece worth transcribing on its own, in VAD frames.
    pub min_segment_frames: usize,
    /// Non-speech run that counts as a pause to cut in, in VAD frames.
    pub min_pause_frames: usize,
    /// Longest piece before a cut is forced, in VAD frames.
    pub max_segment_frames: usize,
}

const fn frames(ms: usize) -> usize {
    ms * 16 / VAD_FRAME_SAMPLES
}

impl Default for SegmentConfig {
    fn default() -> Self {
        Self {
            min_segment_frames: frames(8_000),
            min_pause_frames: frames(300),
            max_segment_frames: frames(20_000),
        }
    }
}

/// The frame index to cut at for the piece starting at frame `start`, given
/// the speech flags seen so far; `None` while no cut is possible yet.
///
/// The cut lands half the minimum pause into the first qualifying pause that
/// starts late enough, so the piece ends in silence and the next piece begins
/// in silence.
pub fn find_cut(flags: &[bool], start: usize, cfg: &SegmentConfig) -> Option<usize> {
    let earliest = start + cfg.min_segment_frames;
    let latest = start + cfg.max_segment_frames;
    if flags.len() < earliest {
        return None;
    }
    let half_pause = cfg.min_pause_frames.max(1) / 2;

    let mut run_start = start;
    let mut run_len = 0usize;
    for (i, &speech) in flags
        .iter()
        .enumerate()
        .take(latest.min(flags.len()))
        .skip(start)
    {
        if speech {
            run_len = 0;
            continue;
        }
        if run_len == 0 {
            run_start = i;
        }
        run_len += 1;
        if run_len >= cfg.min_pause_frames.max(1) && run_start + half_pause >= earliest {
            return Some(run_start + half_pause);
        }
    }

    if flags.len() < latest {
        return None;
    }
    // No real pause within the maximum length: cut in the longest silence
    // after the earliest point, or hard-cut at the maximum.
    let mut best: Option<(usize, usize)> = None; // (run_start, run_len)
    let mut run_start = earliest;
    let mut run_len = 0usize;
    for (i, &speech) in flags.iter().enumerate().take(latest).skip(earliest) {
        if speech {
            run_len = 0;
            continue;
        }
        if run_len == 0 {
            run_start = i;
        }
        run_len += 1;
        if best.is_none_or(|(_, len)| run_len > len) {
            best = Some((run_start, run_len));
        }
    }
    Some(best.map_or(latest, |(s, len)| s + len / 2))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> SegmentConfig {
        SegmentConfig {
            min_segment_frames: 100,
            min_pause_frames: 10,
            max_segment_frames: 300,
        }
    }

    fn flags(spans: &[(bool, usize)]) -> Vec<bool> {
        spans
            .iter()
            .flat_map(|&(v, n)| std::iter::repeat_n(v, n))
            .collect()
    }

    #[test]
    fn waits_for_minimum_length() {
        let f = flags(&[(true, 50), (false, 20), (true, 20)]);
        assert_eq!(find_cut(&f, 0, &cfg()), None);
    }

    #[test]
    fn cuts_inside_first_pause_after_minimum() {
        // A pause before the minimum is ignored; the one after it is used.
        let f = flags(&[(true, 50), (false, 20), (true, 60), (false, 15), (true, 10)]);
        assert_eq!(find_cut(&f, 0, &cfg()), Some(130 + 5));
    }

    #[test]
    fn short_pauses_do_not_count() {
        let f = flags(&[(true, 120), (false, 5), (true, 50)]);
        assert_eq!(find_cut(&f, 0, &cfg()), None);
    }

    #[test]
    fn cuts_as_soon_as_pause_is_long_enough() {
        // The pause is still going on; the cut doesn't wait for it to end.
        let f = flags(&[(true, 120), (false, 10)]);
        assert_eq!(find_cut(&f, 0, &cfg()), Some(125));
    }

    #[test]
    fn pause_straddling_minimum_is_usable_when_cut_lands_after_it() {
        let f = flags(&[(true, 98), (false, 12)]);
        // Cut would land at 103 >= 100.
        assert_eq!(find_cut(&f, 0, &cfg()), Some(103));
    }

    #[test]
    fn respects_start_offset() {
        let f = flags(&[(true, 200), (false, 10), (true, 150), (false, 10)]);
        assert_eq!(find_cut(&f, 0, &cfg()), Some(205));
        assert_eq!(find_cut(&f, 205, &cfg()), Some(360 + 5));
    }

    #[test]
    fn forces_cut_at_longest_silence_when_nobody_pauses() {
        let f = flags(&[(true, 150), (false, 3), (true, 50), (false, 6), (true, 200)]);
        assert_eq!(find_cut(&f, 0, &cfg()), Some(203 + 3));
    }

    #[test]
    fn hard_cut_at_maximum_without_any_silence() {
        let f = flags(&[(true, 299)]);
        assert_eq!(find_cut(&f, 0, &cfg()), None);
        let f = flags(&[(true, 400)]);
        assert_eq!(find_cut(&f, 0, &cfg()), Some(300));
    }

    #[test]
    fn default_config_in_frames() {
        let c = SegmentConfig::default();
        assert_eq!(c.min_segment_frames, 266);
        assert_eq!(c.min_pause_frames, 10);
        assert_eq!(c.max_segment_frames, 666);
    }
}
