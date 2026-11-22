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

/// The anchor for a popover that belongs to a range of lines, so that
/// flipping to the other side never puts it over the range itself.
pub fn span_anchor(first: Rect, last: Rect, left: f64) -> Rect {
    Rect {
        left,
        right: left,
        top: first.top.min(last.top),
        bottom: first.bottom.max(last.bottom),
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

/// The rect to anchor on for a line, with a line past an edge of the area
/// pinned to that edge, so a span that runs off screen still anchors inside.
pub fn pinned_line(line: LineAt, left: f64, area: &Viewport) -> Rect {
    let edge = |y: f64| Rect {
        left,
        right: left,
        top: y,
        bottom: y,
    };
    match line {
        LineAt::Above => edge(0.0),
        LineAt::Shown(rect) => rect,
        LineAt::Below => edge(area.height),
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

    #[test]
    fn a_span_anchor_runs_from_the_first_line_to_the_last_at_the_given_left() {
        let first = line(40.0, 100.0);
        let last = line(10.0, 160.0);
        assert_eq!(
            span_anchor(first, last, 25.0),
            Rect {
                left: 25.0,
                right: 25.0,
                top: 100.0,
                bottom: 180.0,
            }
        );
    }

    #[test]
    fn a_span_anchor_keeps_a_last_line_reported_above_the_first() {
        let first = line(40.0, 100.0);
        let last = line(10.0, 60.0);
        let anchor = span_anchor(first, last, 40.0);
        assert_eq!((anchor.top, anchor.bottom), (60.0, 120.0));
    }

    #[test]
    fn a_popover_flipped_off_a_span_never_covers_the_span() {
        let span = span_anchor(line(40.0, 30.0), line(40.0, 130.0), 40.0);
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
            width: 360.0,
            height: 200.0,
            area: AREA,
            side,
            gap: 6.0,
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

    fn span_of(first: LineAt, last: LineAt, side: Side) -> Option<Placed> {
        let span = span_anchor(
            pinned_line(first, 40.0, &AREA),
            pinned_line(last, 40.0, &AREA),
            40.0,
        );
        place_popover(&input(span, side))
    }

    fn inside(placed: Placed) -> bool {
        placed.top >= EDGE_GAP && placed.top + 200.0 <= AREA.height - EDGE_GAP
    }

    #[test]
    fn a_selection_past_both_edges_shows_at_the_edge_of_its_head() {
        let forward = span_of(LineAt::Above, LineAt::Below, Side::Below).unwrap();
        assert!(inside(forward));
        assert_eq!(forward.top, AREA.height - EDGE_GAP - 200.0);
        let reversed = span_of(LineAt::Above, LineAt::Below, Side::Above).unwrap();
        assert!(inside(reversed));
        assert_eq!(reversed.top, EDGE_GAP);
    }

    #[test]
    fn a_selection_running_past_the_bottom_shows_above_its_start() {
        let placed = span_of(LineAt::Shown(line(40.0, 300.0)), LineAt::Below, Side::Below).unwrap();
        assert_eq!(placed.side, Side::Above);
        assert_eq!(placed.top, 300.0 - 6.0 - 200.0);
    }

    #[test]
    fn a_selection_running_past_the_top_shows_below_its_end() {
        let placed = span_of(LineAt::Above, LineAt::Shown(line(40.0, 200.0)), Side::Above).unwrap();
        assert_eq!(placed.side, Side::Below);
        assert_eq!(placed.top, 220.0 + 6.0);
    }

    #[test]
    fn a_selection_wholly_past_one_edge_hides() {
        assert_eq!(span_of(LineAt::Below, LineAt::Below, Side::Below), None);
        assert_eq!(span_of(LineAt::Above, LineAt::Above, Side::Above), None);
    }

    #[test]
    fn hides_when_the_anchor_is_out_of_the_area() {
        assert_eq!(place_popover(&input(line(100.0, 620.0), Side::Below)), None);
        assert_eq!(place_popover(&input(line(100.0, -40.0), Side::Below)), None);
    }
}
