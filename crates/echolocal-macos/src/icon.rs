/// Index of the image size to use: the smallest at least `min` wide, or the
/// largest available if all are smaller. `None` if there are none.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn pick_size(widths: &[isize], min: isize) -> Option<usize> {
    let big_enough = widths
        .iter()
        .enumerate()
        .filter(|(_, &w)| w >= min)
        .min_by_key(|(_, &w)| w)
        .map(|(i, _)| i);
    big_enough.or_else(|| {
        widths
            .iter()
            .enumerate()
            .max_by_key(|(_, &w)| w)
            .map(|(i, _)| i)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_smallest_sufficient_size() {
        assert_eq!(pick_size(&[16, 32, 64, 128, 1024], 64), Some(2));
        assert_eq!(pick_size(&[1024, 128, 64], 100), Some(1));
    }

    #[test]
    fn falls_back_to_largest() {
        assert_eq!(pick_size(&[16, 32], 64), Some(1));
        assert_eq!(pick_size(&[], 64), None);
    }
}
