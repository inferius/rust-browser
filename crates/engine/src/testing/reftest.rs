//! Reference rendering tests - visual diff.
//!
//! Pair: test HTML + reference HTML. Render obou do PNG, pixel diff s tolerance.
//! Pri identical visual output (within epsilon) = pass.
//!
//! Format reftests v WPT: HTML s `<link rel=match href="ref.html">`. Tu cestu
//! parse + render obojiho + diff.
//!
//! Foundation impl: PNG diff utility. Real wire pres render pipeline = next session.
//!
//! Inspired by:
//! - WPT reftest harness
//! - Servo `tests/reftest.rs`
//! - Chromium `third_party/blink/tools/blinkpy/web_tests/run_web_tests.py`

/// Diff dvou RGBA buffers - vrati (pixels_different, max_channel_diff).
/// `tolerance_channel`: max allowed per-channel diff (typicky 1-3 pro
/// sub-pixel rasterization noise).
pub fn pixel_diff(
    a: &[u8],
    b: &[u8],
    width: u32,
    height: u32,
    tolerance_channel: u8,
) -> (u32, u8) {
    assert_eq!(a.len(), b.len(), "buffer size mismatch");
    let mut diff_count = 0u32;
    let mut max_diff = 0u8;
    for i in 0..(width as usize * height as usize) {
        let off = i * 4;
        let mut pixel_diff = 0u8;
        for c in 0..4 {
            let d = a[off + c].abs_diff(b[off + c]);
            if d > pixel_diff { pixel_diff = d; }
        }
        if pixel_diff > tolerance_channel {
            diff_count += 1;
        }
        if pixel_diff > max_diff { max_diff = pixel_diff; }
    }
    (diff_count, max_diff)
}

/// Reftest result - pass kdyz < threshold pixels differ.
#[derive(Debug, Clone, PartialEq)]
pub enum ReftestStatus {
    Pass,
    Fail { diff_pixels: u32, max_diff: u8 },
}

pub fn compare_buffers(
    a: &[u8],
    b: &[u8],
    width: u32,
    height: u32,
    max_diff_pixels: u32,
    tolerance_channel: u8,
) -> ReftestStatus {
    let (diff_pixels, max_diff) = pixel_diff(a, b, width, height, tolerance_channel);
    if diff_pixels <= max_diff_pixels {
        ReftestStatus::Pass
    } else {
        ReftestStatus::Fail { diff_pixels, max_diff }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_buffers_pass() {
        let a = vec![255u8; 4 * 10 * 10];
        let b = vec![255u8; 4 * 10 * 10];
        let r = compare_buffers(&a, &b, 10, 10, 0, 0);
        assert_eq!(r, ReftestStatus::Pass);
    }

    #[test]
    fn small_diff_within_tolerance() {
        let a = vec![255u8; 4 * 10 * 10];
        let mut b = vec![255u8; 4 * 10 * 10];
        // Single pixel off by 1.
        b[0] = 254;
        let r = compare_buffers(&a, &b, 10, 10, 0, 2); // tolerance 2
        assert_eq!(r, ReftestStatus::Pass);
    }

    #[test]
    fn large_diff_fail() {
        let a = vec![255u8; 4 * 10 * 10];
        let mut b = vec![255u8; 4 * 10 * 10];
        // 5 pixels totally different (channel diff = 255).
        for i in 0..5 { b[i * 4] = 0; }
        let r = compare_buffers(&a, &b, 10, 10, 2, 1);
        assert!(matches!(r, ReftestStatus::Fail { .. }));
    }

    #[test]
    fn pixel_diff_max() {
        let a = vec![255u8; 4 * 5 * 5];
        let mut b = vec![255u8; 4 * 5 * 5];
        b[0] = 100;
        let (count, max) = pixel_diff(&a, &b, 5, 5, 0);
        assert_eq!(count, 1);
        assert_eq!(max, 155);
    }
}
