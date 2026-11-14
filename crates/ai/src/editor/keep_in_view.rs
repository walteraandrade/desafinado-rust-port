use super::geometry::{Band, Rect};

const EDGE_GAP: f64 = 8.0;

pub struct KeepInBand {
    pub anchor: Rect,
    pub range: Band,
    pub band: Band,
    pub left: f64,
    pub reserve: f64,
}

pub fn keep_in_band(input: &KeepInBand) -> Rect {
    let KeepInBand {
        anchor,
        range,
        band,
        left,
        reserve,
    } = input;
    let below = anchor.bottom + reserve > band.bottom;
    let above = anchor.top < band.top;
    let range_on_screen = range.top < band.bottom && range.bottom > band.top;
    if (!below && !above) || !range_on_screen {
        return *anchor;
    }
    let y = if below {
        (band.top + EDGE_GAP).max(band.bottom - EDGE_GAP - reserve)
    } else {
        band.top + EDGE_GAP
    };
    Rect {
        left: *left,
        right: *left,
        top: y,
        bottom: y,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BAND: Band = Band {
        top: 100.0,
        bottom: 900.0,
    };

    fn at(top: f64) -> Rect {
        Rect {
            left: 400.0,
            right: 401.0,
            top,
            bottom: top + 20.0,
        }
    }

    fn point(y: f64) -> Rect {
        Rect {
            left: 80.0,
            right: 80.0,
            top: y,
            bottom: y,
        }
    }

    fn run(anchor: Rect, top: f64, bottom: f64, reserve: f64) -> Rect {
        keep_in_band(&KeepInBand {
            anchor,
            range: Band { top, bottom },
            band: BAND,
            left: 80.0,
            reserve,
        })
    }

    #[test]
    fn stays_at_its_anchor_while_the_anchor_is_on_screen() {
        assert_eq!(run(at(500.0), 0.0, 600.0, 0.0), at(500.0));
    }

    #[test]
    fn moves_to_the_bottom_edge_when_the_anchor_is_below_the_screen() {
        assert_eq!(run(at(7400.0), 150.0, 7420.0, 0.0), point(892.0));
    }

    #[test]
    fn moves_to_the_top_edge_when_the_anchor_is_above_the_screen() {
        assert_eq!(run(at(-3000.0), -3000.0, 400.0, 0.0), point(108.0));
    }

    #[test]
    fn moves_when_the_anchor_line_is_cut_by_an_edge() {
        assert_eq!(run(at(890.0), 150.0, 910.0, 0.0).top, 892.0);
    }

    #[test]
    fn stays_at_its_anchor_when_none_of_the_range_is_on_screen() {
        assert_eq!(run(at(7400.0), 7000.0, 7420.0, 0.0), at(7400.0));
    }

    #[test]
    fn lifts_a_tall_card_whose_buttons_would_fall_below_the_screen() {
        assert_eq!(run(at(850.0), 150.0, 880.0, 200.0), point(692.0));
    }

    #[test]
    fn keeps_the_anchor_when_the_whole_card_fits() {
        assert_eq!(run(at(500.0), 0.0, 600.0, 200.0), at(500.0));
    }

    #[test]
    fn never_lifts_a_card_above_the_top_edge() {
        assert_eq!(run(at(850.0), 150.0, 880.0, 2000.0).top, 108.0);
    }
}
