//! img / iframe lazy loading - load when near viewport.
//!
//! Spec: https://html.spec.whatwg.org/multipage/urls-and-fetching.html#lazy-loading-attributes

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LoadingMode {
    Eager,
    Lazy,
}

impl LoadingMode {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "lazy" => Self::Lazy,
            _ => Self::Eager,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FetchPriority {
    Auto,
    High,
    Low,
}

impl FetchPriority {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "high" => Self::High,
            "low" => Self::Low,
            _ => Self::Auto,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LazyResource {
    pub id: u64,
    pub url: String,
    pub loading: LoadingMode,
    pub priority: FetchPriority,
    pub rect: (f32, f32, f32, f32),
    pub triggered: bool,
}

#[derive(Default)]
pub struct LazyLoadScheduler {
    pub resources: HashMap<u64, LazyResource>,
    /// Distance from viewport to trigger preload (CSS px).
    pub root_margin_px: f32,
}

impl LazyLoadScheduler {
    pub fn new() -> Self {
        Self { root_margin_px: 1250.0, ..Self::default() }
    }

    pub fn register(&mut self, res: LazyResource) {
        self.resources.insert(res.id, res);
    }

    /// Given viewport rect, return resource ids that should fire load now.
    pub fn check(&mut self, viewport: (f32, f32, f32, f32)) -> Vec<u64> {
        let margin = self.root_margin_px;
        let expanded = (
            viewport.0 - margin,
            viewport.1 - margin,
            viewport.2 + margin * 2.0,
            viewport.3 + margin * 2.0,
        );
        let mut fired = Vec::new();
        for r in self.resources.values_mut() {
            if r.triggered { continue; }
            if r.loading == LoadingMode::Eager || intersects(r.rect, expanded) {
                r.triggered = true;
                fired.push(r.id);
            }
        }
        fired
    }
}

fn intersects(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> bool {
    a.0 < b.0 + b.2 && a.0 + a.2 > b.0
    && a.1 < b.1 + b.3 && a.1 + a.3 > b.1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn res(id: u64, x: f32, y: f32, lazy: bool) -> LazyResource {
        LazyResource {
            id, url: "u".into(),
            loading: if lazy { LoadingMode::Lazy } else { LoadingMode::Eager },
            priority: FetchPriority::Auto,
            rect: (x, y, 100.0, 100.0),
            triggered: false,
        }
    }

    #[test]
    fn parse_loading() {
        assert_eq!(LoadingMode::parse("lazy"), LoadingMode::Lazy);
        assert_eq!(LoadingMode::parse("eager"), LoadingMode::Eager);
    }

    #[test]
    fn eager_fires_immediately() {
        let mut s = LazyLoadScheduler::new();
        s.register(res(1, 10_000.0, 10_000.0, false));
        let fired = s.check((0.0, 0.0, 100.0, 100.0));
        assert_eq!(fired, vec![1]);
    }

    #[test]
    fn lazy_inside_viewport_fires() {
        let mut s = LazyLoadScheduler::new();
        s.register(res(1, 50.0, 50.0, true));
        let fired = s.check((0.0, 0.0, 800.0, 600.0));
        assert_eq!(fired, vec![1]);
    }

    #[test]
    fn lazy_outside_does_not_fire() {
        let mut s = LazyLoadScheduler::new();
        s.register(res(1, 5000.0, 5000.0, true));
        let fired = s.check((0.0, 0.0, 800.0, 600.0));
        assert!(fired.is_empty());
    }

    #[test]
    fn lazy_within_margin_fires() {
        let mut s = LazyLoadScheduler::new();
        s.root_margin_px = 500.0;
        s.register(res(1, 900.0, 0.0, true));
        // viewport ends at 800; with margin 500 reaches 1300 -> hits.
        let fired = s.check((0.0, 0.0, 800.0, 600.0));
        assert_eq!(fired, vec![1]);
    }

    #[test]
    fn triggered_does_not_refire() {
        let mut s = LazyLoadScheduler::new();
        s.register(res(1, 50.0, 50.0, true));
        s.check((0.0, 0.0, 800.0, 600.0));
        let fired = s.check((0.0, 0.0, 800.0, 600.0));
        assert!(fired.is_empty());
    }
}
