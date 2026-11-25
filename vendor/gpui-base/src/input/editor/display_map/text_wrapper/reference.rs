//! The galloping search `measured_wrap` used before the estimate-then-verify one, kept as the
//! differential oracle for it (same boundaries for any measure that grows with the prefix).

use super::WrappingIndent;
use super::wrap_width::{MeasuredWrap, WidthRange};
use gpui::{Pixels, px};
use unicode_segmentation::UnicodeSegmentation as _;

pub(super) fn reference_wrap(
    text: &str,
    width: Pixels,
    wrapping_indent: WrappingIndent,
    mut measure: impl FnMut(&str) -> Pixels,
) -> MeasuredWrap {
    let mut valid_widths = WidthRange::all();
    let indent = if wrapping_indent == WrappingIndent::Same {
        text.chars()
            .take_while(|&c| c == ' ')
            .count()
            .min(gpui::LineWrapper::MAX_INDENT as usize)
    } else {
        0
    };
    let indent_width = measure(&text[..indent]);
    let ends: Vec<usize> = text
        .grapheme_indices(true)
        .map(|(ix, grapheme)| ix + grapheme.len())
        .collect();
    let opportunities: Vec<usize> = unicode_linebreak::linebreaks(text)
        .map(|(ix, _)| ix)
        .filter(|ix| ends.binary_search(ix).is_ok())
        .collect();
    let mut result = Vec::new();
    let mut first = 0;
    let mut start = 0;
    while first < ends.len() {
        let available = if start == 0 {
            width
        } else {
            width - indent_width
        };
        // Probe legal word breaks first. Character-fit changes inside the same word do not
        // change the result, so they must not invalidate a cached line during a divider drag.
        // Bound word probes before shaping: a single word can span the entire remaining line.
        // Beyond this local window, use the original exponential grapheme search instead.
        const MAX_WORD_PROBE_GRAPHEMES: usize = 256;
        let word_probe_end = ends[first..]
            .get(MAX_WORD_PROBE_GRAPHEMES - 1)
            .copied()
            .unwrap_or(text.len());
        let mut fitting = |candidates: &[usize], limit: Option<usize>| {
            let mut fits = |end| {
                if limit.is_some_and(|limit| end > limit) {
                    return None;
                }
                Some(valid_widths.fits(
                    measure(&text[start..end]),
                    available,
                    if start == 0 { px(0.) } else { indent_width },
                ))
            };
            let mut low = 0;
            let mut high = 1;
            while high <= candidates.len() && fits(candidates[high - 1])? {
                low = high;
                if high == candidates.len() {
                    break;
                }
                high = (high * 2).min(candidates.len());
            }
            while low + 1 < high {
                let mid = (low + high) / 2;
                if fits(candidates[mid - 1])? {
                    low = mid;
                } else {
                    high = mid;
                }
            }
            Some(low)
        };
        let first_word =
            opportunities.partition_point(|&ix| ix <= start || (start == 0 && ix <= indent));
        let words = &opportunities[first_word..];
        let end = if let Some(word_count) = fitting(words, Some(word_probe_end))
            && word_count > 0
        {
            words[word_count - 1]
        } else {
            // An oversized word retains the original complete-grapheme fallback. At least one
            // grapheme is consumed even when it is wider than the available viewport.
            let count = fitting(&ends[first..], None).unwrap();
            let fitting_end = ends[first + count.max(1) - 1];
            if fitting_end == text.len() {
                break;
            }
            let word_count = words.partition_point(|&ix| ix <= fitting_end);
            word_count
                .checked_sub(1)
                .map(|ix| words[ix])
                .unwrap_or(fitting_end)
        };
        if end == text.len() {
            break;
        }
        result.push(gpui::Boundary {
            ix: end,
            next_indent: indent as u32,
        });
        first = ends.partition_point(|&ix| ix <= end);
        start = end;
    }
    MeasuredWrap {
        boundaries: result,
        valid_widths,
    }
}
