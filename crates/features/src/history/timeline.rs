use std::ops::Range;

use super::story::plural;
use super::time::format_clock;
use super::types::{VersionCause, VersionId, VersionSummary};

pub const AI_SHARE: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimelineFit {
    pub all: usize,
    pub recent: usize,
    pub cells: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StripCell {
    pub range: Range<usize>,
    pub first_at: i64,
    pub last_at: i64,
    pub accepted: usize,
    pub mostly_ai: bool,
    pub shown: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimelineWindow {
    pub dots: Range<usize>,
    pub strip: Vec<StripCell>,
}

fn overlaps(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

fn cell_of(versions: &[VersionSummary], range: Range<usize>, dots: &Range<usize>) -> StripCell {
    let accepted = versions[range.clone()]
        .iter()
        .filter(|version| matches!(version.cause, VersionCause::Accepted { .. }))
        .count();
    StripCell {
        first_at: versions[range.start].at,
        last_at: versions[range.end - 1].at,
        accepted,
        mostly_ai: accepted * AI_SHARE >= range.len(),
        shown: overlaps(&range, dots),
        range,
    }
}

pub fn timeline_window(
    versions: &[VersionSummary],
    focus: Option<VersionId>,
    fit: TimelineFit,
) -> TimelineWindow {
    let len = versions.len();
    if len <= fit.all.max(1) {
        return TimelineWindow {
            dots: 0..len,
            strip: Vec::new(),
        };
    }
    let shown = fit.recent.clamp(1, len);
    let end = focus
        .and_then(|id| versions.iter().position(|version| version.id == id))
        .map_or(len, |index| (index + shown / 2 + 1).clamp(shown, len));
    let dots = end - shown..end;
    let cells = fit.cells.clamp(1, len);
    let strip = (0..cells)
        .map(|cell| {
            cell_of(
                versions,
                len * cell / cells..len * (cell + 1) / cells,
                &dots,
            )
        })
        .collect();
    TimelineWindow { dots, strip }
}

pub fn cell_span_label(cell: &StripCell, now: i64, utc_offset_minutes: i64) -> String {
    let first = format_clock(cell.first_at, now, utc_offset_minutes);
    if cell.range.len() == 1 || cell.first_at == cell.last_at {
        return first;
    }
    let last = format_clock(cell.last_at, cell.first_at, utc_offset_minutes);
    format!("{first} to {last}")
}

pub fn cell_summary(cell: &StripCell) -> String {
    let versions = plural(cell.range.len(), "Version");
    match cell.accepted {
        0 => versions,
        accepted => format!("{versions} · {} accepted", plural(accepted, "Suggestion")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: i64 = 60_000;
    const HOUR: i64 = 60 * MINUTE;

    fn edits(count: i64) -> Vec<VersionSummary> {
        (1..=count)
            .map(|id| VersionSummary {
                id,
                at: id * MINUTE,
                cause: VersionCause::Edited,
                words: 0,
                words_added: 0,
                words_removed: 0,
                saved: None,
            })
            .collect()
    }

    fn fit(all: usize, recent: usize, cells: usize) -> TimelineFit {
        TimelineFit { all, recent, cells }
    }

    fn ranges(window: &TimelineWindow) -> Vec<Range<usize>> {
        window.strip.iter().map(|cell| cell.range.clone()).collect()
    }

    #[test]
    fn shows_every_version_as_a_dot_while_they_fit() {
        let window = timeline_window(&edits(5), None, fit(5, 2, 2));
        assert_eq!(window.dots, 0..5);
        assert!(window.strip.is_empty());
    }

    #[test]
    fn shows_the_latest_versions_as_dots_and_all_of_them_on_the_strip() {
        let window = timeline_window(&edits(10), None, fit(4, 3, 5));
        assert_eq!(window.dots, 7..10);
        assert_eq!(ranges(&window), vec![0..2, 2..4, 4..6, 6..8, 8..10]);
        let shown: Vec<bool> = window.strip.iter().map(|cell| cell.shown).collect();
        assert_eq!(shown, vec![false, false, false, true, true]);
    }

    #[test]
    fn centers_the_dots_on_an_older_focused_version() {
        let window = timeline_window(&edits(10), Some(3), fit(4, 3, 5));
        assert_eq!(window.dots, 1..4);
    }

    #[test]
    fn keeps_the_dots_inside_the_history_at_either_end() {
        assert_eq!(
            timeline_window(&edits(10), Some(1), fit(4, 3, 5)).dots,
            0..3
        );
        assert_eq!(
            timeline_window(&edits(10), Some(10), fit(4, 3, 5)).dots,
            7..10
        );
    }

    #[test]
    fn a_focus_on_a_version_that_is_gone_shows_the_latest() {
        let window = timeline_window(&edits(10), Some(99), fit(4, 3, 5));
        assert_eq!(window.dots, 7..10);
    }

    #[test]
    fn never_makes_more_cells_than_versions() {
        let window = timeline_window(&edits(6), None, fit(4, 2, 50));
        assert_eq!(window.strip.len(), 6);
        assert!(window.strip.iter().all(|cell| cell.range.len() == 1));
    }

    #[test]
    fn a_cell_counts_its_accepted_suggestions() {
        let mut versions = edits(6);
        versions[1].cause = VersionCause::Accepted { changes: vec![] };
        let window = timeline_window(&versions, None, fit(4, 2, 2));
        assert_eq!(window.strip[0].accepted, 1);
        assert_eq!(
            cell_summary(&window.strip[0]),
            "3 Versions · 1 Suggestion accepted"
        );
        assert_eq!(cell_summary(&window.strip[1]), "3 Versions");
        assert_eq!(
            (window.strip[0].first_at, window.strip[0].last_at),
            (MINUTE, 3 * MINUTE)
        );
    }

    #[test]
    fn a_cell_is_mostly_ai_when_a_quarter_of_its_versions_are_accepted_suggestions() {
        let mut versions = edits(16);
        versions[0].cause = VersionCause::Accepted { changes: vec![] };
        versions[8].cause = VersionCause::Accepted { changes: vec![] };
        versions[9].cause = VersionCause::Accepted { changes: vec![] };
        let window = timeline_window(&versions, None, fit(4, 2, 2));
        let mostly_ai: Vec<bool> = window.strip.iter().map(|cell| cell.mostly_ai).collect();
        assert_eq!(mostly_ai, vec![false, true]);
    }

    #[test]
    fn a_span_label_names_the_end_date_only_on_another_day() {
        let cell = |first_at, last_at| StripCell {
            range: 0..2,
            first_at,
            last_at,
            accepted: 0,
            mostly_ai: false,
            shown: false,
        };
        let now = 30 * HOUR;
        assert_eq!(
            cell_span_label(&cell(25 * HOUR, 26 * HOUR + 5 * MINUTE), now, 0),
            "01:00 to 02:05"
        );
        assert_eq!(
            cell_span_label(&cell(HOUR, 2 * HOUR), now, 0),
            "Jan 1, 01:00 to 02:00"
        );
        assert_eq!(
            cell_span_label(&cell(HOUR, 26 * HOUR), now, 0),
            "Jan 1, 01:00 to Jan 2, 02:00"
        );
    }
}
