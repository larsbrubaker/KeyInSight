//! The repertoire Library sheet — `UI/LibrarySheet.swift` as a 700×560
//! modal: bundled pieces grouped one row per song (one- and two-hand
//! editions share the row as separate Play buttons), live title search,
//! hands filter, sort picker, per-song play stats, and MusicXML import on
//! platforms that provide a file picker.
//!
//! The list logic (grouping, search, sort, titles) is pure and lives in
//! [`model`]; this module is the chrome. Filter state lives in shared
//! cells; every change bumps `list_generation` so the [`Rebuilder`]
//! re-runs the query — the SwiftUI `@State` → body re-evaluation.

#[cfg(test)]
mod layout_tests;
mod model;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use agg_gui::color::Color;
use agg_gui::geometry::Size;
use agg_gui::layout_props::Insets;
use agg_gui::widget::Widget;
use agg_gui::widgets::{
    Button, ComboBox, Conditional, Container, FlexColumn, FlexRow, Label, LabelAlign, ModalSheet,
    Padding, Rebuilder, ScrollView, SegmentedControl, Separator, SizedBox, Spacer, TextField,
};

use crate::score::{MusicXmlImporter, RepertoireLibrary, RepertoirePiece};
use crate::ui::app::SharedPlatform;
use crate::ui::fonts::{icon, size, UiFonts};
use crate::ui::palette;
use crate::ui::side_panel::{watch_cell, SidePanelCells};
use crate::ui::{DynamicLabel, InfoRow, InfoRows};

use super::Engine;
use model::{HandsFilter, SongEntry, SortOrder};

/// The Swift ideal frame (`idealWidth: 700, idealHeight: 560`).
const SHEET_SIZE: Size = Size {
    width: 700.0,
    height: 560.0,
};

/// The Swift `@State` trio driving the list.
struct Filter {
    search: Rc<RefCell<String>>,
    hands: Rc<Cell<HandsFilter>>,
    sort: Rc<Cell<SortOrder>>,
    /// Bumped on every filter change; the list Rebuilder keys on it.
    list_generation: Rc<Cell<u64>>,
    /// Bumped when the search text is reset from outside the field (the
    /// clear button) so the TextField is recreated with the empty buffer.
    field_generation: Rc<Cell<u64>>,
    /// `songs.count` from the last list build, for the footer.
    shown: Rc<Cell<usize>>,
    /// `SidePanelCells::library_generation`: bumped by every open of the
    /// sheet so the rows' baked-in stats lines are rebuilt (SwiftUI
    /// re-evaluates the body on appear).
    open_generation: Rc<Cell<u64>>,
}

impl Filter {
    fn new(open_generation: Rc<Cell<u64>>) -> Self {
        Self {
            search: Rc::new(RefCell::new(String::new())),
            hands: Rc::new(Cell::new(HandsFilter::All)),
            sort: Rc::new(Cell::new(SortOrder::Title)),
            list_generation: Rc::new(Cell::new(0)),
            field_generation: Rc::new(Cell::new(0)),
            shown: Rc::new(Cell::new(0)),
            open_generation,
        }
    }

    fn changed(&self) {
        self.list_generation.set(self.list_generation.get() + 1);
        agg_gui::animation::request_draw();
    }

    /// The list Rebuilder's key: a filter change or a (re)open rebuilds.
    fn list_key(&self) -> u64 {
        self.list_generation.get() + self.open_generation.get()
    }
}

pub fn build_library_sheet(
    engine: &Engine,
    fonts: &UiFonts,
    cells: &SidePanelCells,
    platform: &SharedPlatform,
) -> Box<dyn Widget> {
    let visible = Rc::clone(&cells.show_library);
    let import_error: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));
    let filter = Rc::new(Filter::new(Rc::clone(&cells.library_generation)));
    let all_songs: Rc<Vec<SongEntry>> = Rc::new(model::all_songs(&RepertoireLibrary::bundled()));

    let mut column = FlexColumn::new().with_gap(0.0);
    column = column.add(Box::new(header(engine, fonts, &visible, platform, &import_error)));
    column = column.add(Box::new(filter_bar(fonts, &filter)));

    // Import error line (visible while non-empty; the specific reason
    // per OQ-10, never silent stripping).
    {
        let watch = Rc::clone(&import_error);
        let has_error = watch_cell(move || !watch.borrow().is_empty());
        let text = Rc::clone(&import_error);
        let error_rows = InfoRows::new(fonts, move || {
            let error = text.borrow();
            if error.is_empty() {
                Vec::new()
            } else {
                vec![InfoRow::text(error.clone(), size::CALLOUT)
                    .with_icon(icon::WARNING)
                    .with_color(palette::RED)]
            }
        });
        column = column.add(Box::new(Conditional::new(
            has_error,
            Box::new(Padding::new(
                Insets {
                    left: 14.0,
                    right: 14.0,
                    top: 0.0,
                    bottom: 8.0,
                },
                Box::new(error_rows),
            )),
        )));
    }

    column = column.add(Box::new(Separator::horizontal().with_line_inset(0.0)));

    // The song list (or the empty state), rebuilt on every filter change.
    {
        let key_filter = Rc::clone(&filter);
        let build_engine = Rc::clone(engine);
        let build_fonts = fonts.clone();
        let build_visible = Rc::clone(&visible);
        let build_filter = Rc::clone(&filter);
        let build_songs = Rc::clone(&all_songs);
        let list = Rebuilder::new(
            move || key_filter.list_key(),
            move || {
                song_list(
                    &build_engine,
                    &build_fonts,
                    &build_visible,
                    &build_filter,
                    &build_songs,
                )
            },
        );
        column = column.add_flex(Box::new(list), 1.0);
    }

    // Footer: Divider + "\(songs.count) of \(allSongs.count) songs".
    column = column.add(Box::new(Separator::horizontal().with_line_inset(0.0)));
    {
        let shown = Rc::clone(&filter.shown);
        let total = all_songs.len();
        column = column.add(Box::new(
            Padding::new(
                Insets {
                    left: 6.0,
                    right: 6.0,
                    top: 6.0,
                    bottom: 6.0,
                },
                Box::new(
                    DynamicLabel::new(
                        move || model::footer_text(shown.get(), total),
                        Arc::clone(&fonts.regular),
                    )
                    .with_font_size(size::CAPTION)
                    .with_dim(true)
                    .with_align(LabelAlign::Center),
                ),
            ),
        ));
    }

    Box::new(ModalSheet::new(visible, Box::new(column)).with_panel_size(SHEET_SIZE))
}

/// Header: title + Import MusicXML… + Done (padding 14).
fn header(
    engine: &Engine,
    fonts: &UiFonts,
    visible: &Rc<Cell<bool>>,
    platform: &SharedPlatform,
    import_error: &Rc<RefCell<String>>,
) -> FlexRow {
    let mut header = FlexRow::new().with_gap(8.0).with_padding(14.0);
    header = header.add(Box::new(
        Label::new("Library", Arc::clone(&fonts.bold)).with_font_size(size::TITLE2),
    ));
    header = header.add_flex(Box::new(crate::ui::hspacer()), 1.0);
    if platform.supports_musicxml_import() {
        let import_engine = Rc::clone(engine);
        let import_visible = Rc::clone(visible);
        let import_platform = Rc::clone(platform);
        let error = Rc::clone(import_error);
        header = header.add(Box::new(
            Button::new("Import MusicXML…", Arc::clone(&fonts.regular))
                .with_subtle()
                .with_active_fn(|| false)
                .on_click(move || {
                    error.borrow_mut().clear();
                    let engine = Rc::clone(&import_engine);
                    let visible = Rc::clone(&import_visible);
                    let error = Rc::clone(&error);
                    import_platform.open_musicxml(Box::new(move |data, name| {
                        match MusicXmlImporter::parse(&data, &name) {
                            Ok(imported) => {
                                engine.borrow_mut().start_piece(RepertoirePiece {
                                    slug: format!("import:{name}"),
                                    title: imported.title,
                                    exercise: imported.exercise,
                                });
                                visible.set(false);
                            }
                            Err(err) => {
                                *error.borrow_mut() = err.to_string();
                            }
                        }
                        agg_gui::animation::request_draw();
                    }));
                }),
        ));
    }
    let close = Rc::clone(visible);
    header.add(Box::new(
        // `.keyboardShortcut(.cancelAction)`: Esc closes.
        Button::new("Done", Arc::clone(&fonts.regular))
            .with_subtle()
            .with_active_fn(|| false)
            .with_cancel_action()
            .on_click(move || {
                close.set(false);
                agg_gui::animation::request_draw();
            }),
    ))
}

/// Segment index of a hands filter in `HandsFilter::ALL`.
fn hands_index(hands: HandsFilter) -> usize {
    HandsFilter::ALL
        .iter()
        .position(|h| *h == hands)
        .unwrap_or(0)
}

/// `HStack(spacing: 10) { search box; hands picker; Sort picker }`,
/// horizontal padding, bottom 10.
fn filter_bar(fonts: &UiFonts, filter: &Rc<Filter>) -> Padding {
    let mut bar = FlexRow::new().with_gap(10.0);
    bar = bar.add_flex(Box::new(search_box(fonts, filter)), 1.0);

    // `Picker("", selection: $handsFilter).pickerStyle(.segmented)
    // .fixedSize()`: All | One hand | Two hands at its natural width. The
    // filter's `hands` cell is the model; the control's index cell maps
    // through `HandsFilter::ALL` (the filter only changes here).
    {
        let selected = Rc::new(Cell::new(hands_index(filter.hands.get())));
        let click = Rc::clone(filter);
        let labels: Vec<&str> = HandsFilter::ALL.iter().map(|h| h.raw_value()).collect();
        bar = bar.add(Box::new(
            SegmentedControl::new(labels, selected, Arc::clone(&fonts.regular)).on_change(
                move |index| {
                    click.hands.set(HandsFilter::ALL[index]);
                    click.changed();
                },
            ),
        ));
    }

    // `Picker("Sort", …)` — label + menu.
    bar = bar.add(Box::new(
        Label::new("Sort", Arc::clone(&fonts.regular)).with_font_size(size::CALLOUT),
    ));
    {
        let click = Rc::clone(filter);
        let names: Vec<&str> = SortOrder::ALL.iter().map(|s| s.raw_value()).collect();
        let selected = SortOrder::ALL
            .iter()
            .position(|s| *s == filter.sort.get())
            .unwrap_or(0);
        bar = bar.add(Box::new(
            SizedBox::new().with_width(110.0).with_child(Box::new(
                ComboBox::new(names, selected, Arc::clone(&fonts.regular))
                    .with_font_size(size::CALLOUT)
                    .on_change(move |index| {
                        if let Some(sort) = SortOrder::ALL.get(index) {
                            click.sort.set(*sort);
                            click.changed();
                        }
                    }),
            )),
        ));
    }

    Padding::new(
        Insets {
            left: 14.0,
            right: 14.0,
            top: 0.0,
            bottom: 10.0,
        },
        Box::new(bar),
    )
}

/// `magnifier + TextField("Search songs") + xmark.circle.fill` on a
/// gray 0.12 rounded-7 background, padding 6.
fn search_box(fonts: &UiFonts, filter: &Rc<Filter>) -> Container {
    let mut row = FlexRow::new().with_gap(5.0);
    row = row.add(Box::new(
        Label::new(icon::MAGNIFIER.to_string(), Arc::clone(&fonts.icons))
            .with_font_size(size::CALLOUT)
            .with_dim(true),
    ));
    {
        let version = Rc::clone(&filter.field_generation);
        let build_filter = Rc::clone(filter);
        let font = Arc::clone(&fonts.regular);
        row = row.add_flex(
            Box::new(Rebuilder::new(
                move || version.get(),
                move || {
                    let change = Rc::clone(&build_filter);
                    Box::new(
                        TextField::new(Arc::clone(&font))
                            .with_font_size(size::CALLOUT)
                            .with_placeholder("Search songs")
                            .with_text(build_filter.search.borrow().clone())
                            .on_change(move |text| {
                                *change.search.borrow_mut() = text.to_string();
                                change.changed();
                            }),
                    )
                },
            )),
            1.0,
        );
    }
    {
        let watch = Rc::clone(&filter.search);
        let non_empty = watch_cell(move || !watch.borrow().is_empty());
        let click = Rc::clone(filter);
        row = row.add(Box::new(Conditional::new(
            non_empty,
            Box::new(
                Button::new("", Arc::clone(&fonts.regular))
                    .with_ghost()
                    .with_active_fn(|| false)
                    .with_compact()
                    .with_icon(icon::XMARK_CIRCLE, Arc::clone(&fonts.icons))
                    .on_click(move || {
                        click.search.borrow_mut().clear();
                        click.field_generation.set(click.field_generation.get() + 1);
                        click.changed();
                    }),
            ),
        )));
    }
    Container::new()
        .with_background(Color::rgba(0.5, 0.5, 0.5, 0.12))
        .with_corner_radius(7.0)
        // Without this the Container reports the full available height
        // (its legacy stretch default), the toolbar row swells to the
        // whole panel, and the list plus footer are pushed off the
        // bottom of the sheet.
        .with_fit_height(true)
        .with_inner_padding(Insets {
            left: 6.0,
            right: 6.0,
            top: 6.0,
            bottom: 6.0,
        })
        .add(Box::new(row))
}

/// The filtered, sorted rows — or the centered empty state.
fn song_list(
    engine: &Engine,
    fonts: &UiFonts,
    visible: &Rc<Cell<bool>>,
    filter: &Rc<Filter>,
    all_songs: &Rc<Vec<SongEntry>>,
) -> Box<dyn Widget> {
    let search = filter.search.borrow().clone();
    let songs = model::filter_and_sort(all_songs, filter.hands.get(), &search, filter.sort.get());
    filter.shown.set(songs.len());

    if songs.is_empty() {
        return Box::new(
            FlexColumn::new()
                .with_gap(0.0)
                .add_flex(Box::new(Spacer::new()), 1.0)
                .add(Box::new(
                    Label::new(model::empty_state_text(&search), Arc::clone(&fonts.regular))
                        .with_font_size(size::BODY)
                        .with_dim(true)
                        .with_align(LabelAlign::Center),
                ))
                .add_flex(Box::new(Spacer::new()), 1.0),
        );
    }

    let hands = filter.hands.get();
    let mut list = FlexColumn::new().with_gap(4.0).with_padding(10.0);
    for entry in songs {
        list = list.add(song_row(engine, fonts, visible, entry, hands));
    }
    Box::new(ScrollView::new(Box::new(list)))
}

/// `title + detail + stats | Play / One hand | Two hands` — one List row.
fn song_row(
    engine: &Engine,
    fonts: &UiFonts,
    visible: &Rc<Cell<bool>>,
    entry: SongEntry,
    hands: HandsFilter,
) -> Box<dyn Widget> {
    let mut text = FlexColumn::new()
        .with_fit_width(true)
        .with_gap(2.0)
        .add(Box::new(
            Label::new(entry.title(), Arc::clone(&fonts.bold)).with_font_size(size::BODY),
        ))
        .add(Box::new(
            Label::new(model::piece_detail(&entry.base), Arc::clone(&fonts.regular))
                .with_font_size(size::CALLOUT)
                .with_dim(true),
        ));
    {
        let engine = engine.borrow();
        if let Some((plays, best)) =
            model::best_stats(&entry, |slug| engine.piece_stats(slug))
        {
            text = text.add(Box::new(
                Label::new(model::stats_line(plays, best), Arc::clone(&fonts.regular))
                    .with_font_size(size::CAPTION)
                    .with_dim(true),
            ));
        }
    }

    let mut row = FlexRow::new()
        .with_gap(10.0)
        .with_padding(4.0)
        .add(Box::new(text))
        .add_flex(Box::new(crate::ui::hspacer()), 1.0);
    if hands != HandsFilter::TwoHands {
        let label = if entry.two_hands.is_none() { "Play" } else { "One hand" };
        row = row.add(play_button(engine, fonts, visible, label, entry.base.clone()));
    }
    if let Some(two_hands) = entry.two_hands.clone() {
        if hands != HandsFilter::OneHand {
            row = row.add(play_button(engine, fonts, visible, "Two hands", two_hands));
        }
    }
    Box::new(row)
}

/// `play(piece)`: start it and dismiss.
fn play_button(
    engine: &Engine,
    fonts: &UiFonts,
    visible: &Rc<Cell<bool>>,
    label: &str,
    piece: RepertoirePiece,
) -> Box<dyn Widget> {
    let engine = Rc::clone(engine);
    let close = Rc::clone(visible);
    Box::new(
        Button::new(label, Arc::clone(&fonts.regular))
            .with_subtle()
            .with_active_fn(|| false)
            .with_compact()
            .on_click(move || {
                engine.borrow_mut().start_piece(piece.clone());
                close.set(false);
                agg_gui::animation::request_draw();
            }),
    )
}

#[cfg(test)]
mod tests {
    use super::{hands_index, Cell, Filter, HandsFilter, Rc};

    /// The segmented filter's index cell maps through `HandsFilter::ALL`
    /// both ways.
    #[test]
    fn hands_segments_round_trip() {
        let labels: Vec<&str> = HandsFilter::ALL.iter().map(|h| h.raw_value()).collect();
        assert_eq!(labels, ["All", "One hand", "Two hands"]);
        for (i, hands) in HandsFilter::ALL.iter().enumerate() {
            assert_eq!(hands_index(*hands), i);
            assert_eq!(HandsFilter::ALL[hands_index(*hands)], *hands);
        }
    }

    /// Opening the sheet rebuilds the list (fresh stats lines), as does a
    /// filter change.
    #[test]
    fn list_key_changes_on_open_and_on_filter_change() {
        let opens = Rc::new(Cell::new(0));
        let filter = Filter::new(Rc::clone(&opens));
        assert_eq!(filter.list_key(), 0);
        opens.set(1); // the Library button / `open_library`
        assert_eq!(filter.list_key(), 1);
        assert_eq!(filter.list_key(), 1);
        filter.list_generation.set(filter.list_generation.get() + 1);
        assert_eq!(filter.list_key(), 2);
        opens.set(2);
        assert_eq!(filter.list_key(), 3);
    }
}
