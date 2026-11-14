use super::geometry::Viewport;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TooltipSpace {
    pub top: f64,
    pub left: f64,
    pub right: f64,
    pub bottom: f64,
}

pub fn space_below(inset: f64, viewport: Viewport) -> TooltipSpace {
    TooltipSpace {
        top: inset.max(0.0),
        left: 0.0,
        right: viewport.width,
        bottom: viewport.height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_the_space_under_the_inset_and_keeps_the_rest_of_the_viewport() {
        assert_eq!(
            space_below(
                72.0,
                Viewport {
                    width: 1200.0,
                    height: 900.0
                }
            ),
            TooltipSpace {
                top: 72.0,
                left: 0.0,
                right: 1200.0,
                bottom: 900.0
            }
        );
    }

    #[test]
    fn never_starts_above_the_viewport() {
        let viewport = Viewport {
            width: 800.0,
            height: 600.0,
        };
        assert_eq!(space_below(-30.0, viewport).top, 0.0);
    }
}
