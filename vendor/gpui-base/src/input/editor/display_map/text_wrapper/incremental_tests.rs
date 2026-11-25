//! Rewrapping only from an edit must give the breaks of wrapping the whole edited line.

use super::fast_search_tests::{corpus, kerned};
use super::*;
use std::cell::Cell;

struct Lcg(u64);

impl Lcg {
    fn next(&mut self, below: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % below.max(1)
    }
}

fn char_floor(text: &str, at: usize) -> usize {
    (0..=at.min(text.len()))
        .rev()
        .find(|&ix| text.is_char_boundary(ix))
        .unwrap_or(0)
}

fn breaks(wrap: &MeasuredWrap) -> Vec<(usize, u32)> {
    wrap.boundaries
        .iter()
        .map(|b| (b.ix, b.next_indent))
        .collect()
}

fn wrap_whole(text: &str, width: f32, measure: impl FnMut(&str) -> Pixels) -> MeasuredWrap {
    measured_wrap_hinted(
        text,
        px(width),
        WrappingIndent::Same,
        &Cell::default(),
        measure,
    )
}

fn wrap_edited(
    old: &str,
    new: &str,
    edit: (usize, usize),
    width: f32,
    measure: impl FnMut(&str) -> Pixels,
) -> MeasuredWrap {
    let before = wrap_whole(old, width, kerned);
    let old_breaks: Vec<usize> = before.boundaries.iter().map(|b| b.ix).collect();
    let (start, old_end) = edit;
    let prior = Prior {
        old_breaks: &old_breaks,
        edit_start: start,
        old_edit_end: old_end,
        new_edit_end: new.len() + old_end - old.len(),
        valid_widths: before.valid_widths,
    };
    rewrap_edited(
        new,
        px(width),
        WrappingIndent::Same,
        &Cell::default(),
        &prior,
        measure,
    )
}

fn random_edit(rng: &mut Lcg, text: &str) -> (String, (usize, usize)) {
    let start = char_floor(text, rng.next(text.len() + 1));
    let removed = match rng.next(3) {
        0 => 0,
        _ => char_floor(text, (start + rng.next(40)).min(text.len())).max(start) - start,
    };
    let inserted = ["x", " ", "word ", "é", "你好", "  spaced  out ", "", "\t"][rng.next(8)];
    let mut new = String::from(&text[..start]);
    new.push_str(inserted);
    new.push_str(&text[start + removed..]);
    (new, (start, start + removed))
}

#[test]
fn rewrapping_from_the_edit_gives_the_breaks_of_wrapping_the_whole_line() {
    let mut rng = Lcg(7);
    for line in corpus() {
        for width in [60., 137.3, 333.3, 480., 777., 1200.] {
            let mut text = line.clone();
            for _ in 0..25 {
                let (new, edit) = random_edit(&mut rng, &text);
                let expected = wrap_whole(&new, width, kerned);
                let actual = wrap_edited(&text, &new, edit, width, kerned);
                assert_eq!(
                    breaks(&actual),
                    breaks(&expected),
                    "edit {edit:?} in {} bytes at width {width}",
                    text.len()
                );
                assert!(
                    !actual.valid_widths.contains(px(width))
                        || expected.valid_widths.contains(px(width))
                );
                text = new;
            }
        }
    }
}

#[test]
fn typing_in_a_long_line_shapes_a_few_rows_not_the_whole_line() {
    let text = corpus().pop().unwrap_or_default();
    let middle = char_floor(&text, text.len() / 2);
    let mut new = text.clone();
    new.insert(middle, 'q');
    let shaped = Cell::new(0usize);
    let counting = |t: &str| {
        shaped.set(shaped.get() + 1);
        kerned(t)
    };
    wrap_whole(&new, 480., counting);
    let whole = shaped.replace(0);
    let before = wrap_whole(&text, 480., kerned);
    let old_breaks: Vec<usize> = before.boundaries.iter().map(|b| b.ix).collect();
    let prior = Prior {
        old_breaks: &old_breaks,
        edit_start: middle,
        old_edit_end: middle,
        new_edit_end: middle + 1,
        valid_widths: before.valid_widths,
    };
    rewrap_edited(
        &new,
        px(480.),
        WrappingIndent::Same,
        &Cell::default(),
        &prior,
        |t: &str| {
            shaped.set(shaped.get() + 1);
            kerned(t)
        },
    );
    let edited = shaped.get();
    assert!(edited * 20 < whole, "edited {edited}, whole {whole}");
}

#[cfg(target_os = "linux")]
#[test]
fn a_wrapper_edited_in_place_matches_a_fresh_one_on_real_shaping() {
    use super::tests::{shaping_test_context, test_font};
    let cx = shaping_test_context();
    cx.update(|cx| {
        let font = test_font();
        let size = px(14.);
        let mut text = format!("# Title\n\n{}\n\nlast line", corpus()[2].repeat(60));
        let paragraph = text.find("\n\n").map_or(0, |at| at + 2);
        let mut wrapper = TextWrapper::new(font.clone(), size, Some(px(520.)));
        let all = Rope::from(text.as_str());
        wrapper.update(&all, &(0..0), &all, cx);
        let mut rng = Lcg(11);
        for _ in 0..40 {
            let start = char_floor(&text, paragraph + rng.next(text.len() / 2));
            let end = char_floor(&text, (start + rng.next(30)).min(text.len()));
            let inserted = ["a", " ", "quick ", "é", ""][rng.next(5)];
            let mut next = String::from(&text[..start]);
            next.push_str(inserted);
            next.push_str(&text[end..]);
            let next_rope = Rope::from(next.as_str());
            wrapper.update(&next_rope, &(start..end), &Rope::from(inserted), cx);
            let mut fresh = TextWrapper::new(font.clone(), size, Some(px(520.)));
            fresh.update(&next_rope, &(0..0), &next_rope, cx);
            let rows = |wrapper: &TextWrapper| -> Vec<Vec<Range<usize>>> {
                wrapper
                    .iter_lines()
                    .map(|line| line.wrapped_lines.to_vec())
                    .collect()
            };
            assert_eq!(rows(&wrapper), rows(&fresh), "after editing {start}..{end}");
            text = next;
        }
    });
}
