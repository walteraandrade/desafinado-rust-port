//! The estimate-then-verify search must pick the same breaks as the galloping one, with far fewer shapings.

use super::reference::reference_wrap;
use super::*;
use std::cell::Cell;

/// Prefix widths that are not a plain sum: pair kerning, wide CJK and emoji, tabs.
fn kerned(text: &str) -> Pixels {
    let mut total = 0.;
    let mut previous = 0u32;
    for grapheme in text.graphemes(true) {
        let c = grapheme.chars().next().unwrap_or(' ');
        total += match c {
            ' ' => 3.5,
            '\t' => 28.,
            c if c.is_ascii_alphanumeric() => 6. + (c as u32 % 5) as f32 * 0.25,
            c if c.is_ascii() => 4.,
            '\u{4e00}'..='\u{9fff}' | '\u{3000}'..='\u{30ff}' => 14.,
            c if c as u32 >= 0x1f000 => 16.,
            _ => 7.5,
        } + (grapheme.chars().count() - 1) as f32 * 0.5;
        if previous != 0 && (previous + c as u32) % 7 == 0 {
            total -= 0.75;
        }
        previous = c as u32;
    }
    px(total)
}

const ENGLISH: &str = "The writer opens a draft and reads each paragraph slowly while the editor keeps every change as a tracked suggestion that waits for review. ";
const PORTUGUESE: &str = "A coleção de músicas baianas não cabe numa só tarde; ação, emoção e informação convivem com a canção. ";

fn corpus() -> Vec<String> {
    vec![
        String::new(),
        "a".into(),
        ENGLISH.repeat(3),
        PORTUGUESE.repeat(3),
        format!("  {}", ENGLISH.repeat(2)),
        format!("      {PORTUGUESE}"),
        "x".repeat(900),
        format!(
            "see https://example.com/{}/end and go",
            "path/segment".repeat(40)
        ),
        "你好，世界。这是一个很长的中文句子，用来测试换行。".repeat(8),
        "emoji 👩‍💻👨‍👩‍👧‍👦 and e\u{301}\u{301} marks 😀😀😀 ".repeat(12),
        "\tindented\twith\ttabs\tand fi ffi office words ".repeat(10),
        "mix 中文 latin 日本語 text".repeat(15),
        ENGLISH.repeat(450),
    ]
}

fn widths() -> Vec<f32> {
    let mut widths: Vec<f32> = (0..40).map(|i| 1. + i as f32 * 1.7).collect();
    widths.extend([
        100., 137.3, 240., 333.3, 480., 640., 777., 1200., 5000., 100_000.,
    ]);
    widths
}

fn boundaries(wrap: &MeasuredWrap) -> Vec<usize> {
    wrap.boundaries.iter().map(|b| b.ix).collect()
}

fn indents(wrap: &MeasuredWrap) -> Vec<u32> {
    wrap.boundaries.iter().map(|b| b.next_indent).collect()
}

#[test]
fn estimated_search_picks_the_same_breaks_as_the_galloping_one() {
    let hint = Cell::new(0.);
    for line in corpus() {
        for indent in [WrappingIndent::None, WrappingIndent::Same] {
            let stride = if line.len() > 20_000 { 7 } else { 1 };
            for width in widths().into_iter().step_by(stride) {
                let width = px(width);
                let expected = reference_wrap(&line, width, indent, kerned);
                let oracle = measured_wrap_boundaries(&line, width, indent, kerned);
                let actual = measured_wrap_hinted(&line, width, indent, &hint, kerned);
                let fresh = measured_wrap(&line, width, indent, kerned);
                assert_eq!(boundaries(&actual), boundaries(&expected), "w={width:?}");
                assert_eq!(boundaries(&fresh), boundaries(&expected), "w={width:?}");
                assert_eq!(
                    boundaries(&expected),
                    oracle.iter().map(|b| b.ix).collect::<Vec<_>>()
                );
                assert_eq!(indents(&actual), indents(&expected));
            }
        }
    }
}

#[test]
fn a_wrong_hint_changes_the_probes_but_not_the_breaks() {
    let line = ENGLISH.repeat(20);
    for hint in [0., 0.01, 0.3, 5., 1e6, f32::MAX] {
        for width in [20., 150., 400., 900.] {
            let expected = reference_wrap(&line, px(width), WrappingIndent::Same, kerned);
            let actual = measured_wrap_hinted(
                &line,
                px(width),
                WrappingIndent::Same,
                &Cell::new(hint),
                kerned,
            );
            assert_eq!(boundaries(&actual), boundaries(&expected), "hint={hint}");
        }
    }
}

#[test]
fn the_valid_width_range_still_holds_the_same_breaks() {
    for line in corpus().into_iter().filter(|l| l.len() < 3000) {
        for indent in [WrappingIndent::None, WrappingIndent::Same] {
            for width in [60., 133.7, 311., 600.] {
                let wrap = measured_wrap(&line, px(width), indent, kerned);
                for step in -40..=40 {
                    let other = px(width + step as f32 * 0.37);
                    if wrap.valid_widths.contains(other) {
                        let expected = reference_wrap(&line, other, indent, kerned);
                        assert_eq!(
                            boundaries(&wrap),
                            boundaries(&expected),
                            "{width} -> {other:?}"
                        );
                    }
                }
            }
        }
    }
}

fn shapings_per_row(text: &str, width: f32, wrap: bool) -> f32 {
    let (mut calls, mut rows) = (0, 0);
    let hint = Cell::new(0.);
    for line in text.split('\n') {
        let mut measure = |part: &str| {
            calls += 1;
            kerned(part)
        };
        let wrapped = if wrap {
            measured_wrap_hinted(line, px(width), WrappingIndent::Same, &hint, &mut measure)
        } else {
            reference_wrap(line, px(width), WrappingIndent::Same, &mut measure)
        };
        rows += wrapped.boundaries.len() + 1;
    }
    calls as f32 / rows as f32
}

#[test]
fn prose_needs_few_shapings_per_row() {
    let paragraph = ENGLISH.repeat(400);
    let lines = [ENGLISH, PORTUGUESE]
        .map(|s| s.repeat(4))
        .join("\n")
        .repeat(30);
    for width in [300., 640., 900.] {
        for text in [&paragraph, &lines] {
            let now = shapings_per_row(text, width, true);
            let before = shapings_per_row(text, width, false);
            assert!(now <= 4., "{now} shapings per row at {width}px");
            assert!(now * 2. < before, "{now} vs {before} at {width}px");
        }
    }
}
