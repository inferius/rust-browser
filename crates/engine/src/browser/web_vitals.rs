//! Core Web Vitals - LCP, FID/INP, CLS measurement.
//!
//! Spec: https://web.dev/vitals/
//! Surfaced via PerformanceObserver entries (largest-contentful-paint, layout-shift, ...).

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WebVital {
    LCP,         // largest contentful paint
    INP,         // interaction to next paint (replaces FID)
    FID,         // first input delay (legacy)
    CLS,         // cumulative layout shift
    TTFB,        // time to first byte
    FCP,         // first contentful paint
}

#[derive(Debug, Clone)]
pub struct LcpEntry {
    pub element_id: Option<u64>,
    pub url: Option<String>,
    pub start_time_ms: f64,
    pub render_time_ms: Option<f64>,
    pub size_px: f64,                // computed area
}

#[derive(Debug, Clone)]
pub struct LayoutShiftEntry {
    pub value: f64,                  // shift score
    pub had_recent_input: bool,
    pub sources: Vec<u64>,           // element ids
    pub start_time_ms: f64,
}

#[derive(Debug, Clone)]
pub struct InteractionEntry {
    pub interaction_type: String,    // "click" | "keydown" | ...
    pub start_time_ms: f64,
    pub processing_start_ms: f64,
    pub processing_end_ms: f64,
    pub presentation_time_ms: f64,
}

#[derive(Default)]
pub struct WebVitalsCollector {
    pub lcp_candidates: Vec<LcpEntry>,
    pub layout_shifts: Vec<LayoutShiftEntry>,
    pub interactions: Vec<InteractionEntry>,
    pub ttfb_ms: Option<f64>,
    pub fcp_ms: Option<f64>,
}

impl WebVitalsCollector {
    pub fn new() -> Self { Self::default() }

    pub fn record_lcp(&mut self, entry: LcpEntry) {
        self.lcp_candidates.push(entry);
    }

    /// Real integration: scan display commands, find candidate LCP elements
    /// (Image / ImageFit), record po render. `now_ms` je doba od navigation
    /// start.
    ///
    /// Per W3C LCP spec: candidate elements = <img>, <image> inside <svg>,
    /// poster of <video>, background-image, block-level text.
    /// LCP = candidate with largest paint area at last paint.
    pub fn collect_from_paint(&mut self, commands: &[crate::browser::paint::DisplayCommand], now_ms: f64) {
        use crate::browser::paint::DisplayCommand;
        for cmd in commands {
            match cmd {
                DisplayCommand::Image { w, h, src, .. } => {
                    let area = (*w as f64) * (*h as f64);
                    if area >= 100.0 {
                        self.record_lcp(LcpEntry {
                            element_id: None,
                            url: Some(src.clone()),
                            start_time_ms: now_ms,
                            render_time_ms: Some(now_ms),
                            size_px: area,
                        });
                    }
                }
                DisplayCommand::ImageFit { w, h, src, .. } => {
                    let area = (*w as f64) * (*h as f64);
                    if area >= 100.0 {
                        self.record_lcp(LcpEntry {
                            element_id: None,
                            url: Some(src.clone()),
                            start_time_ms: now_ms,
                            render_time_ms: Some(now_ms),
                            size_px: area,
                        });
                    }
                }
                _ => {}
            }
        }
    }

    /// Final LCP = largest candidate (by area).
    pub fn lcp(&self) -> Option<&LcpEntry> {
        self.lcp_candidates.iter().max_by(|a, b| a.size_px.partial_cmp(&b.size_px).unwrap())
    }

    /// Real CLS feed: compare rects mezi dvema vrstvami layouts (previous vs current),
    /// detect movements, compute shift score per W3C spec.
    ///
    /// Spec: https://web.dev/cls/
    /// shift_score = impact_fraction * distance_fraction
    /// impact_fraction = union(start_visual_area, end_visual_area) / viewport_area
    /// distance_fraction = max(|dx|, |dy|) / max(viewport_w, viewport_h)
    ///
    /// Vraci pocet detekovanych shifts. Caller predava `had_recent_input` aby
    /// shift se zapocital jen kdyz NEni user-triggered (per spec).
    pub fn feed_layout_shift(
        &mut self,
        previous_rects: &std::collections::HashMap<u64, (f32, f32, f32, f32)>,
        current_rects: &std::collections::HashMap<u64, (f32, f32, f32, f32)>,
        viewport_w: f32,
        viewport_h: f32,
        had_recent_input: bool,
        now_ms: f64,
    ) -> u32 {
        let mut count = 0;
        let vp_area = viewport_w * viewport_h;
        let vp_max = viewport_w.max(viewport_h);
        if vp_area <= 0.0 || vp_max <= 0.0 { return 0; }
        let mut total_shift_score = 0.0_f64;
        let mut sources: Vec<u64> = Vec::new();
        let mut max_dx = 0.0_f32;
        let mut max_dy = 0.0_f32;
        for (id, prev) in previous_rects.iter() {
            let Some(curr) = current_rects.get(id) else { continue; };
            let dx = curr.0 - prev.0;
            let dy = curr.1 - prev.1;
            // Threshold per spec: shifts <= 3px nepocitame.
            if dx.abs() < 3.0 && dy.abs() < 3.0 { continue; }
            // Impact: visual union area inside viewport / viewport_area.
            let union = visual_union_area(prev, curr, viewport_w, viewport_h);
            let impact_fraction = (union / vp_area) as f64;
            // Distance fraction.
            let distance = dx.abs().max(dy.abs());
            let distance_fraction = (distance / vp_max) as f64;
            let shift = impact_fraction * distance_fraction;
            if shift > 0.0 {
                total_shift_score += shift;
                sources.push(*id);
                if dx.abs() > max_dx.abs() { max_dx = dx; }
                if dy.abs() > max_dy.abs() { max_dy = dy; }
                count += 1;
            }
        }
        if total_shift_score > 0.0 {
            self.record_layout_shift(LayoutShiftEntry {
                value: total_shift_score,
                had_recent_input,
                sources,
                start_time_ms: now_ms,
            });
        }
        count
    }

    pub fn record_layout_shift(&mut self, entry: LayoutShiftEntry) {
        self.layout_shifts.push(entry);
    }

    /// CLS = sum of qualifying layout shift scores in session windows.
    /// Simplified: sum of all non-input-triggered shifts.
    pub fn cls(&self) -> f64 {
        self.layout_shifts.iter()
            .filter(|s| !s.had_recent_input)
            .map(|s| s.value)
            .sum()
    }

    pub fn record_interaction(&mut self, entry: InteractionEntry) {
        self.interactions.push(entry);
    }

    /// Real INP feed: zaznamenavat input event roundtrip cas. Caller predava
    /// `start_ms` (input timestamp) + `presentation_ms` (frame ktery zobrazil
    /// vysledek). INP = 75th percentile per spec.
    pub fn record_input_interaction(
        &mut self,
        interaction_type: &str,
        start_ms: f64,
        processing_start_ms: f64,
        processing_end_ms: f64,
        presentation_ms: f64,
    ) {
        self.interactions.push(InteractionEntry {
            interaction_type: interaction_type.into(),
            start_time_ms: start_ms,
            processing_start_ms,
            processing_end_ms,
            presentation_time_ms: presentation_ms,
        });
    }

    /// INP = 75th percentile interaction latency (presentation_time - start).
    pub fn inp_ms(&self) -> Option<f64> {
        let mut latencies: Vec<f64> = self.interactions.iter()
            .map(|i| i.presentation_time_ms - i.start_time_ms)
            .collect();
        if latencies.is_empty() { return None; }
        latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let idx = (latencies.len() * 75 / 100).min(latencies.len() - 1);
        Some(latencies[idx])
    }

    pub fn rating(&self, vital: WebVital) -> Option<VitalsRating> {
        let value = match vital {
            WebVital::LCP => self.lcp().map(|e| e.render_time_ms.unwrap_or(e.start_time_ms))?,
            WebVital::CLS => self.cls(),
            WebVital::INP => self.inp_ms()?,
            WebVital::TTFB => self.ttfb_ms?,
            WebVital::FCP => self.fcp_ms?,
            WebVital::FID => self.inp_ms()?,
        };
        Some(classify(vital, value))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VitalsRating {
    Good,
    NeedsImprovement,
    Poor,
}

/// Visual union area = area pokryta obema rect inside viewport. Pro CLS impact.
fn visual_union_area(
    prev: &(f32, f32, f32, f32),
    curr: &(f32, f32, f32, f32),
    vp_w: f32, vp_h: f32,
) -> f32 {
    let vp = (0.0, 0.0, vp_w, vp_h);
    let a = clip_to_viewport(prev, &vp);
    let b = clip_to_viewport(curr, &vp);
    let a_area = a.2 * a.3;
    let b_area = b.2 * b.3;
    let inter = intersect_area(&a, &b);
    a_area + b_area - inter
}

fn clip_to_viewport(r: &(f32, f32, f32, f32), vp: &(f32, f32, f32, f32)) -> (f32, f32, f32, f32) {
    let x1 = r.0.max(vp.0);
    let y1 = r.1.max(vp.1);
    let x2 = (r.0 + r.2).min(vp.0 + vp.2);
    let y2 = (r.1 + r.3).min(vp.1 + vp.3);
    if x2 <= x1 || y2 <= y1 { (0.0, 0.0, 0.0, 0.0) }
    else { (x1, y1, x2 - x1, y2 - y1) }
}

fn intersect_area(a: &(f32, f32, f32, f32), b: &(f32, f32, f32, f32)) -> f32 {
    let x1 = a.0.max(b.0);
    let y1 = a.1.max(b.1);
    let x2 = (a.0 + a.2).min(b.0 + b.2);
    let y2 = (a.1 + a.3).min(b.1 + b.3);
    if x2 <= x1 || y2 <= y1 { 0.0 } else { (x2 - x1) * (y2 - y1) }
}

pub fn classify(vital: WebVital, value: f64) -> VitalsRating {
    match vital {
        WebVital::LCP => {
            if value <= 2500.0 { VitalsRating::Good }
            else if value <= 4000.0 { VitalsRating::NeedsImprovement }
            else { VitalsRating::Poor }
        }
        WebVital::CLS => {
            if value <= 0.1 { VitalsRating::Good }
            else if value <= 0.25 { VitalsRating::NeedsImprovement }
            else { VitalsRating::Poor }
        }
        WebVital::INP | WebVital::FID => {
            if value <= 200.0 { VitalsRating::Good }
            else if value <= 500.0 { VitalsRating::NeedsImprovement }
            else { VitalsRating::Poor }
        }
        WebVital::TTFB => {
            if value <= 800.0 { VitalsRating::Good }
            else if value <= 1800.0 { VitalsRating::NeedsImprovement }
            else { VitalsRating::Poor }
        }
        WebVital::FCP => {
            if value <= 1800.0 { VitalsRating::Good }
            else if value <= 3000.0 { VitalsRating::NeedsImprovement }
            else { VitalsRating::Poor }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lcp_picks_largest() {
        let mut c = WebVitalsCollector::new();
        c.record_lcp(LcpEntry { element_id: None, url: None, start_time_ms: 0.0, render_time_ms: None, size_px: 100.0 });
        c.record_lcp(LcpEntry { element_id: None, url: None, start_time_ms: 0.0, render_time_ms: None, size_px: 200.0 });
        c.record_lcp(LcpEntry { element_id: None, url: None, start_time_ms: 0.0, render_time_ms: None, size_px: 50.0 });
        assert_eq!(c.lcp().unwrap().size_px, 200.0);
    }

    #[test]
    fn cls_excludes_input_triggered() {
        let mut c = WebVitalsCollector::new();
        c.record_layout_shift(LayoutShiftEntry { value: 0.1, had_recent_input: false, sources: vec![], start_time_ms: 0.0 });
        c.record_layout_shift(LayoutShiftEntry { value: 0.5, had_recent_input: true, sources: vec![], start_time_ms: 0.0 });
        assert!((c.cls() - 0.1).abs() < 0.0001);
    }

    #[test]
    fn inp_percentile() {
        let mut c = WebVitalsCollector::new();
        for ms in [50.0, 100.0, 200.0, 500.0, 1000.0] {
            c.record_interaction(InteractionEntry {
                interaction_type: "click".into(),
                start_time_ms: 0.0,
                processing_start_ms: 0.0,
                processing_end_ms: 0.0,
                presentation_time_ms: ms,
            });
        }
        // 75th of 5 = index 3 -> 500ms
        assert_eq!(c.inp_ms(), Some(500.0));
    }

    #[test]
    fn rating_lcp_good() {
        assert_eq!(classify(WebVital::LCP, 1500.0), VitalsRating::Good);
        assert_eq!(classify(WebVital::LCP, 3000.0), VitalsRating::NeedsImprovement);
        assert_eq!(classify(WebVital::LCP, 5000.0), VitalsRating::Poor);
    }

    #[test]
    fn rating_cls_thresholds() {
        assert_eq!(classify(WebVital::CLS, 0.05), VitalsRating::Good);
        assert_eq!(classify(WebVital::CLS, 0.2), VitalsRating::NeedsImprovement);
        assert_eq!(classify(WebVital::CLS, 0.5), VitalsRating::Poor);
    }

    #[test]
    fn collect_from_paint_picks_largest_image() {
        use crate::browser::paint::DisplayCommand;
        let cmds = vec![
            DisplayCommand::Image {
                x: 0.0, y: 0.0, w: 50.0, h: 50.0,
                src: "small.png".into(), radius: 0.0,
            },
            DisplayCommand::Image {
                x: 0.0, y: 0.0, w: 800.0, h: 600.0,
                src: "hero.jpg".into(), radius: 0.0,
            },
            DisplayCommand::Image {
                x: 0.0, y: 0.0, w: 100.0, h: 100.0,
                src: "thumb.png".into(), radius: 0.0,
            },
        ];
        let mut c = WebVitalsCollector::new();
        c.collect_from_paint(&cmds, 1000.0);
        assert_eq!(c.lcp_candidates.len(), 3);
        let lcp = c.lcp().unwrap();
        assert_eq!(lcp.url.as_deref(), Some("hero.jpg"));
        assert_eq!(lcp.size_px, 480000.0);
    }

    #[test]
    fn record_input_interaction_feeds_inp() {
        let mut c = WebVitalsCollector::new();
        c.record_input_interaction("click", 100.0, 105.0, 110.0, 200.0);
        c.record_input_interaction("click", 300.0, 305.0, 310.0, 450.0);
        c.record_input_interaction("keydown", 500.0, 502.0, 508.0, 580.0);
        assert!(c.inp_ms().is_some());
        // INP > 0 (75th of latencies 100/150/80).
        assert!(c.inp_ms().unwrap() > 0.0);
    }

    #[test]
    fn feed_layout_shift_detects_movement() {
        use std::collections::HashMap;
        let mut prev = HashMap::new();
        let mut curr = HashMap::new();
        // Element at y=100, w=400, h=100 moves to y=200 (shifted 100px down).
        prev.insert(1u64, (0.0, 100.0, 400.0, 100.0));
        curr.insert(1u64, (0.0, 200.0, 400.0, 100.0));
        let mut c = WebVitalsCollector::new();
        let count = c.feed_layout_shift(&prev, &curr, 1000.0, 800.0, false, 1500.0);
        assert!(count >= 1, "expected shift detected");
        assert!(c.cls() > 0.0, "CLS should be > 0 after movement");
    }

    #[test]
    fn feed_layout_shift_skips_small_shifts() {
        use std::collections::HashMap;
        let mut prev = HashMap::new();
        let mut curr = HashMap::new();
        // 2px shift = below 3px threshold = skip.
        prev.insert(1u64, (0.0, 100.0, 400.0, 100.0));
        curr.insert(1u64, (0.0, 102.0, 400.0, 100.0));
        let mut c = WebVitalsCollector::new();
        let count = c.feed_layout_shift(&prev, &curr, 1000.0, 800.0, false, 0.0);
        assert_eq!(count, 0);
    }

    #[test]
    fn feed_layout_shift_input_triggered_excluded() {
        use std::collections::HashMap;
        let mut prev = HashMap::new();
        let mut curr = HashMap::new();
        prev.insert(1u64, (0.0, 100.0, 400.0, 100.0));
        curr.insert(1u64, (0.0, 200.0, 400.0, 100.0));
        let mut c = WebVitalsCollector::new();
        c.feed_layout_shift(&prev, &curr, 1000.0, 800.0, true, 0.0);
        // User-triggered = excluded from CLS per spec.
        assert_eq!(c.cls(), 0.0);
    }

    #[test]
    fn collect_from_paint_skips_tracking_pixels() {
        use crate::browser::paint::DisplayCommand;
        let cmds = vec![
            // 1x1 tracking pixel = area 1 < 100 threshold = skip.
            DisplayCommand::Image {
                x: 0.0, y: 0.0, w: 1.0, h: 1.0,
                src: "tracker.gif".into(), radius: 0.0,
            },
        ];
        let mut c = WebVitalsCollector::new();
        c.collect_from_paint(&cmds, 0.0);
        assert!(c.lcp().is_none());
    }
}
