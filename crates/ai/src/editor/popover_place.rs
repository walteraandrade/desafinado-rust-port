use super::geometry::{Rect, Viewport};

pub const EDGE_GAP: f64 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Above,
    Below,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PopoverInput {
    pub anchor: Rect,
    pub width: f64,
    pub height: f64,
    pub area: Viewport,
    pub side: Side,
    pub gap: f64,
    pub flip: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub left: f64,
    pub top: f64,
    pub side: Side,
}

fn clamp_axis(start: f64, size: f64, room: f64) -> f64 {
    start.min(room - EDGE_GAP - size).max(EDGE_GAP)
}

fn top_on(side: Side, input: &PopoverInput) -> f64 {
    match side {
        Side::Above => input.anchor.top - input.gap - input.height,
        Side::Below => input.anchor.bottom + input.gap,
    }
}

fn fits(top: f64, input: &PopoverInput) -> bool {
    top >= EDGE_GAP && top + input.height <= input.area.height - EDGE_GAP
}

fn other(side: Side) -> Side {
    match side {
        Side::Above => Side::Below,
        Side::Below => Side::Above,
    }
}

fn room_on(side: Side, input: &PopoverInput) -> f64 {
    match side {
        Side::Above => input.anchor.top - input.gap,
        Side::Below => input.area.height - input.anchor.bottom - input.gap,
    }
}

/// Where a line sits against the visible area: laid out on screen, or past
/// one of its edges.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineAt {
    Above,
    Shown(Rect),
    Below,
}

impl LineAt {
    /// Locates the line holding `offset`, given the offset where the first
    /// laid-out row starts and the line's bounds when the editor laid it out.
    /// The editor lays out a run of rows from the top of the view, so a line
    /// with no bounds that is not above them is past the bottom.
    pub fn locate(offset: usize, first_shown: usize, bounds: Option<Rect>) -> Self {
        if offset < first_shown {
            Self::Above
        } else {
            bounds.map_or(Self::Below, Self::Shown)
        }
    }

    /// The line's top and bottom, with a line past an edge of the area pinned
    /// to that edge.
    fn pinned(self, area: &Viewport) -> (f64, f64) {
        match self {
            Self::Above => (0.0, 0.0),
            Self::Shown(rect) => (rect.top, rect.bottom),
            Self::Below => (area.height, area.height),
        }
    }
}

/// The ends of the selection the Ask AI bubble belongs to, and its head (the
/// end the caret is on).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BubbleEnds {
    pub head: LineAt,
    pub start: LineAt,
    pub end: LineAt,
}

/// How far left of the head the bubble starts.
pub const BUBBLE_INSET: f64 = 12.0;

/// The anchor for the Ask AI bubble: it spans the selection, with ends past
/// the view pinned to its edges, so a selection that runs off screen (Select
/// all in a long document) still anchors inside the area. It starts at the
/// head, or at the text column's left edge when the head is off screen.
pub fn bubble_anchor(ends: BubbleEnds, column_left: f64, area: &Viewport) -> Rect {
    let left = match ends.head {
        LineAt::Shown(rect) => rect.left,
        LineAt::Above | LineAt::Below => column_left,
    } - BUBBLE_INSET;
    let (start_top, start_bottom) = ends.start.pinned(area);
    let (end_top, end_bottom) = ends.end.pinned(area);
    Rect {
        left,
        right: left,
        top: start_top.min(end_top),
        bottom: start_bottom.max(end_bottom),
    }
}

pub fn anchor_visible(anchor: &Rect, area: &Viewport) -> bool {
    anchor.bottom > 0.0 && anchor.top < area.height
}

pub fn place_popover(input: &PopoverInput) -> Option<Placed> {
    if !anchor_visible(&input.anchor, &input.area) {
        return None;
    }
    let preferred = input.side;
    let side = if !input.flip || fits(top_on(preferred, input), input) {
        preferred
    } else if fits(top_on(other(preferred), input), input)
        || room_on(other(preferred), input) > room_on(preferred, input)
    {
        other(preferred)
    } else {
        preferred
    };
    Some(Placed {
        left: clamp_axis(input.anchor.left, input.width, input.area.width),
        top: clamp_axis(top_on(side, input), input.height, input.area.height),
        side,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ends(head: Rect, start: Rect, end: Rect) -> BubbleEnds {
        BubbleEnds {
            head: LineAt::Shown(head),
            start: LineAt::Shown(start),
            end: LineAt::Shown(end),
        }
    }

    #[test]
    fn a_bubble_anchor_runs_from_the_first_line_to_the_last_at_the_head() {
        let first = line(40.0, 100.0);
        let last = line(37.0, 160.0);
        assert_eq!(
            bubble_anchor(ends(last, first, last), 0.0, &AREA),
            Rect {
                left: 25.0,
                right: 25.0,
                top: 100.0,
                bottom: 180.0,
            }
        );
    }

    #[test]
    fn a_bubble_anchor_keeps_a_last_line_reported_above_the_first() {
        let first = line(40.0, 100.0);
        let last = line(10.0, 60.0);
        let anchor = bubble_anchor(ends(first, first, last), 0.0, &AREA);
        assert_eq!((anchor.top, anchor.bottom), (60.0, 120.0));
    }

    #[test]
    fn a_popover_flipped_off_a_span_never_covers_the_span() {
        let first = line(40.0, 30.0);
        let span = bubble_anchor(ends(first, first, line(40.0, 130.0)), 0.0, &AREA);
        let placed = place_popover(&input(span, Side::Above)).unwrap();
        assert_eq!(placed.side, Side::Below);
        assert!(placed.top >= span.bottom);
    }

    const AREA: Viewport = Viewport {
        width: 800.0,
        height: 600.0,
    };

    fn line(left: f64, top: f64) -> Rect {
        Rect {
            left,
            right: left,
            top,
            bottom: top + 20.0,
        }
    }

    fn input(anchor: Rect, side: Side) -> PopoverInput {
        PopoverInput {
            anchor,
            width: WIDTH,
            height: HEIGHT,
            area: AREA,
            side,
            gap: GAP,
            flip: true,
        }
    }

    #[test]
    fn keeps_the_preferred_side_when_it_fits() {
        let placed = place_popover(&input(line(100.0, 300.0), Side::Above)).unwrap();
        assert_eq!(placed.side, Side::Above);
        assert_eq!(placed.top, 300.0 - 6.0 - 200.0);
        assert_eq!(placed.left, 100.0);
    }

    #[test]
    fn flips_below_when_the_line_is_near_the_top() {
        let placed = place_popover(&input(line(100.0, 40.0), Side::Above)).unwrap();
        assert_eq!(placed.side, Side::Below);
        assert_eq!(placed.top, 40.0 + 20.0 + 6.0);
    }

    #[test]
    fn flips_above_when_the_line_is_near_the_bottom() {
        let placed = place_popover(&input(line(100.0, 520.0), Side::Below)).unwrap();
        assert_eq!(placed.side, Side::Above);
        assert_eq!(placed.top, 520.0 - 6.0 - 200.0);
    }

    #[test]
    fn stays_inside_the_area_when_neither_side_fits() {
        let tall = PopoverInput {
            height: 500.0,
            ..input(line(100.0, 280.0), Side::Below)
        };
        let placed = place_popover(&tall).unwrap();
        assert!(placed.top >= EDGE_GAP);
        assert!(placed.top + 500.0 <= AREA.height - EDGE_GAP);
    }

    #[test]
    fn shifts_left_to_keep_the_right_edge_inside() {
        let placed = place_popover(&input(line(700.0, 300.0), Side::Above)).unwrap();
        assert_eq!(placed.left, 800.0 - 8.0 - 360.0);
    }

    #[test]
    fn never_starts_left_of_the_edge_gap() {
        let placed = place_popover(&input(line(-40.0, 300.0), Side::Above)).unwrap();
        assert_eq!(placed.left, EDGE_GAP);
        let narrow = PopoverInput {
            area: Viewport {
                width: 300.0,
                height: 600.0,
            },
            ..input(line(100.0, 300.0), Side::Above)
        };
        assert_eq!(place_popover(&narrow).unwrap().left, EDGE_GAP);
    }

    #[test]
    fn a_strict_side_is_clamped_instead_of_flipped() {
        let strict = PopoverInput {
            flip: false,
            gap: 0.0,
            ..input(line(500.0, 520.0), Side::Below)
        };
        let placed = place_popover(&strict).unwrap();
        assert_eq!(placed.side, Side::Below);
        assert_eq!(placed.top, 600.0 - 8.0 - 200.0);
    }

    const WIDTH: f64 = 360.0;
    const HEIGHT: f64 = 200.0;
    const GAP: f64 = 6.0;
    const COLUMN_LEFT: f64 = 60.0;

    fn bubble(head: LineAt, start: LineAt, end: LineAt, area: Viewport) -> Option<Placed> {
        let preferred = if head == start {
            Side::Above
        } else {
            Side::Below
        };
        let anchor = bubble_anchor(BubbleEnds { head, start, end }, COLUMN_LEFT, &area);
        place_popover(&PopoverInput {
            area,
            ..input(anchor, preferred)
        })
    }

    fn select_all(area: Viewport) -> Option<Placed> {
        bubble(LineAt::Below, LineAt::Above, LineAt::Below, area)
    }

    #[test]
    fn a_line_before_the_first_laid_out_row_is_above() {
        assert_eq!(LineAt::locate(10, 40, None), LineAt::Above);
    }

    #[test]
    fn a_laid_out_line_is_shown_where_the_editor_put_it() {
        let at = line(40.0, 300.0);
        assert_eq!(LineAt::locate(50, 40, Some(at)), LineAt::Shown(at));
        assert_eq!(LineAt::locate(0, 0, Some(at)), LineAt::Shown(at));
    }

    #[test]
    fn a_line_after_the_laid_out_rows_is_below() {
        assert_eq!(LineAt::locate(9000, 40, None), LineAt::Below);
    }

    #[test]
    fn select_all_in_a_long_document_shows_the_bubble_at_the_bottom_edge() {
        let placed = select_all(AREA).unwrap();
        assert_eq!(placed.top, AREA.height - EDGE_GAP - HEIGHT);
        assert_eq!(placed.left, COLUMN_LEFT - BUBBLE_INSET);
    }

    #[test]
    fn select_all_keeps_the_bubble_inside_a_pane_barely_wider_than_it() {
        let small = Viewport {
            width: 380.0,
            height: 300.0,
        };
        let placed = select_all(small).unwrap();
        assert!(placed.top >= EDGE_GAP && placed.top + HEIGHT <= small.height - EDGE_GAP);
        assert!(placed.left >= EDGE_GAP && placed.left + WIDTH <= small.width - EDGE_GAP);
    }

    #[test]
    fn a_reversed_selection_past_both_edges_shows_the_bubble_at_the_top_edge() {
        let placed = bubble(LineAt::Above, LineAt::Above, LineAt::Below, AREA).unwrap();
        assert_eq!(placed.top, EDGE_GAP);
    }

    #[test]
    fn a_shown_head_keeps_the_bubble_at_the_caret() {
        let head = line(300.0, 200.0);
        let placed = bubble(
            LineAt::Shown(head),
            LineAt::Above,
            LineAt::Shown(head),
            AREA,
        )
        .unwrap();
        assert_eq!(placed.left, 300.0 - BUBBLE_INSET);
        assert_eq!(placed.top, 220.0 + GAP);
    }

    #[test]
    fn a_selection_running_past_the_bottom_shows_the_bubble_above_its_start() {
        let start = LineAt::Shown(line(40.0, 300.0));
        let placed = bubble(LineAt::Below, start, LineAt::Below, AREA).unwrap();
        assert_eq!(placed.side, Side::Above);
        assert_eq!(placed.top, 300.0 - GAP - HEIGHT);
    }

    #[test]
    fn a_block_starting_above_the_view_shows_the_bubble_below_its_end() {
        let end = LineAt::Shown(line(40.0, 100.0));
        let placed = bubble(LineAt::Above, LineAt::Above, end, AREA).unwrap();
        assert_eq!(placed.side, Side::Below);
        assert_eq!(placed.top, 120.0 + GAP);
        assert_eq!(placed.left, COLUMN_LEFT - BUBBLE_INSET);
    }

    #[test]
    fn a_selection_wholly_past_one_edge_shows_no_bubble() {
        assert_eq!(
            bubble(LineAt::Below, LineAt::Below, LineAt::Below, AREA),
            None
        );
        assert_eq!(
            bubble(LineAt::Above, LineAt::Above, LineAt::Above, AREA),
            None
        );
    }
    #[test]
    fn hides_when_the_anchor_is_out_of_the_area() {
        assert_eq!(place_popover(&input(line(100.0, 620.0), Side::Below)), None);
        assert_eq!(place_popover(&input(line(100.0, -40.0), Side::Below)), None);
    }
}
