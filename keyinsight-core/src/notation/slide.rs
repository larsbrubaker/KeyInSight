//! Follow-top (survival) slide lane: whenever the cursor enters a new
//! system, the whole score slides up so that system lands where the first
//! line lived — a feed, not a page.
//!
//! Ports the `followTop` / `slideOffset` half of the Swift page script in
//! `Notation/NotationController.swift`. The page slid by CSS transform
//! (`transition: transform 0.4s ease-out; translateY(slideOffset px)`);
//! here the same numbers drive the widget's paint origin, interpolated by
//! the host clock. Offsets keep the CSS sign: negative moves the score up.

use super::widget::whole_px;

/// `transition: transform 0.4s ease-out`.
pub const SLIDE_DURATION: f64 = 0.4;

/// CSS `ease-out`, close enough for a 0.4 s slide: `1 - (1 - t)^2`.
pub fn ease_out(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t) * (1.0 - t)
}

#[derive(Debug, Default)]
pub struct SlideLane {
    follow_top: bool,
    last_top_system: Option<usize>,
    /// Where the top line lives on screen (content px, pre-transform):
    /// the first system to go current anchors the lane.
    slide_anchor_top: Option<f64>,
    /// The settled transform (whole px; `translateY(slideOffset px)`).
    slide_offset: f64,
    /// Animation: from the offset on screen when the slide began.
    slide_from: f64,
    slide_started_at: Option<f64>,
}

impl SlideLane {
    /// `setFollowTop(on)`: `followTop = on; lastTopSystem = null;`.
    pub fn set_follow_top(&mut self, on: bool) {
        self.follow_top = on;
        self.last_top_system = None;
    }

    pub fn follow_top(&self) -> bool {
        self.follow_top
    }

    /// `resetSlide()` + the `loadScore` clears: offset 0, anchor gone,
    /// no remembered top system, no motion.
    pub fn reset(&mut self) {
        self.last_top_system = None;
        self.slide_anchor_top = None;
        self.slide_offset = 0.0;
        self.slide_from = 0.0;
        self.slide_started_at = None;
    }

    /// The cursor's system just went current at content-space `top` px.
    /// Returns true when a slide started (the widget keeps frames coming).
    pub fn enter_system(&mut self, system: usize, top: f64, now: f64) -> bool {
        if !self.follow_top || self.last_top_system == Some(system) {
            return false;
        }
        self.last_top_system = Some(system);
        let Some(anchor) = self.slide_anchor_top else {
            self.slide_anchor_top = Some(top); // first line anchors the lane
            return false;
        };
        // Screen position = content position + the (settled) transform.
        // Whole pixels — fractional offsets knock staff lines off the
        // pixel grid.
        let delta = whole_px(top + self.slide_offset - anchor);
        if delta == 0.0 {
            return false;
        }
        // A transition restarting mid-flight continues from where the
        // score is on screen right now.
        self.slide_from = self.offset_at(now);
        self.slide_offset -= delta;
        self.slide_started_at = Some(now);
        true
    }

    /// The settled target offset (`slideOffset`).
    pub fn slide_offset(&self) -> f64 {
        self.slide_offset
    }

    /// The transform on screen at `now` (whole px).
    pub fn offset_at(&self, now: f64) -> f64 {
        let Some(started) = self.slide_started_at else {
            return self.slide_offset;
        };
        let t = ease_out((now - started) / SLIDE_DURATION);
        whole_px(self.slide_from + (self.slide_offset - self.slide_from) * t)
    }

    pub fn is_sliding(&self) -> bool {
        self.slide_started_at.is_some()
    }

    /// Retire a finished transition (`transitionend`); true exactly once
    /// per slide, when it completes.
    pub fn finish_if_done(&mut self, now: f64) -> bool {
        match self.slide_started_at {
            Some(started) if now - started >= SLIDE_DURATION => {
                self.slide_started_at = None;
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_system_anchors_without_motion() {
        let mut lane = SlideLane::default();
        lane.set_follow_top(true);
        assert!(!lane.enter_system(0, 30.0, 1.0));
        assert_eq!(lane.slide_offset(), 0.0);
        assert!(!lane.is_sliding());
        // Staying on the same system never moves.
        assert!(!lane.enter_system(0, 30.0, 2.0));
        assert_eq!(lane.slide_offset(), 0.0);
    }

    #[test]
    fn later_systems_slide_to_the_anchor_in_whole_pixels() {
        let mut lane = SlideLane::default();
        lane.set_follow_top(true);
        lane.enter_system(0, 30.0, 0.0);
        assert!(lane.enter_system(1, 150.4, 1.0));
        assert_eq!(lane.slide_offset(), -(150.4f64 - 30.0).round());
        // Third system: measured on screen with the settled transform
        // applied (the page read `getBoundingClientRect` of the slid score).
        lane.finish_if_done(2.0);
        assert!(lane.enter_system(2, 270.6, 2.0));
        let settled = -(150.4f64 - 30.0).round();
        assert_eq!(
            lane.slide_offset(),
            settled - (270.6 + settled - 30.0).round()
        );
    }

    #[test]
    fn slide_eases_out_over_point_four_seconds() {
        let mut lane = SlideLane::default();
        lane.set_follow_top(true);
        lane.enter_system(0, 0.0, 10.0);
        lane.enter_system(1, 100.0, 10.0);
        assert_eq!(lane.offset_at(10.0), 0.0);
        let mid = lane.offset_at(10.2);
        // ease-out: past halfway at half time (1 - 0.25 = 0.75).
        assert_eq!(mid, -75.0);
        assert_eq!(lane.offset_at(10.4), -100.0);
        assert_eq!(lane.offset_at(11.0), -100.0);
        assert!(!lane.finish_if_done(10.3));
        assert!(lane.finish_if_done(10.4));
        assert!(!lane.finish_if_done(10.5), "completion fires once");
        assert!(!lane.is_sliding());
    }

    #[test]
    fn follow_top_off_never_slides_and_reset_clears() {
        let mut lane = SlideLane::default();
        assert!(!lane.enter_system(0, 0.0, 0.0));
        assert!(!lane.enter_system(1, 100.0, 0.0));
        assert_eq!(lane.slide_offset(), 0.0);
        lane.set_follow_top(true);
        lane.enter_system(0, 0.0, 0.0);
        lane.enter_system(1, 100.0, 0.0);
        lane.reset();
        assert_eq!(lane.slide_offset(), 0.0);
        assert_eq!(lane.offset_at(5.0), 0.0);
        // After a reset the next system anchors afresh.
        assert!(!lane.enter_system(1, 100.0, 6.0));
    }
}
