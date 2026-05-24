//! Frame pacing - VSync prediction + scheduling.
//!
//! Goal: render before the next display vblank; otherwise frame is dropped.
//! Chromium uses BeginFrameSource. Foundation here covers presentation deadline
//! tracking + per-stage timing budgets.

use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub struct FrameBudget {
    pub frame_period: Duration,   // 1/refresh-rate
    pub script_ms: f32,           // JS execution budget
    pub style_ms: f32,            // style recalc budget
    pub layout_ms: f32,
    pub paint_ms: f32,
    pub composite_ms: f32,
}

impl FrameBudget {
    pub fn for_refresh_hz(hz: u32) -> Self {
        let period = Duration::from_secs_f32(1.0 / hz as f32);
        // Rough fractions per pipeline stage.
        let ms = period.as_secs_f32() * 1000.0;
        Self {
            frame_period: period,
            script_ms: ms * 0.4,
            style_ms: ms * 0.1,
            layout_ms: ms * 0.2,
            paint_ms: ms * 0.15,
            composite_ms: ms * 0.15,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FrameMetric {
    pub frame_index: u64,
    pub stage_durations: Vec<(FrameStage, Duration)>,
    pub presented: bool,
    pub dropped_reason: Option<String>,
    pub start: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FrameStage {
    Input,
    RequestAnimationFrame,
    Style,
    Layout,
    Paint,
    Composite,
    Present,
}

#[derive(Default)]
pub struct FramePacer {
    pub vsync_period: Duration,
    pub last_vsync: Option<Instant>,
    pub history: Vec<FrameMetric>,
    pub max_history: usize,
    pub frame_counter: u64,
}

impl FramePacer {
    pub fn new(refresh_hz: u32) -> Self {
        Self {
            vsync_period: Duration::from_secs_f32(1.0 / refresh_hz as f32),
            last_vsync: None,
            history: Vec::new(),
            max_history: 240,           // ~ 4s at 60fps
            frame_counter: 0,
        }
    }

    pub fn record_vsync(&mut self, t: Instant) {
        self.last_vsync = Some(t);
    }

    /// Estimated next presentation deadline.
    pub fn next_deadline(&self, now: Instant) -> Instant {
        match self.last_vsync {
            Some(t) => {
                let mut next = t + self.vsync_period;
                while next < now { next += self.vsync_period; }
                next
            }
            None => now + self.vsync_period,
        }
    }

    pub fn begin_frame(&mut self) -> u64 {
        self.frame_counter += 1;
        let m = FrameMetric {
            frame_index: self.frame_counter,
            stage_durations: Vec::new(),
            presented: false,
            dropped_reason: None,
            start: Instant::now(),
        };
        self.history.push(m);
        if self.history.len() > self.max_history { self.history.remove(0); }
        self.frame_counter
    }

    pub fn record_stage(&mut self, frame_index: u64, stage: FrameStage, dur: Duration) {
        if let Some(m) = self.history.iter_mut().find(|m| m.frame_index == frame_index) {
            m.stage_durations.push((stage, dur));
        }
    }

    pub fn mark_presented(&mut self, frame_index: u64) {
        if let Some(m) = self.history.iter_mut().find(|m| m.frame_index == frame_index) {
            m.presented = true;
        }
    }

    pub fn mark_dropped(&mut self, frame_index: u64, reason: &str) {
        if let Some(m) = self.history.iter_mut().find(|m| m.frame_index == frame_index) {
            m.dropped_reason = Some(reason.into());
        }
    }

    pub fn dropped_count(&self) -> usize {
        self.history.iter().filter(|m| m.dropped_reason.is_some()).count()
    }

    pub fn presented_count(&self) -> usize {
        self.history.iter().filter(|m| m.presented).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_budget_60hz() {
        let b = FrameBudget::for_refresh_hz(60);
        let total = b.script_ms + b.style_ms + b.layout_ms + b.paint_ms + b.composite_ms;
        // Should be close to 16.66ms total budget
        assert!(total > 16.0 && total < 17.0);
    }

    #[test]
    fn pacer_records_metrics() {
        let mut p = FramePacer::new(60);
        let idx = p.begin_frame();
        p.record_stage(idx, FrameStage::Layout, Duration::from_micros(500));
        p.mark_presented(idx);
        assert_eq!(p.presented_count(), 1);
    }

    #[test]
    fn pacer_drops_recorded() {
        let mut p = FramePacer::new(60);
        let idx = p.begin_frame();
        p.mark_dropped(idx, "overbudget");
        assert_eq!(p.dropped_count(), 1);
    }

    #[test]
    fn pacer_history_capped() {
        let mut p = FramePacer::new(60);
        p.max_history = 10;
        for _ in 0..20 { p.begin_frame(); }
        assert_eq!(p.history.len(), 10);
    }

    #[test]
    fn next_deadline_from_vsync() {
        let mut p = FramePacer::new(60);
        let t0 = Instant::now();
        p.record_vsync(t0);
        let d = p.next_deadline(t0 + Duration::from_millis(8));
        assert!(d > t0);
    }
}
