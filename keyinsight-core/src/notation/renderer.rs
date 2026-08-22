//! Wraps the verovio-rust toolkit: MusicXML in, an engraved layout +
//! time-ordered note ids out.
//!
//! Ports `Notation/NotationRenderer.swift`. Where the Swift renderer got
//! SVG + a JSON timemap from C++ Verovio, this one holds the toolkit's
//! layout directly; qstamps stay quarter-note based (units / 2) so the
//! session engine's onset binding matches the Swift math line for line.

use verovio_rust::{Breaks, LayoutOptions, Toolkit};

/// One engraving result.
pub struct Rendered {
    /// Per-note element ids in playback order (from the timemap).
    pub note_ids: Vec<String>,
    /// Timemap onset groups: quarter-note stamp → ids sounding at that
    /// moment (document order — treble voice then bass).
    pub note_groups: Vec<(f64, Vec<String>)>,
}

/// The staff height (five lines, layout px) a page reads at on screen:
/// the Swift page showed its 1400-unit score at `width: 100%`, which put
/// a staff at about 40 px in the training view, and the engraving's own
/// unit (staff space 10 → staff 40) matches it at display scale 1.
pub const READING_STAFF_PX: f64 = 40.0;

/// How the current engraving is fitted to its widget.
#[derive(Debug, Clone, Copy, PartialEq)]
enum FitTarget {
    /// Fit the whole score into `(w, h)` at a uniform scale (whole px).
    View(f64, f64),
    /// Page: wrap systems at `view_w` (whole px) for a fixed display
    /// `scale`; the score grows downward and the widget scrolls it.
    Page { view_w: f64, scale: f64 },
    /// Fit into `(w, h)`, falling back to a page at `scale` when the
    /// fitted score would read smaller than that.
    Auto { w: f64, h: f64, scale: f64 },
}

pub struct NotationRenderer {
    toolkit: Toolkit,
    layout_options: LayoutOptions,
    /// The widget fit the engraving is laid out for.
    fit: Option<FitTarget>,
    /// Some(scale) when the current layout is a page (wrapped at the
    /// widget width for a fixed reading scale) rather than a fitted view.
    page_scale: Option<f64>,
    /// The layout mode of the current engraving (see [`Self::render_with`]).
    feed: bool,
}

impl Default for NotationRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl NotationRenderer {
    pub fn new() -> Self {
        Self {
            toolkit: Toolkit::new(),
            layout_options: LayoutOptions::default(),
            fit: None,
            page_scale: None,
            feed: false,
        }
    }

    /// MIDI pitch of a rendered note id, from the toolkit's own model —
    /// pins the id-order assumptions without duplicating pitch math.
    pub fn midi_pitch(&self, id: &str) -> Option<u8> {
        self.toolkit.note_midi(id)
    }

    /// Engrave with the automatic (justified page) layout; returns None
    /// when the input is outside the supported subset (the Swift renderer
    /// returned nil on toolkit failure).
    pub fn render(&mut self, music_xml: &str) -> Option<Rendered> {
        self.render_with(music_xml, false)
    }

    /// `feed` (survival): honor `<print new-system="yes"/>` breaks from
    /// the encoder AND space notes linearly in time, so equal-duration
    /// measures get near-equal widths and barlines line up across lines —
    /// a feed, not a justified page. Otherwise the toolkit breaks and
    /// spaces automatically. Set every call — the Swift toolkit options
    /// were sticky, and the layout options here persist the same way.
    pub fn render_with(&mut self, music_xml: &str, feed: bool) -> Option<Rendered> {
        self.feed = feed;
        let options = &mut self.layout_options;
        if feed {
            options.breaks = Breaks::Encoded;
            options.spacing_linear = 0.3;
            options.spacing_non_linear = 1.0;
            // Every two-bar row justifies to the lane, the last included —
            // a ragged last line would break the feed's fixed barlines.
            options.min_last_justification = 0.0;
        } else {
            options.breaks = Breaks::Auto;
            options.spacing_linear = 0.25;
            options.spacing_non_linear = 0.6;
            options.min_last_justification = 0.8;
        }
        self.toolkit.load_music_xml(music_xml).ok()?;
        self.toolkit.layout(&self.layout_options);
        self.apply_fit();
        let layout = self.toolkit.current_layout()?;

        let mut note_ids: Vec<String> = Vec::new();
        let mut note_groups: Vec<(f64, Vec<String>)> = Vec::new();
        for moment in &layout.timemap {
            if moment.note_ids.is_empty() {
                continue;
            }
            note_ids.extend(moment.note_ids.iter().cloned());
            note_groups.push((moment.onset_units as f64 / 2.0, moment.note_ids.clone()));
        }
        Some(Rendered {
            note_ids,
            note_groups,
        })
    }

    /// The toolkit holding the current engraving (widget painting and
    /// bounds queries go through it).
    pub fn toolkit(&self) -> &Toolkit {
        &self.toolkit
    }

    /// The engraving's staff space (layout px) — the distance between
    /// adjacent staff lines every other length scales off.
    pub fn staff_space(&self) -> f64 {
        self.layout_options.staff_space
    }

    /// The system width (layout px) the current engraving wraps and
    /// justifies to; `None` engraves one endless, unjustified system.
    pub fn system_width(&self) -> Option<f64> {
        self.layout_options.system_width
    }

    /// Wrap systems at `width` layout pixels: long scores flow onto
    /// multiple rows. Element ids are stable across relayouts, so
    /// feedback coloring and cursor state carry over.
    pub fn set_system_width(&mut self, width: f64) {
        // Whole pixels, and never so narrow that a single measure can't
        // fit — avoids relayout churn from sub-pixel resize noise.
        let width = Some(width.round().max(200.0));
        if self.layout_options.system_width == width {
            return;
        }
        self.layout_options.system_width = width;
        if self.toolkit.current_layout().is_some() {
            self.toolkit.layout(&self.layout_options);
        }
    }

    /// Fit the engraving to a widget viewport, like the Swift page
    /// reflowing on resize: try a few wrap widths and keep the one whose
    /// fitted (uniform) scale reads largest. Re-runs when the viewport
    /// changes and after each new score engraves.
    pub fn fit_view(&mut self, view_w: f64, view_h: f64) {
        let fit = FitTarget::View(view_w.round().max(1.0), view_h.round().max(1.0));
        if self.fit == Some(fit) {
            return;
        }
        self.fit = Some(fit);
        self.apply_fit();
    }

    /// Page layout: wrap systems at the widget width for a fixed reading
    /// scale — a staff `target_staff_px` tall on screen — so a long score
    /// flows down a scrollable page instead of shrinking to fit. Returns
    /// the display scale the widget paints at. Re-runs after each new
    /// score engraves, like [`Self::fit_view`].
    pub fn fit_page(&mut self, view_w: f64, target_staff_px: f64) -> f64 {
        let scale = self.reading_scale(target_staff_px);
        let fit = FitTarget::Page {
            view_w: view_w.round().max(1.0),
            scale,
        };
        if self.fit != Some(fit) {
            self.fit = Some(fit);
            self.apply_fit();
        }
        scale
    }

    /// Training: fit the score to the viewport like [`Self::fit_view`],
    /// but when the fitted scale would read smaller than a staff
    /// `target_staff_px` tall, lay it out as a page at that reading scale
    /// instead (see [`Self::page_scale`]). Short exercises fit; long
    /// pieces scroll.
    pub fn fit_auto(&mut self, view_w: f64, view_h: f64, target_staff_px: f64) {
        let fit = FitTarget::Auto {
            w: view_w.round().max(1.0),
            h: view_h.round().max(1.0),
            scale: self.reading_scale(target_staff_px),
        };
        if self.fit == Some(fit) {
            return;
        }
        self.fit = Some(fit);
        self.apply_fit();
    }

    /// The display scale at which a staff is `target_staff_px` tall.
    pub fn reading_scale(&self, target_staff_px: f64) -> f64 {
        (target_staff_px / (4.0 * self.layout_options.staff_space)).max(0.05)
    }

    /// Some(display scale) while the current layout is a page — laid out
    /// by [`Self::fit_page`], or by [`Self::fit_auto`] falling back — so
    /// the widget paints at that scale and scrolls; `None` = fitted view.
    pub fn page_scale(&self) -> Option<f64> {
        self.page_scale
    }

    /// The uniform display scale that fits the current engraving into
    /// `(view_w, view_h)`, capped so small exercises don't balloon. The
    /// widget paints at this scale; the feed/auto probes compare it.
    pub fn display_scale(&self, view_w: f64, view_h: f64) -> Option<f64> {
        let layout = self.toolkit.current_layout()?;
        Some((view_w / layout.width).min(view_h / layout.height).min(1.6))
    }

    fn apply_fit(&mut self) {
        let Some(fit) = self.fit else {
            return;
        };
        if self.toolkit.current_layout().is_none() {
            return;
        }
        self.page_scale = None;
        let (view_w, view_h) = match fit {
            FitTarget::View(w, h) | FitTarget::Auto { w, h, .. } => (w, h),
            FitTarget::Page { view_w, scale } => {
                self.layout_page(view_w, scale);
                return;
            }
        };
        if self.feed {
            // The encoder owns the breaks; the lane is simply the widget
            // width (layout units are view px at scale 1), and only the
            // display scale is left to choose — at paint, from the layout.
            self.layout_options.system_width = Some(view_w.round().max(200.0));
            self.toolkit.layout(&self.layout_options);
            return;
        }
        // Wider rows = fewer, shorter systems; narrower rows use the full
        // width. The best trade depends on the score, so measure it.
        let mut best: (f64, Option<f64>) = (f64::MIN, None);
        for factor in [1.0, 1.5, 2.0, 3.0, 4.0] {
            let candidate = Some((view_w * factor).round().max(200.0));
            self.layout_options.system_width = candidate;
            self.toolkit.layout(&self.layout_options);
            let scale = self.display_scale(view_w, view_h).unwrap_or(f64::MIN);
            if scale > best.0 {
                best = (scale, candidate);
            }
        }
        self.layout_options.system_width = best.1;
        self.toolkit.layout(&self.layout_options);
        if let FitTarget::Auto { scale, .. } = fit {
            // Too small to read fitted: page it at the reading scale.
            if best.0 < scale {
                self.layout_page(view_w, scale);
            }
        }
    }

    /// Page layout: one wrap width — the widget width in layout px at the
    /// reading scale (whole px, so sub-pixel resize noise doesn't churn
    /// the layout).
    fn layout_page(&mut self, view_w: f64, scale: f64) {
        self.layout_options.system_width = Some((view_w / scale).round().max(200.0));
        self.toolkit.layout(&self.layout_options);
        self.page_scale = Some(scale);
    }
}
