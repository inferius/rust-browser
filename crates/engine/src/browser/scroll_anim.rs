//! Smooth scroll animation s velocity-preserving retarget.
//!
//! Drive lerp 25%/frame (frame-rate dependent), pak cubic-bezier s fixed
//! duration (ale reset velocity pri kazdem wheel = rapid scroll pomalejsi
//! nez slow).
//!
//! Ted: cubic bezier s "initial slope" zachovava current velocity pri
//! retarget. Rapid wheel events accelerate scroll natural way.
//!
//! Inspired by:
//! - Chromium `cc/animation/scroll_offset_animation_curve.cc`
//!   `EaseInOutWithInitialSlope` - bezier P1 scaled by slope = velocity
//!   continuity pri retarget.
//! - WebRender `gfx/wr/webrender/src/animation.rs` momentum scroll.

use std::time::Instant;

/// Smooth scroll animation state - drzi start/target + casovy okno + initial
/// velocity pro continuity pres rapid wheel retargets.
#[derive(Debug, Clone, Copy)]
pub struct ScrollAnimState {
    pub start_value: f32,
    pub target_value: f32,
    pub start_time: Instant,
    pub duration_secs: f32,
    /// Initial velocity v px/sec at start_time. Nenulová pri retarget z
    /// active anim - bezier P1 scaled k zachovani spojitosti.
    pub start_velocity_per_sec: f32,
}

impl ScrollAnimState {
    /// Vypocti sample value v case `now`. Pri t >= 1.0 vraci target + done=true.
    pub fn sample(&self, now: Instant) -> (f32, bool) {
        let elapsed = now.duration_since(self.start_time).as_secs_f32();
        let t = (elapsed / self.duration_secs.max(0.001)).clamp(0.0, 1.0);
        let delta = self.target_value - self.start_value;
        // Bezier P1 = (x1, x1 * normalized_slope). Slope clamped na [0, 2.0]
        // aby curve NEPRESLA target mid-progress (= no overshoot/backtrack).
        // Pri high velocity slope > 2 = curve y > 1 mid-anim = scroll prepass
        // target value pak nazpet. User bug "vrati se zpatky".
        let normalized_slope = if delta.abs() > 0.01 {
            (self.start_velocity_per_sec * self.duration_secs / delta).clamp(0.0, 2.0)
        } else { 0.0 };
        let progress = cubic_bezier_y(t, 0.42, 0.42 * normalized_slope, 0.58, 1.0);
        let v = self.start_value + delta * progress;
        (v, t >= 1.0)
    }

    /// Vypocti aktualni velocity (px/sec) v case `now`. Pro retarget continuity.
    pub fn velocity_at(&self, now: Instant) -> f32 {
        let elapsed = now.duration_since(self.start_time).as_secs_f32();
        let t = (elapsed / self.duration_secs.max(0.001)).clamp(0.0, 1.0);
        let delta = self.target_value - self.start_value;
        if delta.abs() < 0.01 { return 0.0; }
        let normalized_slope = (self.start_velocity_per_sec * self.duration_secs / delta).clamp(0.0, 2.0);
        let eps = 0.001_f32;
        let p1 = cubic_bezier_y(t, 0.42, 0.42 * normalized_slope, 0.58, 1.0);
        let t2 = (t + eps / self.duration_secs).min(1.0);
        let p2 = cubic_bezier_y(t2, 0.42, 0.42 * normalized_slope, 0.58, 1.0);
        let derivative = (p2 - p1) / eps;
        delta * derivative
    }
}

/// Retarget scroll animation pri wheel/kbd.
///
/// `new_target` je ABSOLUTNI cilovy scroll offset. Caller jiz akumuloval
/// previous remainder pres `scroll_target_y + dy`, takze funkce NESMI znovu
/// pricitat prev anim's remainder = "double-counting bug": scroll overstreli
/// cil, anim skonci, frame tick `scroll_y = scroll_target_y` = skok zpatky.
/// To je user-reported regression "po chvili skoci ve skrollu kus zpet".
///
/// Curve VZDY resetuje od `current_value` s novym start_time. Bez resetu by
/// prevoz prev.start_value/start_time pri vetsim total delta = vetsi
/// duration = sample s puvodnim (kratsim) elapsed posouval zpatky v
/// t-progressu = "vrati se zpatky" jine forma backtracku.
///
/// Velocity continuity: pri same-direction retarget dedi `start_velocity`
/// z prev anim = no ease-in lag pri rapid wheel.
///
/// Inspired by Chromium `cc::ScrollOffsetAnimationCurve::UpdateTarget`.
/// Reference Chromium unit testy: cc/animation/scroll_offset_animation_curve_unittest.cc
pub fn retarget_scroll(
    current_value: f32,
    new_target: f32,
    now: Instant,
    prev: Option<&ScrollAnimState>,
) -> Option<ScrollAnimState> {
    let delta_from_current = new_target - current_value;
    if delta_from_current.abs() < 0.5 {
        return None;
    }
    let direction = delta_from_current.signum();
    // Velocity continuity z prev anim (jen same-direction).
    let prev_velocity = prev.map(|p| p.velocity_at(now)).unwrap_or(0.0);
    let same_dir = prev_velocity.signum() == direction || prev_velocity.abs() < 0.01;
    let initial_velocity = if same_dir { prev_velocity } else { 0.0 };
    // Duration scaled by velocity - vetsi velocity = kratsi duration.
    let abs_delta = delta_from_current.abs();
    let base_duration = duration_for_delta(abs_delta);
    let velocity_factor = (initial_velocity.abs() / 3000.0).min(1.0);
    let duration = (base_duration * (1.0 - 0.4 * velocity_factor)).max(0.05);
    Some(ScrollAnimState {
        start_value: current_value,
        target_value: new_target,
        start_time: now,
        duration_secs: duration,
        start_velocity_per_sec: initial_velocity,
    })
}

/// Duration scroll animation z delta. Inspired by Chromium kInverseDelta ramp:
/// short delta = longer per-unit time (smooth), big delta = shorter.
pub fn duration_for_delta(delta_px: f32) -> f32 {
    let d = delta_px.abs();
    if d <= 120.0 { 0.18 }
    else if d >= 480.0 { 0.35 }
    else { 0.18 + (d - 120.0) / (480.0 - 120.0) * 0.17 }
}

/// Cubic bezier curve - Newton solver pres control points (x1, y1, x2, y2).
/// Input x (= t v normalized progress), output y (eased value).
/// P0 = (0, 0), P3 = (1, 1) - fixed endpoints.
pub fn cubic_bezier_y(x: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    if x <= 0.0 { return 0.0; }
    if x >= 1.0 { return 1.0; }
    let cb_x = |t: f32| {
        let omt = 1.0 - t;
        3.0 * omt * omt * t * x1 + 3.0 * omt * t * t * x2 + t * t * t
    };
    let cb_x_prime = |t: f32| {
        let omt = 1.0 - t;
        3.0 * omt * omt * x1 + 6.0 * omt * t * (x2 - x1) + 3.0 * t * t * (1.0 - x2)
    };
    let mut t = x;
    for _ in 0..8 {
        let cur = cb_x(t);
        let err = cur - x;
        if err.abs() < 1e-5 { break; }
        let d = cb_x_prime(t);
        if d.abs() < 1e-6 { break; }
        t = (t - err / d).clamp(0.0, 1.0);
    }
    let omt = 1.0 - t;
    3.0 * omt * omt * t * y1 + 3.0 * omt * t * t * y2 + t * t * t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bezier_endpoints() {
        assert!((cubic_bezier_y(0.0, 0.42, 0.0, 0.58, 1.0) - 0.0).abs() < 1e-5);
        assert!((cubic_bezier_y(1.0, 0.42, 0.0, 0.58, 1.0) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn bezier_midpoint_ease_in_out() {
        // Standard ease-in-out at t=0.5 -> y=0.5.
        let y = cubic_bezier_y(0.5, 0.42, 0.0, 0.58, 1.0);
        assert!((y - 0.5).abs() < 0.05);
    }

    #[test]
    fn duration_short_delta_smaller_than_long() {
        let short = duration_for_delta(50.0);
        let long = duration_for_delta(800.0);
        assert!(short < long);
    }

    #[test]
    fn retarget_no_anim_for_small_delta() {
        let now = Instant::now();
        assert!(retarget_scroll(0.0, 0.2, now, None).is_none());
    }

    #[test]
    fn retarget_uses_absolute_target_no_double_count() {
        // Caller passes absolute new_target (already includes prev remainder).
        // Function MUST NOT add prev remainder again.
        // User-reported bug fix: scroll overshoots target then jumps back.
        let now = Instant::now();
        let prev = ScrollAnimState {
            start_value: 0.0, target_value: 100.0,
            start_time: now - std::time::Duration::from_millis(50),
            duration_secs: 0.2,
            start_velocity_per_sec: 0.0,
        };
        let new_anim = retarget_scroll(40.0, 200.0, now, Some(&prev)).unwrap();
        // Target = exactly new_target arg, no prev_remainder added.
        assert_eq!(new_anim.target_value, 200.0);
        assert_eq!(new_anim.start_value, 40.0);
        assert_eq!(new_anim.start_time, now);
    }

    #[test]
    fn retarget_zeros_velocity_opposite_direction() {
        let now = Instant::now();
        let prev = ScrollAnimState {
            start_value: 0.0, target_value: 100.0,
            start_time: now - std::time::Duration::from_millis(50),
            duration_secs: 0.2,
            start_velocity_per_sec: 0.0,
        };
        // Retarget opacnym smerem (scroll up after scroll down) - NOVY curve.
        let new_anim = retarget_scroll(40.0, -50.0, now, Some(&prev)).unwrap();
        // Start = current value (not prev.start_value).
        assert_eq!(new_anim.start_value, 40.0);
        // Velocity zero - smer changed.
        assert_eq!(new_anim.start_velocity_per_sec, 0.0);
    }

    #[test]
    fn rapid_retargeting_accelerates() {
        // Simulate rapid wheel sequence - 3 events s 30ms apart.
        // Caller arithmetic: scroll_target_y += dy per event.
        // Po 90ms s 3 wheel impulses cumulative target 180px.
        let t0 = Instant::now();
        let mut scroll_target = 0.0_f32;
        let mut cur = 0.0_f32;
        let mut state: Option<ScrollAnimState> = None;
        for i in 0..3 {
            let ti = t0 + std::time::Duration::from_millis(i * 30);
            if let Some(s) = state { cur = s.sample(ti).0; }
            scroll_target += 60.0;
            state = retarget_scroll(cur, scroll_target, ti, state.as_ref());
        }
        let t3 = t0 + std::time::Duration::from_millis(90);
        let cur3 = state.unwrap().sample(t3).0;
        // Klicove: scroll progresses, ne stuck at 5-10 (= pred fix bug).
        assert!(cur3 > 30.0, "rapid scroll should accumulate, got {}", cur3);
        // Klicove: scroll NEPREJDE total target (= no overshoot).
        assert!(cur3 <= scroll_target + 1.0,
            "scroll {} overshot target {} (double-count bug)", cur3, scroll_target);
    }

    #[test]
    fn rapid_scroll_finishes_at_total_distance_no_overshoot() {
        // 5 rapid wheels po 60 = absolute target 300px.
        // Po anim done cur = 300 EXACTLY (no overshoot, no backtrack).
        let t0 = Instant::now();
        let mut state: Option<ScrollAnimState> = None;
        let mut cur = 0.0_f32;
        let mut scroll_target = 0.0_f32;
        for i in 0..5 {
            let ti = t0 + std::time::Duration::from_millis(i * 20);
            if let Some(s) = state { cur = s.sample(ti).0; }
            scroll_target += 60.0;
            state = retarget_scroll(cur, scroll_target, ti, state.as_ref());
        }
        // Sample po 1.5s - anim should be done at exactly 300.
        let t_end = t0 + std::time::Duration::from_millis(1500);
        let final_v = state.unwrap().sample(t_end).0;
        assert!((final_v - 300.0).abs() < 0.5,
            "after rapid scroll burst expected 300, got {}", final_v);
    }

    // Chromium-inspired tests per cc/animation/scroll_offset_animation_curve_unittest.cc.

    #[test]
    fn chromium_update_target_keeps_progress_forward() {
        // ScrollOffsetAnimationCurveTest.UpdateTarget_PointAtCurrentTime:
        // Po retarget v progress momentu se scroll nezasekne (cur >= prev sample).
        let t0 = Instant::now();
        let state1 = retarget_scroll(0.0, 100.0, t0, None).unwrap();
        let t_mid = t0 + std::time::Duration::from_millis(60);
        let cur_before = state1.sample(t_mid).0;
        let state2 = retarget_scroll(cur_before, 300.0, t_mid, Some(&state1)).unwrap();
        // Hned po retarget sample na stejny cas vraci aspon cur_before (no jump back).
        let cur_after = state2.sample(t_mid).0;
        assert!(cur_after >= cur_before - 0.01,
            "after retarget sample dropped from {} to {}", cur_before, cur_after);
    }

    #[test]
    fn chromium_reverse_does_not_change_current_value() {
        // Po reverse retarget sample na stejny cas == cur (no discontinuity).
        let t0 = Instant::now();
        let state1 = retarget_scroll(0.0, 100.0, t0, None).unwrap();
        let t_mid = t0 + std::time::Duration::from_millis(50);
        let cur_before = state1.sample(t_mid).0;
        let state2 = retarget_scroll(cur_before, -100.0, t_mid, Some(&state1)).unwrap();
        let cur_after = state2.sample(t_mid).0;
        assert!((cur_after - cur_before).abs() < 0.5,
            "reverse retarget jumped from {} to {}", cur_before, cur_after);
        // Velocity sign flipped (0 - protoze opacny smer = no inheritance).
        assert_eq!(state2.start_velocity_per_sec, 0.0);
    }

    #[test]
    fn chromium_duration_progress_positive() {
        // Po cele duration sample == target.
        let t0 = Instant::now();
        let state = retarget_scroll(0.0, 200.0, t0, None).unwrap();
        let t_end = t0 + std::time::Duration::from_secs_f32(state.duration_secs + 0.01);
        let (v, done) = state.sample(t_end);
        assert!(done);
        assert!((v - 200.0).abs() < 0.5);
    }

    // Firefox-inspired (gfx/layers/apz/AsyncPanZoomController.cpp DragBlock):
    // velocity should carry over across same-direction retargets.
    #[test]
    fn firefox_velocity_carries_same_direction() {
        let t0 = Instant::now();
        let state1 = retarget_scroll(0.0, 100.0, t0, None).unwrap();
        let t_mid = t0 + std::time::Duration::from_millis(30);
        let v_before = state1.velocity_at(t_mid);
        let cur = state1.sample(t_mid).0;
        let state2 = retarget_scroll(cur, 300.0, t_mid, Some(&state1)).unwrap();
        // initial_velocity matches prev (within sign + reasonable magnitude).
        assert_eq!(state2.start_velocity_per_sec.signum(), v_before.signum());
    }

    #[test]
    fn no_backtrack_after_anim_finish() {
        // User regression: scroll happens, then jumps back after a moment.
        // Cause: anim.target > scroll_target_y -> when anim done, frame tick
        // assigns scroll_y = scroll_target_y = JUMP BACK.
        // Fix: anim.target_value == scroll_target_y exactly.
        let t0 = Instant::now();
        let scroll_target = 120.0_f32;
        // First wheel: 60.
        let state1 = retarget_scroll(0.0, 60.0, t0, None).unwrap();
        let t1 = t0 + std::time::Duration::from_millis(30);
        let cur1 = state1.sample(t1).0;
        // Second wheel: scroll_target = 60 + 60 = 120.
        let state2 = retarget_scroll(cur1, scroll_target, t1, Some(&state1)).unwrap();
        // Anim target MUST equal scroll_target (== 120, not 120 + remainder).
        assert!((state2.target_value - scroll_target).abs() < 0.5,
            "anim.target {} != scroll_target {}", state2.target_value, scroll_target);
    }
}
