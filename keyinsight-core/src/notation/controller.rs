//! The notation feedback controller: per-note states, the wrong-answer
//! ghost, timing ticks, and the playback-follow schedule. The engine talks
//! to this; [`super::NotationWidget`] paints it.
//!
//! Ports `Notation/NotationController.swift`. The WKWebView command
//! surface (`setState`, `showGhost`, `addTick`, `followSchedule`) maps to
//! plain state the widget reads each paint; the JS rAF follow loop maps to
//! frame-time evaluation in [`NotationController::follow_ids_at`]; the
//! survival follow-top slide lives in [`super::slide::SlideLane`].

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use verovio_rust::{ElementKind, Primitive};

use crate::notation::slide::SlideLane;
use crate::notation::{NotationRenderer, Rendered};

/// Hover callback: (kind, element id); empty strings end the hover.
pub type InspectCallback = Box<dyn Fn(&str, &str)>;
/// Note-click callback: the clicked note's element id.
pub type NoteClickCallback = Box<dyn Fn(&str)>;

/// Feedback / heat-map state of one note element. Colors mirror the Swift
/// page CSS exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteState {
    // Training feedback states (missed = tempo-mode window closed).
    Current,
    Correct,
    Wrong,
    Missed,
    // Progress heat-map states.
    Mastered,
    Learning,
    Weak,
    Locked,
}

impl NoteState {
    /// The engraving override color (r, g, b, a) for this state.
    pub fn color(self) -> agg_gui::color::Color {
        use agg_gui::color::Color;
        match self {
            NoteState::Current => Color::from_rgb8(0x1D, 0x6F, 0xD6),
            NoteState::Correct | NoteState::Mastered => Color::from_rgb8(0x1A, 0x98, 0x50),
            NoteState::Wrong | NoteState::Weak => Color::from_rgb8(0xD7, 0x30, 0x27),
            NoteState::Missed => Color::from_rgba8(0xE6, 0xA2, 0x3C, 191), // 0.75 opacity
            NoteState::Learning => Color::from_rgb8(0xE6, 0xA2, 0x3C),
            NoteState::Locked => Color::from_rgb8(0xC4, 0xC4, 0xC4),
        }
    }
}

/// The wrong-answer ghost: a gray notehead at the played note's staff
/// position, horizontally aligned with the expected note.
#[derive(Debug, Clone, PartialEq)]
pub struct Ghost {
    pub expected_id: String,
    /// Diatonic steps from the expected note to the played note
    /// (positive = played higher).
    pub offset_steps: i32,
}

/// Timing tick above a note (tempo mode): ◂ early, ▸ late.
#[derive(Debug, Clone, PartialEq)]
pub struct Tick {
    pub id: String,
    pub early: bool,
}

/// Playback-follow schedule: one entry per event — all its ids (a chord,
/// or both hands at a shared onset) highlight together.
pub struct FollowSchedule {
    pub id_groups: Vec<Vec<String>>,
    pub times: Vec<f64>,
    started_at: f64,
}

pub struct NotationController {
    pub renderer: Rc<RefCell<NotationRenderer>>,
    states: HashMap<String, NoteState>,
    ghost: Option<Ghost>,
    ticks: Vec<Tick>,
    follow: Option<FollowSchedule>,
    /// Painted follow indices (the Swift page's `followLog` demo audit).
    follow_log: Vec<usize>,
    /// The follow group the cursor is on right now (`None` between
    /// schedules): the page treats it as the current system while a
    /// playback runs, since the engine paints the cursor by override
    /// instead of `Current` states.
    follow_group: Option<usize>,
    /// Called when the pointer hovers a notation element: (kind, id).
    /// Fires with empty strings when the hover ends.
    pub on_inspect: Option<InspectCallback>,
    /// Called when a note is clicked (same padded hit boxes as hover).
    pub on_note_click: Option<NoteClickCallback>,
    last_hover_key: RefCell<String>,
    /// Follow-top (survival) slide state; see [`Self::set_follow_top`].
    lane: SlideLane,
    /// A note just went current: its system is checked for visibility on
    /// the next paint (the page deferred `ensureVisible` to the next rAF).
    pending_visible: Option<String>,
    /// Page scrolling: the system the user grabbed scroll control on
    /// (`userScrollSystem`); auto-follow yields until the cursor moves on
    /// to a different system.
    user_scroll_system: Option<usize>,
    /// The widget-owned page scroll must return to 0 on the next paint
    /// (`loadScore`'s `window.scrollTo(0, 0)`).
    scroll_reset: bool,
}

/// One system's vertical span in layout px (y-down): the top staff's top
/// line to the lowest staff line (a grand staff reaches into the bass).
#[derive(Debug, Clone, Copy, PartialEq)]
struct SystemSpan {
    index: usize,
    top: f64,
    bottom: f64,
}

impl NotationController {
    pub fn new(renderer: Rc<RefCell<NotationRenderer>>) -> Self {
        Self {
            renderer,
            states: HashMap::new(),
            ghost: None,
            ticks: Vec::new(),
            follow: None,
            follow_log: Vec::new(),
            follow_group: None,
            on_inspect: None,
            on_note_click: None,
            last_hover_key: RefCell::new(String::new()),
            lane: SlideLane::default(),
            pending_visible: None,
            user_scroll_system: None,
            scroll_reset: false,
        }
    }

    /// A fresh score is on the toolkit: reset all per-exercise feedback
    /// (the Swift `loadScore` cleared ghost + ticks and re-set the SVG),
    /// scroll back to the top, and hand scroll control back to the follow.
    pub fn load_score(&mut self) {
        self.states.clear();
        self.ghost = None;
        self.ticks.clear();
        self.follow = None;
        self.follow_group = None;
        self.lane.reset();
        self.pending_visible = None;
        self.clear_user_scroll();
        self.scroll_reset = true;
        agg_gui::animation::request_draw();
    }

    pub fn set_state(&mut self, id: &str, state: Option<NoteState>) {
        match state {
            Some(state) => {
                if state == NoteState::Current {
                    self.pending_visible = Some(id.to_string());
                }
                self.states.insert(id.to_string(), state);
            }
            None => {
                self.states.remove(id);
            }
        }
        agg_gui::animation::request_draw();
    }

    pub fn state_of(&self, id: &str) -> Option<NoteState> {
        self.states.get(id).copied()
    }

    pub fn show_ghost(&mut self, expected_id: &str, offset_steps: i32) {
        self.ghost = Some(Ghost {
            expected_id: expected_id.to_string(),
            offset_steps,
        });
        agg_gui::animation::request_draw();
    }

    pub fn clear_ghost(&mut self) {
        self.ghost = None;
        agg_gui::animation::request_draw();
    }

    pub fn ghost(&self) -> Option<&Ghost> {
        self.ghost.as_ref()
    }

    pub fn add_tick(&mut self, id: &str, early: bool) {
        self.ticks.push(Tick {
            id: id.to_string(),
            early,
        });
        agg_gui::animation::request_draw();
    }

    pub fn ticks(&self) -> &[Tick] {
        &self.ticks
    }

    /// Drive the playback-follow cursor: the widget advances it every
    /// painted frame so every note gets a painted frame.
    pub fn follow_schedule(&mut self, id_groups: Vec<Vec<String>>, times: Vec<f64>, now: f64) {
        self.follow = Some(FollowSchedule {
            id_groups,
            times,
            started_at: now,
        });
        self.follow_log.clear();
        self.follow_group = None;
        agg_gui::animation::request_draw();
    }

    pub fn cancel_follow(&mut self) {
        self.follow = None;
        self.follow_group = None;
        agg_gui::animation::request_draw();
    }

    pub fn is_following(&self) -> bool {
        self.follow.is_some()
    }

    /// The follow group active at `now`, if any; logs newly reached
    /// indices (the demo audit). Returns the ids to paint as current.
    /// Reaching a new group also queues its first id for `ensureVisible`
    /// — the Swift follow loop `setState(id, 'current')`s each group, so
    /// a paged piece glides along under the playback cursor.
    pub fn follow_ids_at(&mut self, now: f64) -> Option<Vec<String>> {
        let follow = self.follow.as_ref()?;
        let t = now - follow.started_at;
        let mut index: Option<usize> = None;
        for (i, &time) in follow.times.iter().enumerate() {
            if time <= t {
                index = Some(i);
            } else {
                break;
            }
        }
        let index = index?;
        let group = follow.id_groups[index].clone();
        if self.follow_log.last() != Some(&index) {
            self.follow_log.push(index);
        }
        if self.follow_group != Some(index) {
            self.follow_group = Some(index);
            if let Some(first) = group.first() {
                self.pending_visible = Some(first.clone());
            }
        }
        Some(group)
    }

    /// The first id of the follow group the cursor is on, if a playback
    /// is running.
    fn follow_cursor_id(&self) -> Option<String> {
        let follow = self.follow.as_ref()?;
        follow
            .id_groups
            .get(self.follow_group?)?
            .first()
            .cloned()
    }

    /// The note indices the follow cursor actually painted (demo audit).
    pub fn follow_log(&self) -> &[usize] {
        &self.follow_log
    }

    /// Hover routing from the widget; deduplicates like the Swift page's
    /// `sendHover`.
    pub fn send_hover(&self, kind: &str, id: &str) {
        let key = format!("{kind}:{id}");
        if *self.last_hover_key.borrow() == key {
            return;
        }
        *self.last_hover_key.borrow_mut() = key;
        if let Some(on_inspect) = &self.on_inspect {
            on_inspect(kind, id);
        }
    }

    /// Convenience used by both engine and widget.
    pub fn render(&self, music_xml: &str) -> Option<Rendered> {
        self.renderer.borrow_mut().render(music_xml)
    }

    // --- Follow-top (survival) ---

    /// Follow-top scrolling (survival): whenever the cursor enters a new
    /// system, that system slides to the top of the view — a feed, not a
    /// page. Off = the default keep-in-upper-third behavior (which the
    /// fit-to-view widget satisfies without scrolling; see `widget.rs`).
    pub fn set_follow_top(&mut self, on: bool) {
        self.lane.set_follow_top(on);
        if on {
            // The feed slides by transform, never by page scroll: the
            // widget drops back to the fitted view with the scroll at 0
            // and no user override left behind.
            self.clear_user_scroll();
            self.scroll_reset = true;
        }
        agg_gui::animation::request_draw();
    }

    pub fn follow_top(&self) -> bool {
        self.lane.follow_top()
    }

    /// The note whose system must be brought into view on this paint.
    pub fn take_pending_visible(&mut self) -> Option<String> {
        self.pending_visible.take()
    }

    /// `ensureVisible(el)` in follow-top mode: entering a new system slides
    /// the score so that system lands where the top line lives. `scale` is
    /// the display scale (layout → screen px); `now` the host clock.
    pub fn ensure_visible(&mut self, id: &str, scale: f64, now: f64) {
        let Some(system) = self.system_of(id) else {
            return;
        };
        if self
            .lane
            .enter_system(system.index, system.top * scale, now)
        {
            agg_gui::animation::request_draw();
        }
    }

    // --- Page scrolling (keep the current system in the upper third) ---

    /// `ensureVisible(el)` on a scrolling page: the scroll offset (content
    /// px, 0 = top, whole px) the page should glide to so the current
    /// note's system sits in the upper third, or `None` to leave the
    /// scroll alone. Verbatim rule: when the system's top is above 5% or
    /// its bottom below 70% of the viewport, put its top at 18% —
    /// `round(scrollY + r.top - vh*0.18)`, never below 0, never past the
    /// end of the page. Yields to the user: after a manual scroll the
    /// follow stays off while the cursor remains on that system and
    /// re-engages when it enters a different one. Follow-top (survival)
    /// slides by transform instead, so this is always `None` there.
    ///
    /// The system rect is the [`SystemSpan`] — the top staff line to the
    /// lowest staff line — not the Swift page's full `g.system` ink box
    /// (which also spans ledger lines, stems, and text above/below the
    /// staves), so the band test runs a few px tighter than the page's.
    pub fn follow_scroll_target(
        &mut self,
        id: &str,
        scale: f64,
        viewport_h: f64,
        scroll: f64,
    ) -> Option<f64> {
        if self.lane.follow_top() {
            return None;
        }
        let system = self.system_of(id)?;
        if let Some(held) = self.user_scroll_system {
            if held == system.index {
                return None; // the user is in control here
            }
            self.user_scroll_system = None; // new system: re-engage
        }
        let top = system.top * scale - scroll;
        let bottom = system.bottom * scale - scroll;
        if top >= viewport_h * 0.05 && bottom <= viewport_h * 0.7 {
            return None;
        }
        let content_h = {
            let renderer = self.renderer.borrow();
            renderer.toolkit().current_layout()?.height * scale
        };
        let max_scroll = (content_h - viewport_h).max(0.0);
        let y = (scroll + top - viewport_h * 0.18).round();
        Some(y.max(0.0).min(max_scroll.round()))
    }

    /// Manual wheel/trackbar input hands scroll control to the user for
    /// the remainder of the current system (`userScrollSystem`). During a
    /// playback the follow cursor's group is the current note; otherwise
    /// the `Current`-state note. With no current note there is nothing to
    /// hold, so the next cursor move re-engages at once (the page's
    /// `document.body` placeholder).
    pub fn note_user_scroll(&mut self) {
        let current = self.follow_cursor_id().or_else(|| {
            self.states
                .iter()
                .filter(|(_, &state)| state == NoteState::Current)
                .map(|(id, _)| id.clone())
                .min()
        });
        self.user_scroll_system = current.and_then(|id| self.system_of(&id)).map(|s| s.index);
    }

    /// Drop the manual-scroll override (`userScrollSystem = null`).
    pub fn clear_user_scroll(&mut self) {
        self.user_scroll_system = None;
    }

    /// The system the user holds scroll control on, if any.
    pub fn user_scroll_system(&self) -> Option<usize> {
        self.user_scroll_system
    }

    /// True once after each `load_score` / follow-top switch: the widget
    /// puts its page scroll back to 0.
    pub fn take_scroll_reset(&mut self) -> bool {
        std::mem::take(&mut self.scroll_reset)
    }

    /// The system a note sits on and its top staff line's layout y (the
    /// page walked up to the `g.system` ancestor): the system whose staff
    /// lines are nearest the note's ink — notehead plus stem, so a high
    /// ledger note still belongs to the staff its stem reaches for — which
    /// puts the boundary between two systems at the midpoint of the gap
    /// between them.
    fn system_of(&self, id: &str) -> Option<SystemSpan> {
        let renderer = self.renderer.borrow();
        let layout = renderer.toolkit().current_layout()?;
        let &(_, y_top, _, h) = layout.bounds_by_id.get(id)?;
        let systems = &layout.systems;
        // The note's ink: the notehead's centre (its glyph box is an em
        // square, too loose to measure with) stretched along its stem.
        let cy = y_top + h / 2.0;
        let (ink_top, ink_bottom) = layout
            .elements
            .iter()
            .filter(|element| {
                element.kind == ElementKind::Stem && element.id.as_deref() == Some(id)
            })
            .fold((cy, cy), |(top, bottom), element| {
                let (_, y, _, h) = element.bounds;
                (top.min(y), bottom.max(y + h))
            });
        // Each system's lowest staff line (a grand staff reaches down to the
        // bass staff), attributed by the system the line falls under.
        let mut bottoms: Vec<f64> = systems.iter().map(|system| system.staff_top).collect();
        for element in layout.elements.iter().filter(|e| e.kind == ElementKind::StaffLine) {
            let Primitive::Line { y1: y, .. } = element.primitive else {
                continue;
            };
            if let Some(system) = systems.iter().rev().find(|system| system.staff_top <= y) {
                bottoms[system.index] = bottoms[system.index].max(y);
            }
        }
        systems
            .iter()
            .zip(&bottoms)
            .map(|(system, &bottom)| {
                let gap = (system.staff_top - ink_bottom).max(ink_top - bottom).max(0.0);
                (system, gap)
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(system, _)| SystemSpan {
                index: system.index,
                top: system.staff_top,
                bottom: bottoms[system.index],
            })
    }

    /// The settled slide target (`slideOffset`, whole px; negative = up).
    pub fn slide_offset(&self) -> f64 {
        self.lane.slide_offset()
    }

    /// The slide transform on screen at `now`; keeps frames coming while
    /// a slide is in flight and, like the page's `transitionend`, requests
    /// one more paint when it completes so hit geometry settles.
    pub fn slide_offset_at(&mut self, now: f64) -> f64 {
        let offset = self.lane.offset_at(now);
        if self.lane.finish_if_done(now) || self.lane.is_sliding() {
            agg_gui::animation::request_draw();
        }
        offset
    }

    /// The slide transform on screen at `now`, read-only (hit testing).
    pub fn slide_offset_on_screen(&self, now: f64) -> f64 {
        self.lane.offset_at(now)
    }

    /// Click routing from the widget (repertoire: practice-from-here).
    pub fn send_click(&self, id: &str) {
        if let Some(on_note_click) = &self.on_note_click {
            on_note_click(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn controller() -> NotationController {
        NotationController::new(Rc::new(RefCell::new(NotationRenderer::new())))
    }

    #[test]
    fn state_flips_and_clears() {
        let mut c = controller();
        c.set_state("note-0", Some(NoteState::Current));
        assert_eq!(c.state_of("note-0"), Some(NoteState::Current));
        c.set_state("note-0", Some(NoteState::Correct));
        assert_eq!(c.state_of("note-0"), Some(NoteState::Correct));
        c.set_state("note-0", None);
        assert_eq!(c.state_of("note-0"), None);
    }

    #[test]
    fn load_score_resets_feedback() {
        let mut c = controller();
        c.set_state("note-0", Some(NoteState::Wrong));
        c.show_ghost("note-0", 2);
        c.add_tick("note-0", true);
        c.load_score();
        assert_eq!(c.state_of("note-0"), None);
        assert!(c.ghost().is_none());
        assert!(c.ticks().is_empty());
    }

    #[test]
    fn follow_advances_with_time_and_logs() {
        let mut c = controller();
        c.follow_schedule(
            vec![vec!["a".into()], vec!["b".into()], vec!["c".into()]],
            vec![0.0, 1.0, 2.0],
            100.0,
        );
        assert_eq!(c.follow_ids_at(100.0), Some(vec!["a".to_string()]));
        // Each newly reached group queues its first id for ensureVisible,
        // once (the Swift follow loop's setState 'current' per group).
        assert_eq!(c.take_pending_visible().as_deref(), Some("a"));
        assert_eq!(c.follow_ids_at(100.5), Some(vec!["a".to_string()]));
        assert!(c.take_pending_visible().is_none());
        assert_eq!(c.follow_ids_at(101.5), Some(vec!["b".to_string()]));
        assert_eq!(c.take_pending_visible().as_deref(), Some("b"));
        assert_eq!(c.follow_ids_at(102.5), Some(vec!["c".to_string()]));
        assert_eq!(c.follow_cursor_id().as_deref(), Some("c"));
        assert_eq!(c.follow_log(), [0, 1, 2]);
        c.cancel_follow();
        assert!(c.follow_ids_at(103.0).is_none());
        assert!(c.follow_cursor_id().is_none());
    }
}
