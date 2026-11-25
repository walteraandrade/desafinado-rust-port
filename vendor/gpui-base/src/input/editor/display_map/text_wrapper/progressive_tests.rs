//! A large document opens with most rows estimated; refining them must give the rows of a full wrap.

use super::tests::test_font;
use super::*;
use std::time::{Duration, Instant};

const WIDTH: f32 = 100.;
const ROW: usize = 10;

/// Breaks every `ROW` bytes at a character boundary, whatever the width.
fn fake_wrap(line: &str, _: Pixels, _: usize, _: Option<&Prior>) -> MeasuredWrap {
    let mut boundaries = Vec::new();
    let mut row_start = 0;
    for (ix, ch) in line.char_indices() {
        if ix + ch.len_utf8() - row_start > ROW {
            boundaries.push(gpui::Boundary { ix, next_indent: 0 });
            row_start = ix;
        }
    }
    MeasuredWrap {
        boundaries,
        valid_widths: WidthRange::all(),
    }
}

fn document(lines: usize) -> Rope {
    let text: Vec<String> = (0..lines)
        .map(|n| match n % 5 {
            0 => String::new(),
            1 => format!("line {n}"),
            2 => format!("{n} 世界 {}", "ab".repeat(n % 17)),
            3 => "word ".repeat(n % 13),
            _ => format!("{}é{}", "x".repeat(n % 9), "y".repeat(n % 31)),
        })
        .collect();
    Rope::from(text.join("\n"))
}

fn empty() -> TextWrapper {
    TextWrapper::new(test_font(), px(14.), Some(px(WIDTH)))
}

fn exact(text: &Rope) -> TextWrapper {
    let mut wrapper = empty();
    wrapper.update_lines(text, &(0..0), text, false, None, &mut fake_wrap);
    wrapper
}

fn progressive(text: &Rope, focus: &[Range<usize>]) -> TextWrapper {
    let mut wrapper = empty();
    wrapper.update_lines(text, &(0..0), text, false, Some(focus), &mut fake_wrap);
    wrapper
}

fn rows(wrapper: &TextWrapper) -> Vec<(Vec<Range<usize>>, u32, usize)> {
    wrapper
        .iter_lines()
        .map(|line| (line.wrapped_lines.to_vec(), line.indent, line.zone_rows))
        .collect()
}

fn refine_all(wrapper: &mut TextWrapper) {
    let count = wrapper.lines_count();
    while wrapper
        .refine_lines_with(0..count, None, &mut fake_wrap)
        .is_some()
    {}
}

#[test]
fn only_the_focus_is_wrapped_exactly() {
    let text = document(200);
    let full = exact(&text);
    let partial = progressive(&text, &[40..60, 150..155]);
    assert_eq!(partial.lines_count(), full.lines_count());
    for row in 0..full.lines_count() {
        let line = partial.line(row).unwrap();
        let focused = (40..60).contains(&row) || (150..155).contains(&row);
        assert_eq!(line.is_estimated(), !focused, "line {row}");
        assert_eq!(line.len(), full.line(row).unwrap().len());
        if focused {
            assert_eq!(line.wrapped_lines, full.line(row).unwrap().wrapped_lines);
        }
    }
    assert!(partial.has_estimated());
}

#[test]
fn refining_every_estimated_line_equals_a_full_wrap() {
    let text = document(500);
    let full = exact(&text);
    let mut partial = progressive(&text, &[100..130]);
    refine_all(&mut partial);
    assert!(!partial.has_estimated());
    assert_eq!(rows(&partial), rows(&full));
    assert_eq!(partial.len(), full.len());
    assert_eq!(partial.longest_row(), full.longest_row());
    for row in [0, 1, 77, 499] {
        assert_eq!(
            partial.buffer_line_to_wrap_row_range(row),
            full.buffer_line_to_wrap_row_range(row)
        );
    }
}

#[test]
fn estimated_rows_tile_the_line_on_character_boundaries() {
    let text = document(300);
    let partial = progressive(&text, &[0..20]);
    for (row, line) in partial.iter_lines().enumerate() {
        let source = text.slice_line(row).to_string();
        let mut end = 0;
        for range in &line.wrapped_lines {
            assert_eq!(range.start, end, "line {row} has a gap");
            assert!(source.is_char_boundary(range.start) && source.is_char_boundary(range.end));
            end = range.end;
        }
        assert_eq!(end, source.len());
    }
}

#[test]
fn estimates_are_near_the_exact_rows_when_lines_look_alike() {
    let text = Rope::from(
        (0..400)
            .map(|n| "abcdefghij".repeat(1 + n % 7))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let full = exact(&text);
    let partial = progressive(&text, &[0..40]);
    let (estimated, truth) = (partial.len() as f32, full.len() as f32);
    assert!(
        (estimated - truth).abs() / truth < 0.15,
        "estimated {estimated} rows for {truth}"
    );
}

#[test]
fn refining_stops_at_the_deadline_after_one_line_and_resumes() {
    let text = document(100);
    let full = exact(&text);
    let mut partial = progressive(&text, &[0..1]);
    let past = Instant::now() - Duration::from_secs(1);
    let mut steps = 0;
    while let Some(done) = partial.refine_lines_with(0..100, Some(past), &mut fake_wrap) {
        assert_eq!(done.len(), 1);
        steps += 1;
    }
    assert!((90..=99).contains(&steps), "{steps} steps");
    assert_eq!(rows(&partial), rows(&full));
}

#[test]
fn refining_a_range_leaves_the_rest_estimated() {
    let text = document(100);
    let mut partial = progressive(&text, &[0..1]);
    let done = partial
        .refine_lines_with(30..50, None, &mut fake_wrap)
        .unwrap();
    assert_eq!(done, 30..50);
    for row in 0..100 {
        let wrapped = row == 0 || (30..50).contains(&row);
        assert_eq!(partial.line(row).unwrap().is_estimated(), !wrapped, "{row}");
    }
}

#[test]
fn refining_without_estimated_lines_does_nothing() {
    let text = document(50);
    let mut full = exact(&text);
    assert!(
        full.refine_lines_with(0..50, None, &mut fake_wrap)
            .is_none()
    );
}

#[test]
fn editing_an_estimated_line_wraps_it_exactly_and_the_rest_still_converges() {
    let text = document(120);
    let mut partial = progressive(&text, &[0..1]);
    let mut edited = text.clone();
    let at = edited.line_start_offset(62) + 2;
    let inserted = "inserted words here";
    edited.replace(at..at, inserted);
    partial.update_lines(
        &edited,
        &(at..at),
        &Rope::from(inserted),
        false,
        None,
        &mut fake_wrap,
    );
    let full = exact(&edited);
    assert!(!partial.line(62).unwrap().is_estimated());
    assert_eq!(
        partial.line(62).unwrap().wrapped_lines,
        full.line(62).unwrap().wrapped_lines
    );
    assert!(partial.line(63).unwrap().is_estimated());
    refine_all(&mut partial);
    assert_eq!(rows(&partial), rows(&full));
}

#[test]
fn an_edit_that_adds_lines_keeps_estimated_neighbours_and_converges() {
    let text = document(120);
    let mut partial = progressive(&text, &[0..1]);
    let mut edited = text.clone();
    let at = edited.line_start_offset(30);
    let inserted = "first\nsecond line that is long\n";
    edited.replace(at..at, inserted);
    partial.update_lines(
        &edited,
        &(at..at),
        &Rope::from(inserted),
        false,
        None,
        &mut fake_wrap,
    );
    assert_eq!(partial.lines_count(), 122);
    refine_all(&mut partial);
    assert_eq!(rows(&partial), rows(&exact(&edited)));
}

#[test]
fn a_full_pass_with_reusable_rows_estimates_only_the_rest() {
    let text = document(150);
    let mut wrapper = exact(&text);
    wrapper.update_lines(
        &text,
        &(0..text.len()),
        &text,
        true,
        Some(&[10..12]),
        &mut fake_wrap,
    );
    assert!(!wrapper.has_estimated());
    refine_all(&mut wrapper);
    assert_eq!(rows(&wrapper), rows(&exact(&text)));
}

#[test]
fn a_zone_adds_rows_to_an_estimated_line_too() {
    let text = document(60);
    let mut wrapper = empty();
    let offset = text.line_start_offset(41) + 1;
    wrapper.zones = Rc::from([(offset, 3)]);
    wrapper.update_lines(&text, &(0..0), &text, false, Some(&[0..5]), &mut fake_wrap);
    assert!(wrapper.line(41).unwrap().is_estimated());
    assert_eq!(wrapper.line(41).unwrap().zone_rows, 3);
    assert_eq!(
        wrapper.line(41).unwrap().lines_len(),
        wrapper.line(41).unwrap().text_rows() + 3
    );
}

#[cfg(target_os = "linux")]
#[test]
fn production_refining_equals_a_full_production_wrap() {
    let cx = super::tests::shaping_test_context();
    cx.update(|cx| {
        let text = Rope::from(
            (0..120)
                .map(|n| match n % 4 {
                    0 => "The quick brown fox jumps over the lazy dog. ".repeat(1 + n % 6),
                    1 => String::new(),
                    2 => format!("    indented 世界 line {n} with some words to wrap around"),
                    _ => "short".to_owned(),
                })
                .collect::<Vec<_>>()
                .join("\n"),
        );
        let mut full = TextWrapper::new(test_font(), px(14.), Some(px(200.)));
        full.progressive_min_bytes = usize::MAX;
        full.update_all(&text, cx);
        let mut partial = TextWrapper::new(test_font(), px(14.), Some(px(200.)));
        partial.progressive_min_bytes = 0;
        partial.set_focus(vec![40..44]);
        partial.update_all(&text, cx);
        assert!(partial.has_estimated());
        assert!(partial.line(40).is_some_and(|line| !line.is_estimated()));
        assert_eq!(
            partial.line(40).unwrap().wrapped_lines,
            full.line(40).unwrap().wrapped_lines
        );
        let deadline = Instant::now() + Duration::from_millis(1);
        while partial.refine_lines(0..120, Some(deadline), cx).is_some() {}
        assert!(!partial.has_estimated());
        assert_eq!(rows(&partial), rows(&full));
        assert_eq!(partial.len(), full.len());
    });
}

#[cfg(target_os = "linux")]
#[test]
fn production_replacing_the_whole_text_is_progressive_and_converges() {
    let cx = super::tests::shaping_test_context();
    cx.update(|cx| {
        let lines = |count: usize| {
            Rope::from(
                (0..count)
                    .map(|n| format!("{n} {}", "lorem ipsum dolor sit amet ".repeat(1 + n % 7)))
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        };
        let (old, new) = (lines(30), lines(300));
        let mut wrapper = TextWrapper::new(test_font(), px(14.), Some(px(200.)));
        wrapper.progressive_min_bytes = 1000;
        wrapper.update_all(&old, cx);
        assert!(!wrapper.has_estimated());
        wrapper.update(&new, &(0..old.len()), &new, cx);
        assert!(wrapper.has_estimated());
        while wrapper.refine_lines(0..300, None, cx).is_some() {}
        let mut full = TextWrapper::new(test_font(), px(14.), Some(px(200.)));
        full.progressive_min_bytes = usize::MAX;
        full.update_all(&new, cx);
        assert_eq!(rows(&wrapper), rows(&full));
    });
}
