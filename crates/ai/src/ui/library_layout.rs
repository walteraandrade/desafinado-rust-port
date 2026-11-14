pub const RAIL_WIDTH_PX: f64 = 40.0;
pub const LIBRARY_WIDTH_PX: f64 = 288.0;
pub const TEXT_COLUMN_PX: f64 = 768.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryPlacement {
    Column,
    Overlay,
}

pub fn library_placement(viewport_width: f64) -> LibraryPlacement {
    if viewport_width >= RAIL_WIDTH_PX + LIBRARY_WIDTH_PX + TEXT_COLUMN_PX {
        LibraryPlacement::Column
    } else {
        LibraryPlacement::Overlay
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FITS: f64 = RAIL_WIDTH_PX + LIBRARY_WIDTH_PX + TEXT_COLUMN_PX;

    #[test]
    fn measures_the_text_column_as_its_48rem_border_box_gutters_included() {
        assert_eq!(TEXT_COLUMN_PX, 768.0);
        assert_eq!(FITS, 1096.0);
    }

    #[test]
    fn takes_its_own_column_when_the_rail_the_library_and_the_text_fit() {
        assert_eq!(library_placement(FITS), LibraryPlacement::Column);
        assert_eq!(library_placement(1440.0), LibraryPlacement::Column);
    }

    #[test]
    fn lies_over_the_page_one_pixel_short_of_that() {
        assert_eq!(library_placement(FITS - 1.0), LibraryPlacement::Overlay);
        assert_eq!(library_placement(900.0), LibraryPlacement::Overlay);
    }
}
