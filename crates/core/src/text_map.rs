//! The text map: a miniature of the Document beside the column, one bar per row of text, that
//! shows where Suggestions sit and which part of the text is on screen. Rows are display rows,
//! after wrapping, so the map matches what the editor scrolls through.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Text,
    Heading,
    Blank,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapLine {
    pub start: usize,
    pub len: usize,
    pub first_row: usize,
    pub rows: usize,
    pub kind: LineKind,
}

/// A Suggestion as the map sees it: the display row it starts on, and how it reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapMark {
    pub row: usize,
    pub tone: MarkTone,
}

/// Ordered by how much a bar must stand out when rows share one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MarkTone {
    Text,
    Heading,
    Suggestion,
    Failed,
    Active,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapBar {
    pub top: f32,
    pub height: f32,
    /// Share of the map's width, from 0 to 1.
    pub width: f32,
    pub tone: MarkTone,
}

/// The rows on screen: the first one, possibly in part, and how many fit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub first_row: f32,
    pub rows: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextMap {
    pub bars: Vec<MapBar>,
    pub band_top: f32,
    pub band_height: f32,
    pub height: f32,
    /// Pixels per display row.
    pub pitch: f32,
}

pub const MAX_PITCH: f32 = 4.;
const MIN_WIDTH: f32 = 0.2;

/// The lines of the text, without their rows: where each starts, how long, what it looks like.
/// It reads the text only, so it can run apart from the editor; [`fill_rows`] adds the rows.
pub fn line_shapes(text: &str) -> Vec<MapLine> {
    let mut start = 0;
    text.split('\n')
        .map(|line| {
            let trimmed = line.trim();
            let kind = if trimmed.is_empty() {
                LineKind::Blank
            } else if trimmed.starts_with('#') {
                LineKind::Heading
            } else {
                LineKind::Text
            };
            let item = MapLine {
                start,
                len: line.len(),
                first_row: 0,
                rows: 0,
                kind,
            };
            start += line.len() + 1;
            item
        })
        .collect()
}

/// Gives each line of [`line_shapes`] its rows, and the row it starts on.
pub fn fill_rows(mut lines: Vec<MapLine>, rows_of: impl Fn(usize) -> usize) -> Vec<MapLine> {
    let mut first_row = 0;
    for (index, line) in lines.iter_mut().enumerate() {
        let rows = rows_of(index);
        line.first_row = first_row;
        line.rows = rows;
        first_row += rows;
    }
    lines
}

pub fn map_lines(text: &str, rows_of: impl Fn(usize) -> usize) -> Vec<MapLine> {
    fill_rows(line_shapes(text), rows_of)
}

pub fn total_rows(lines: &[MapLine]) -> usize {
    lines.last().map_or(0, |line| line.first_row + line.rows)
}

/// The line an offset falls on and its byte column there.
pub fn locate(lines: &[MapLine], offset: usize) -> (usize, usize) {
    let index = lines
        .partition_point(|line| line.start <= offset)
        .saturating_sub(1);
    lines.get(index).map_or((0, 0), |line| {
        (index, offset.saturating_sub(line.start).min(line.len))
    })
}

/// The mark a step from the screen lands on, when the caret is off screen: going down, the first
/// one from the top edge on; going up, the last one before the bottom edge. `rows` are in
/// document order.
pub fn step_in_view(rows: &[usize], view: View, forward: bool) -> Option<usize> {
    let top = view.first_row;
    let bottom = view.first_row + view.rows;
    if forward {
        rows.iter().position(|&row| (row as f32) + 1. > top)
    } else {
        rows.iter().rposition(|&row| (row as f32) < bottom)
    }
}

/// How many marks sit above and below the rows on screen.
pub fn off_screen(rows: &[usize], view: View) -> (usize, usize) {
    let top = view.first_row;
    let bottom = view.first_row + view.rows;
    let above = rows.iter().filter(|&&row| (row as f32) + 1. <= top).count();
    let below = rows.iter().filter(|&&row| (row as f32) >= bottom).count();
    (above, below)
}

fn row_width(line: &MapLine, row: usize, columns: usize) -> f32 {
    let width = |chars: usize| (chars as f32 / columns as f32).clamp(MIN_WIDTH, 1.);
    match line.kind {
        LineKind::Blank => 0.,
        LineKind::Heading => width(line.len).min(0.6),
        LineKind::Text if row + 1 < line.rows => 1.,
        LineKind::Text => width(line.len.saturating_sub(columns * (line.rows - 1))),
    }
}

/// Characters in a full row, read from the lines that wrap; 80 when none does.
fn columns_of(lines: &[MapLine]) -> usize {
    lines
        .iter()
        .filter(|line| line.rows > 1 && line.kind == LineKind::Text)
        .map(|line| line.len.div_ceil(line.rows))
        .max()
        .unwrap_or(80)
        .max(1)
}

/// The map for the rows on screen in `view`. The bars do not depend on the view, so a caller
/// that draws every frame can keep [`text_map_bars`] while the lines, marks and height hold, and
/// only move the band with [`TextMap::with_view`].
pub fn text_map(lines: &[MapLine], marks: &[MapMark], view: View, height: f32) -> TextMap {
    text_map_bars(lines, marks, height).with_view(view)
}

impl TextMap {
    /// This map with its band on the rows of `view`.
    #[must_use]
    pub fn with_view(mut self, view: View) -> Self {
        self.band_top = (view.first_row.max(0.) * self.pitch).min(self.height);
        self.band_height = (view.rows * self.pitch).min(self.height - self.band_top);
        self
    }
}

/// The bars of the map, with the band at the top; the part that reads every line.
pub fn text_map_bars(lines: &[MapLine], marks: &[MapMark], height: f32) -> TextMap {
    let total = total_rows(lines).max(1);
    let pitch = (height / total as f32).min(MAX_PITCH);
    let bar_height = (pitch * 0.5).clamp(1., 2.);
    let columns = columns_of(lines);
    let mut marked: Vec<Option<MarkTone>> = vec![None; total];
    for mark in marks {
        let row = mark.row.min(total - 1);
        marked[row] = marked[row].max(Some(mark.tone));
    }
    let mut bars: Vec<MapBar> = Vec::new();
    for line in lines {
        for row in 0..line.rows {
            let at = line.first_row + row;
            let base = match line.kind {
                LineKind::Heading => MarkTone::Heading,
                _ => MarkTone::Text,
            };
            let tone = marked.get(at).copied().flatten().unwrap_or(base);
            let width = match tone {
                MarkTone::Text | MarkTone::Heading => row_width(line, row, columns),
                _ => row_width(line, row, columns).max(0.5),
            };
            if width == 0. {
                continue;
            }
            let top = (at as f32 * pitch).floor();
            match bars.last_mut() {
                Some(last) if last.top == top => {
                    last.tone = last.tone.max(tone);
                    last.width = last.width.max(width);
                }
                _ => bars.push(MapBar {
                    top,
                    height: bar_height,
                    width,
                    tone,
                }),
            }
        }
    }
    TextMap {
        bars,
        band_top: 0.,
        band_height: 0.,
        height: total as f32 * pitch,
        pitch,
    }
}

/// The first row to show so that the row under `y` on the map sits mid-screen.
pub fn scroll_row_for(map: &TextMap, y: f32, view_rows: f32) -> f32 {
    if map.pitch <= 0. {
        return 0.;
    }
    (y / map.pitch - view_rows / 2.).max(0.)
}

/// Where a jump to `row` leaves it: a third of the way down the screen, unless it already sits
/// between the top edge and three quarters of the way down. `None` keeps the scroll.
pub fn reveal_first_row(view: View, row: usize) -> Option<f32> {
    let row = row as f32;
    let shown = row >= view.first_row.max(0.) + 1. && row <= view.first_row + view.rows * 0.75;
    (!shown).then(|| (row - view.rows / 3.).max(0.))
}

/// The mark whose bar is within `reach` pixels of `y`, nearest first.
pub fn mark_near(marks: &[MapMark], map: &TextMap, y: f32, reach: f32) -> Option<usize> {
    marks
        .iter()
        .enumerate()
        .map(|(index, mark)| (index, (mark.row as f32 * map.pitch - y).abs()))
        .filter(|&(_, distance)| distance <= reach)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(index, _)| index)
}

#[cfg(test)]
mod tests {
    #[test]
    fn shapes_then_rows_make_the_same_lines_as_map_lines() {
        let text = "# Title\n\nA paragraph that wraps.\nlast";
        let rows = |index: usize| index + 1;
        assert_eq!(fill_rows(line_shapes(text), rows), map_lines(text, rows));
        assert_eq!(line_shapes(text)[2].rows, 0);
    }

    use super::*;

    fn one_row(_: usize) -> usize {
        1
    }

    fn mark(row: usize) -> MapMark {
        MapMark {
            row,
            tone: MarkTone::Suggestion,
        }
    }

    #[test]
    fn reads_headings_blanks_and_text_with_their_offsets_and_rows() {
        let lines = map_lines(
            "# Title\n\nSome text",
            |index| if index == 2 { 3 } else { 1 },
        );
        let kinds: Vec<_> = lines.iter().map(|line| line.kind).collect();
        assert_eq!(kinds, [LineKind::Heading, LineKind::Blank, LineKind::Text]);
        assert_eq!(lines[2].start, 9);
        assert_eq!(lines[2].first_row, 2);
        assert_eq!(total_rows(&lines), 5);
    }

    #[test]
    fn locates_an_offset_on_its_line_and_column() {
        let lines = map_lines("intro\nsecond line\n", one_row);
        assert_eq!(locate(&lines, 2), (0, 2));
        assert_eq!(locate(&lines, 6), (1, 0));
        assert_eq!(locate(&lines, 9), (1, 3));
        assert_eq!(locate(&lines, 18), (2, 0));
        assert_eq!(locate(&lines, 10_000), (2, 0));
    }

    #[test]
    fn steps_from_the_screen_when_the_caret_is_off_it() {
        let view = View {
            first_row: 10.5,
            rows: 20.,
        };
        let rows = [2, 10, 15, 30, 31, 80];
        assert_eq!(step_in_view(&rows, view, true), Some(1));
        assert_eq!(step_in_view(&rows, view, false), Some(3));
        assert_eq!(step_in_view(&[2, 3], view, true), None);
        assert_eq!(step_in_view(&[50], view, false), None);
    }

    #[test]
    fn counts_marks_above_and_below_the_rows_on_screen() {
        let view = View {
            first_row: 10.5,
            rows: 20.,
        };
        assert_eq!(off_screen(&[2, 9, 10, 15, 30, 31, 80], view), (2, 2));
    }

    #[test]
    fn a_short_text_gets_the_full_pitch_and_a_long_one_fits_the_height() {
        let short = map_lines("a\nb\nc", one_row);
        let view = View {
            first_row: 0.,
            rows: 40.,
        };
        assert_eq!(text_map(&short, &[], view, 400.).pitch, MAX_PITCH);
        let long_text = vec!["word"; 1000].join("\n");
        let long = map_lines(&long_text, one_row);
        let map = text_map(&long, &[], view, 400.);
        assert!((map.pitch - 0.4).abs() < 1e-6);
        assert!(map.height <= 400.);
        assert!(map.bars.len() <= 400, "rows that share a pixel share a bar");
    }

    #[test]
    fn moving_the_view_moves_only_the_band() {
        let lines = map_lines(&vec!["word"; 300].join("\n"), one_row);
        let marks = [mark(7), mark(120)];
        let bars = text_map_bars(&lines, &marks, 400.);
        for first_row in [0., 3.5, 120., 290., 400.] {
            let view = View {
                first_row,
                rows: 40.,
            };
            assert_eq!(
                bars.clone().with_view(view),
                text_map(&lines, &marks, view, 400.)
            );
        }
        let low = bars.clone().with_view(View {
            first_row: 10.,
            rows: 40.,
        });
        let high = bars.with_view(View {
            first_row: 100.,
            rows: 40.,
        });
        assert_eq!(low.bars, high.bars);
        assert!(high.band_top > low.band_top);
    }

    #[test]
    fn a_suggestion_wins_its_shared_bar_and_the_active_one_wins_over_it() {
        let text = vec!["word"; 1000].join("\n");
        let lines = map_lines(&text, one_row);
        let view = View {
            first_row: 0.,
            rows: 40.,
        };
        let marks = [
            mark(500),
            MapMark {
                row: 501,
                tone: MarkTone::Active,
            },
            mark(900),
        ];
        let map = text_map(&lines, &marks, view, 100.);
        let tones: Vec<_> = map
            .bars
            .iter()
            .filter(|bar| bar.tone != MarkTone::Text)
            .map(|bar| bar.tone)
            .collect();
        assert_eq!(tones, [MarkTone::Active, MarkTone::Suggestion]);
    }

    #[test]
    fn blank_lines_draw_no_bar_and_a_wrapped_line_ends_short() {
        let text = format!("{}\n\nshort", "x".repeat(200));
        let lines = map_lines(&text, |index| if index == 0 { 2 } else { 1 });
        let view = View {
            first_row: 0.,
            rows: 10.,
        };
        let map = text_map(&lines, &[], view, 400.);
        let widths: Vec<_> = map.bars.iter().map(|bar| bar.width).collect();
        assert_eq!(map.bars.len(), 3);
        assert_eq!(widths[0], 1.);
        assert_eq!(widths[1], 1.);
        assert!((widths[2] - MIN_WIDTH).abs() < 1e-6);
        assert_eq!(map.bars[2].top, 3. * MAX_PITCH);
    }

    #[test]
    fn the_band_covers_the_rows_on_screen() {
        let text = vec!["word"; 100].join("\n");
        let lines = map_lines(&text, one_row);
        let view = View {
            first_row: 50.,
            rows: 25.,
        };
        let map = text_map(&lines, &[], view, 1000.);
        assert_eq!(map.band_top, 200.);
        assert_eq!(map.band_height, 100.);
        let end = View {
            first_row: 90.,
            rows: 25.,
        };
        let map = text_map(&lines, &[], end, 1000.);
        assert_eq!(map.band_top + map.band_height, map.height);
    }

    #[test]
    fn a_jump_scrolls_only_when_the_row_is_off_or_near_the_edges() {
        let view = View {
            first_row: 100.,
            rows: 30.,
        };
        assert_eq!(reveal_first_row(view, 110), None);
        assert_eq!(reveal_first_row(view, 300), Some(290.));
        assert_eq!(reveal_first_row(view, 127), Some(117.));
        assert_eq!(reveal_first_row(view, 100), Some(90.));
        assert_eq!(reveal_first_row(view, 4), Some(0.));
    }

    #[test]
    fn a_click_centers_its_row_and_finds_the_nearest_suggestion() {
        let text = vec!["word"; 100].join("\n");
        let lines = map_lines(&text, one_row);
        let view = View {
            first_row: 0.,
            rows: 20.,
        };
        let marks = [mark(10), mark(12), mark(60)];
        let map = text_map(&lines, &marks, view, 1000.);
        assert_eq!(scroll_row_for(&map, 200., 20.), 40.);
        assert_eq!(scroll_row_for(&map, 8., 20.), 0.);
        assert_eq!(mark_near(&marks, &map, 47., 6.), Some(1));
        assert_eq!(mark_near(&marks, &map, 150., 6.), None);
    }
}
