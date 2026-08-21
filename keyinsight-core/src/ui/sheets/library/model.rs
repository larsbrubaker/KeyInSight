//! The Library sheet's pure list logic — `LibrarySheet.swift`'s
//! `// MARK: - Grouping, filtering, sorting`: one- and two-hand editions of
//! the same tune share a row, live title search, hands filter, and the
//! three sort orders. No widgets here, so it is unit-tested directly.

use crate::core::PitchSpelling;
use crate::score::RepertoirePiece;

/// The slug suffix that marks a two-hand edition of a base song.
const TWO_HANDS_SUFFIX: &str = "-two-hands";

/// One library row: a song with its available editions (the Swift `id`
/// is `base.slug`).
#[derive(Debug, Clone)]
pub struct SongEntry {
    pub base: RepertoirePiece,
    pub two_hands: Option<RepertoirePiece>,
    /// `base.difficultyIndex`, computed once (the Swift property recomputes
    /// per access; the sort compares it pairwise).
    pub difficulty: f64,
    /// `base.exercise.allSoundedNotes.count`.
    pub length: usize,
}

impl SongEntry {
    pub fn new(base: RepertoirePiece, two_hands: Option<RepertoirePiece>) -> Self {
        let difficulty = base.difficulty_index();
        let length = base.exercise.all_sounded_notes().len();
        Self {
            base,
            two_hands,
            difficulty,
            length,
        }
    }

    /// Row title without the edition suffix.
    pub fn title(&self) -> String {
        base_title(&self.base.title)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    Title,
    Difficulty,
    Length,
}

impl SortOrder {
    pub const ALL: [SortOrder; 3] = [SortOrder::Title, SortOrder::Difficulty, SortOrder::Length];

    /// The Swift `rawValue` — the picker label.
    pub fn raw_value(self) -> &'static str {
        match self {
            SortOrder::Title => "Title",
            SortOrder::Difficulty => "Difficulty",
            SortOrder::Length => "Length",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandsFilter {
    All,
    OneHand,
    TwoHands,
}

impl HandsFilter {
    pub const ALL: [HandsFilter; 3] = [HandsFilter::All, HandsFilter::OneHand, HandsFilter::TwoHands];

    pub fn raw_value(self) -> &'static str {
        match self {
            HandsFilter::All => "All",
            HandsFilter::OneHand => "One hand",
            HandsFilter::TwoHands => "Two hands",
        }
    }
}

/// Pieces paired into songs: `slug` + `slug-two-hands` share a row. A
/// `-two-hands` piece whose base slug is absent stays a standalone row.
pub fn all_songs(pieces: &[RepertoirePiece]) -> Vec<SongEntry> {
    let find = |slug: &str| pieces.iter().find(|piece| piece.slug == slug).cloned();
    pieces
        .iter()
        .filter_map(|piece| {
            if let Some(base_slug) = piece.slug.strip_suffix(TWO_HANDS_SUFFIX) {
                if find(base_slug).is_some() {
                    return None; // shown on its base song's row
                }
            }
            let two_hands = find(&format!("{}{TWO_HANDS_SUFFIX}", piece.slug));
            Some(SongEntry::new(piece.clone(), two_hands))
        })
        .collect()
}

/// The Swift `songs` computed property: hands filter, then search, then
/// sort.
pub fn filter_and_sort(
    all: &[SongEntry],
    hands: HandsFilter,
    search: &str,
    sort: SortOrder,
) -> Vec<SongEntry> {
    let mut result: Vec<SongEntry> = all
        .iter()
        .filter(|entry| match hands {
            HandsFilter::All => true,
            HandsFilter::OneHand => true, // every row has a one-hand edition
            HandsFilter::TwoHands => entry.two_hands.is_some(),
        })
        .cloned()
        .collect();
    if !search.is_empty() {
        let needle = search.to_lowercase();
        result.retain(|entry| entry.title().to_lowercase().contains(&needle));
    }
    match sort {
        SortOrder::Title => result.sort_by_key(|entry| entry.title().to_lowercase()),
        SortOrder::Difficulty => result.sort_by(|a, b| {
            a.difficulty
                .partial_cmp(&b.difficulty)
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        SortOrder::Length => result.sort_by_key(|entry| entry.length),
    }
    result
}

/// Row title without the edition suffix ("Ode to Joy (Two Hands)" pairs
/// under "Ode to Joy").
pub fn base_title(title: &str) -> String {
    title
        .replace(" (Two Hands)", "")
        .replace(", Two Hands)", ")")
}

/// Stats for the most-played edition of the song: `(plays, bestAccuracy)`.
pub fn best_stats(
    entry: &SongEntry,
    piece_stats: impl Fn(&str) -> Option<(i64, f64)>,
) -> Option<(i64, f64)> {
    [Some(&entry.base), entry.two_hands.as_ref()]
        .into_iter()
        .flatten()
        .filter_map(|piece| piece_stats(&piece.slug))
        .max_by_key(|stats| stats.0)
}

/// `"\(plays)× · best \(Int((bestAccuracy * 100).rounded()))%"`.
pub fn stats_line(plays: i64, best_accuracy: f64) -> String {
    format!("{plays}× · best {}%", (best_accuracy * 100.0).round() as i64)
}

/// `"8 measures · 14 notes · C major · difficulty 1.2"`.
pub fn piece_detail(piece: &RepertoirePiece) -> String {
    let exercise = &piece.exercise;
    format!(
        "{} measures · {} notes · {} · difficulty {:.1}",
        exercise.measure_count(),
        exercise.all_sounded_notes().len(),
        PitchSpelling::key_name(exercise.fifths),
        piece.difficulty_index()
    )
}

/// The centered empty-state text.
pub fn empty_state_text(search: &str) -> String {
    if search.is_empty() {
        "No songs match the filter.".to_string()
    } else {
        format!("No songs match “{search}”.")
    }
}

/// The footer count: `"\(songs.count) of \(allSongs.count) songs"`.
pub fn footer_text(shown: usize, total: usize) -> String {
    format!("{shown} of {total} songs")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::{Exercise, NoteDuration, ScoreNote};

    fn piece(slug: &str, title: &str, midis: &[u8]) -> RepertoirePiece {
        let notes = midis
            .iter()
            .map(|&m| ScoreNote::new(Some(m), NoteDuration::Quarter))
            .collect();
        RepertoirePiece {
            slug: slug.to_string(),
            title: title.to_string(),
            exercise: Exercise::new(notes, 4),
        }
    }

    fn ids(songs: &[SongEntry]) -> Vec<&str> {
        songs.iter().map(|s| s.base.slug.as_str()).collect()
    }

    fn pieces() -> Vec<RepertoirePiece> {
        vec![
            piece("ode-to-joy", "Ode to Joy", &[64, 64, 65, 67]),
            piece("ode-to-joy-two-hands", "Ode to Joy (Two Hands)", &[64, 64, 65, 67, 67]),
            piece("amazing-grace", "Amazing Grace", &[60, 65, 69, 65, 69, 67, 65, 62, 60]),
            piece("orphan-two-hands", "Orphan (Arr., Two Hands)", &[60, 62]),
        ]
    }

    #[test]
    fn two_hands_edition_attaches_to_its_base_row() {
        let songs = all_songs(&pieces());
        assert_eq!(ids(&songs), ["ode-to-joy", "amazing-grace", "orphan-two-hands"]);
        let ode = &songs[0];
        assert_eq!(
            ode.two_hands.as_ref().map(|p| p.slug.as_str()),
            Some("ode-to-joy-two-hands")
        );
        assert!(songs[1].two_hands.is_none());
        // A "-two-hands" slug without a base stays a standalone row.
        assert!(songs[2].two_hands.is_none());
        assert_eq!(songs[2].title(), "Orphan (Arr.)");
    }

    #[test]
    fn base_title_strips_the_edition_suffix() {
        assert_eq!(base_title("Ode to Joy (Two Hands)"), "Ode to Joy");
        assert_eq!(base_title("Minuet in G (Bach, Two Hands)"), "Minuet in G (Bach)");
        assert_eq!(base_title("Twinkle Twinkle"), "Twinkle Twinkle");
    }

    #[test]
    fn hands_filter_keeps_only_songs_with_a_two_hand_edition() {
        let songs = all_songs(&pieces());
        let all = filter_and_sort(&songs, HandsFilter::All, "", SortOrder::Title);
        assert_eq!(all.len(), 3);
        let one = filter_and_sort(&songs, HandsFilter::OneHand, "", SortOrder::Title);
        assert_eq!(one.len(), 3, "every row has a one-hand edition");
        let two = filter_and_sort(&songs, HandsFilter::TwoHands, "", SortOrder::Title);
        assert_eq!(ids(&two), ["ode-to-joy"]);
    }

    #[test]
    fn search_is_case_insensitive_on_the_base_title() {
        let songs = all_songs(&pieces());
        let hits = filter_and_sort(&songs, HandsFilter::All, "ODE", SortOrder::Title);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].base.slug, "ode-to-joy");
        // The suffix is not searchable — it is stripped from the row title.
        assert!(filter_and_sort(&songs, HandsFilter::All, "Two Hands", SortOrder::Title).is_empty());
        assert!(filter_and_sort(&songs, HandsFilter::All, "zzz", SortOrder::Title).is_empty());
    }

    #[test]
    fn sort_orders() {
        let songs = all_songs(&pieces());
        let by_title: Vec<String> = filter_and_sort(&songs, HandsFilter::All, "", SortOrder::Title)
            .iter()
            .map(SongEntry::title)
            .collect();
        assert_eq!(by_title, ["Amazing Grace", "Ode to Joy", "Orphan (Arr.)"]);

        let by_length: Vec<usize> =
            filter_and_sort(&songs, HandsFilter::All, "", SortOrder::Length)
                .iter()
                .map(|e| e.length)
                .collect();
        assert!(by_length.windows(2).all(|w| w[0] <= w[1]), "{by_length:?}");

        let by_difficulty: Vec<f64> =
            filter_and_sort(&songs, HandsFilter::All, "", SortOrder::Difficulty)
                .iter()
                .map(|e| e.difficulty)
                .collect();
        assert!(by_difficulty.windows(2).all(|w| w[0] <= w[1]), "{by_difficulty:?}");
    }

    #[test]
    fn best_stats_takes_the_most_played_edition() {
        let songs = all_songs(&pieces());
        let ode = &songs[0];
        let stats = best_stats(ode, |slug| match slug {
            "ode-to-joy" => Some((2, 0.5)),
            "ode-to-joy-two-hands" => Some((5, 0.9)),
            _ => None,
        });
        assert_eq!(stats, Some((5, 0.9)));
        assert_eq!(best_stats(ode, |_| None), None);
        assert_eq!(stats_line(5, 0.876), "5× · best 88%");
    }

    #[test]
    fn detail_line_names_the_key_from_fifths() {
        let mut p = piece("x", "X", &[60, 62, 64, 65, 67, 69, 71, 72]);
        p.exercise.fifths = -2;
        let detail = piece_detail(&p);
        assert!(detail.starts_with("2 measures · 8 notes · B♭ major · difficulty "), "{detail}");
    }

    #[test]
    fn empty_state_and_footer_strings() {
        assert_eq!(empty_state_text(""), "No songs match the filter.");
        assert_eq!(empty_state_text("zz"), "No songs match “zz”.");
        assert_eq!(footer_text(3, 61), "3 of 61 songs");
    }

    /// The bundled library pairs every two-hand edition with its base.
    #[test]
    fn bundled_two_hand_editions_all_have_a_base() {
        let pieces = crate::score::RepertoireLibrary::bundled();
        let songs = all_songs(&pieces);
        let paired = songs.iter().filter(|s| s.two_hands.is_some()).count();
        let two_hand_pieces = pieces
            .iter()
            .filter(|p| p.slug.ends_with(TWO_HANDS_SUFFIX))
            .count();
        assert_eq!(paired, two_hand_pieces);
        assert_eq!(songs.len(), pieces.len() - two_hand_pieces);
    }
}
