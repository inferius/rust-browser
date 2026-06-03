//! CSS Scroll Snap.
//!
//! Spec: https://www.w3.org/TR/css-scroll-snap-1/

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollSnapType {
    None,
    X,
    Y,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollSnapStrictness {
    Proximity,
    Mandatory,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollSnapAlign {
    None,
    Start,
    Center,
    End,
}

#[derive(Debug, Clone, Copy)]
pub struct SnapTarget {
    pub start: f32,         // inset-start coordinate
    pub end: f32,
    pub align: ScrollSnapAlign,
}

#[derive(Debug, Clone)]
pub struct ScrollSnapContainer {
    pub snap_type: ScrollSnapType,
    pub strictness: ScrollSnapStrictness,
    pub targets_y: Vec<SnapTarget>,
    pub targets_x: Vec<SnapTarget>,
    pub viewport_size: (f32, f32),
}

impl ScrollSnapContainer {
    pub fn new() -> Self {
        Self {
            snap_type: ScrollSnapType::None,
            strictness: ScrollSnapStrictness::Proximity,
            targets_y: Vec::new(),
            targets_x: Vec::new(),
            viewport_size: (0.0, 0.0),
        }
    }

    /// Find nearest snap position for a given scroll offset.
    pub fn snap_y(&self, scroll_y: f32, viewport_h: f32) -> Option<f32> {
        if !matches!(self.snap_type, ScrollSnapType::Y | ScrollSnapType::Both) { return None; }
        if self.targets_y.is_empty() { return None; }

        let mut best: Option<(f32, f32)> = None;
        for t in &self.targets_y {
            let snap_pos = match t.align {
                ScrollSnapAlign::Start => t.start,
                ScrollSnapAlign::End => t.end - viewport_h,
                ScrollSnapAlign::Center => t.start + (t.end - t.start) / 2.0 - viewport_h / 2.0,
                ScrollSnapAlign::None => continue,
            };
            let dist = (snap_pos - scroll_y).abs();
            if best.map(|(_, d)| dist < d).unwrap_or(true) {
                best = Some((snap_pos, dist));
            }
        }
        let (pos, dist) = best?;
        // Proximity: only snap if close.
        if self.strictness == ScrollSnapStrictness::Proximity && dist > viewport_h / 2.0 {
            return None;
        }
        Some(pos)
    }
}

impl Default for ScrollSnapContainer { fn default() -> Self { Self::new() } }

#[cfg(test)]
mod tests {
    use super::*;

    fn container() -> ScrollSnapContainer {
        let mut c = ScrollSnapContainer::new();
        c.snap_type = ScrollSnapType::Y;
        c.strictness = ScrollSnapStrictness::Mandatory;
        c.targets_y = vec![
            SnapTarget { start: 0.0, end: 600.0, align: ScrollSnapAlign::Start },
            SnapTarget { start: 600.0, end: 1200.0, align: ScrollSnapAlign::Start },
            SnapTarget { start: 1200.0, end: 1800.0, align: ScrollSnapAlign::Start },
        ];
        c
    }

    #[test]
    fn snaps_to_nearest() {
        let c = container();
        let pos = c.snap_y(450.0, 600.0).unwrap();
        // 450 between 0 and 600 -> closer to 600
        assert_eq!(pos, 600.0);
    }

    #[test]
    fn no_snap_when_proximity_far() {
        let mut c = container();
        c.strictness = ScrollSnapStrictness::Proximity;
        c.targets_y = vec![SnapTarget { start: 0.0, end: 600.0, align: ScrollSnapAlign::Start }];
        // far away
        assert!(c.snap_y(5000.0, 600.0).is_none());
    }

    #[test]
    fn snap_none_returns_none() {
        let c = ScrollSnapContainer::new();
        assert!(c.snap_y(0.0, 600.0).is_none());
    }

    #[test]
    fn align_center() {
        let mut c = ScrollSnapContainer::new();
        c.snap_type = ScrollSnapType::Y;
        c.strictness = ScrollSnapStrictness::Mandatory;
        c.targets_y = vec![
            SnapTarget { start: 100.0, end: 300.0, align: ScrollSnapAlign::Center },
        ];
        // center = 200, vp/2 = 50 -> snap = 150
        let pos = c.snap_y(0.0, 100.0).unwrap();
        assert_eq!(pos, 150.0);
    }

    #[test]
    fn align_end() {
        let mut c = ScrollSnapContainer::new();
        c.snap_type = ScrollSnapType::Y;
        c.strictness = ScrollSnapStrictness::Mandatory;
        c.targets_y = vec![
            SnapTarget { start: 0.0, end: 500.0, align: ScrollSnapAlign::End },
        ];
        // end - vp = 500 - 100 = 400
        let pos = c.snap_y(0.0, 100.0).unwrap();
        assert_eq!(pos, 400.0);
    }
}
