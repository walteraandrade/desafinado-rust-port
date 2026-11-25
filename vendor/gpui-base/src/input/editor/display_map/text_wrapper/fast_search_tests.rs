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
            '\u{2060}' | '\u{200b}' => 0.,
            '\t' => 28.,
            c if c.is_ascii_alphanumeric() => 6. + (c as u32 % 5) as f32 * 0.25,
            c if c.is_ascii() => 4.,
            '\u{4e00}'..='\u{9fff}' | '\u{3000}'..='\u{30ff}' => 14.,
            c if c as u32 >= 0x1f000 => 16.,
            _ => 7.5,
        } + (grapheme.chars().count() - 1) as f32 * 0.5;
        if previous != 0 && c != '\u{2060}' && c != '\u{200b}' && (previous + c as u32) % 7 == 0 {
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
    let hint = Cell::default();
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
    let hints = [
        (0., 0),
        (0.01, 10),
        (0.3, 100),
        (5., 3),
        (1e6, 100),
        (f32::MAX, 1),
        (f32::MIN_POSITIVE, usize::MAX),
        (1e-30, 5),
        (f32::NAN, 7),
        (f32::INFINITY, 7),
    ];
    for (pixels_per_byte, row_bytes) in hints {
        let hint = WrapHint {
            pixels_per_byte,
            row_bytes,
        };
        for width in [20., 150., 400., 900.] {
            let expected = reference_wrap(&line, px(width), WrappingIndent::Same, kerned);
            let actual = measured_wrap_hinted(
                &line,
                px(width),
                WrappingIndent::Same,
                &Cell::new(hint),
                kerned,
            );
            assert_eq!(
                boundaries(&actual),
                boundaries(&expected),
                "hint={pixels_per_byte}"
            );
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
    let hint = Cell::default();
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

/// Rows of a few letters and thousands of zero-width joiners: a very small width per byte.
fn zero_heavy_line() -> String {
    format!("a{} ", "\u{2060}".repeat(1500)).repeat(20)
}

/// Shapings and the longest prefix shaped, per row, while wrapping `lines` in order with one hint.
fn probe_costs(lines: &[String], width: f32) -> Vec<(f32, usize)> {
    let hint = Cell::default();
    lines
        .iter()
        .map(|line| {
            let (mut calls, mut longest) = (0, 0);
            let wrapped = measured_wrap_hinted(
                line,
                px(width),
                WrappingIndent::Same,
                &hint,
                |part: &str| {
                    calls += 1;
                    longest = longest.max(part.len());
                    kerned(part)
                },
            );
            (
                calls as f32 / (wrapped.boundaries.len() + 1) as f32,
                longest,
            )
        })
        .collect()
}

#[test]
fn a_bad_hint_costs_a_bounded_amount_not_a_line_length() {
    let zero_width = format!("{}end", "\u{2060}".repeat(6000));
    let zero_heavy = zero_heavy_line();
    let cjk = "你好，世界。这是一个很长的中文句子。".repeat(40);
    let latin = ENGLISH.repeat(430);
    assert!(latin.len() > 60_000);
    let narrow_latin = "il1 ".repeat(300);
    let lines = [
        zero_width.clone(),
        zero_heavy.clone(),
        "x".repeat(60_000),
        latin.clone(),
        cjk.clone(),
        narrow_latin.clone(),
        zero_heavy.clone(),
        zero_width.clone(),
        narrow_latin.clone(),
        latin.clone(),
        cjk,
    ]
    .to_vec();
    for width in [120., 300., 640., 900.] {
        for (line, (per_row, longest)) in lines.iter().zip(probe_costs(&lines, width)) {
            // Text that takes no width has to be verified whole, as before.
            let zero = line.starts_with(['\u{2060}', 'a']);
            assert!(
                zero || longest <= 8 * 1024,
                "a {}-byte line shaped a {longest}-byte prefix at {width}px",
                line.len()
            );
            if !zero && line.len() > 20_000 {
                assert!(per_row <= 5., "{per_row} shapings per row at {width}px");
            }
        }
        let hint = Cell::default();
        for line in &lines {
            let expected = reference_wrap(line, px(width), WrappingIndent::Same, kerned);
            let actual = measured_wrap_hinted(line, px(width), WrappingIndent::Same, &hint, kerned);
            assert_eq!(boundaries(&actual), boundaries(&expected), "w={width}");
        }
    }
}

#[test]
fn a_hint_from_zero_width_text_cannot_make_the_next_line_cost_its_length() {
    let hint = Cell::default();
    let zero_width = zero_heavy_line();
    measured_wrap_hinted(&zero_width, px(120.), WrappingIndent::Same, &hint, kerned);
    let unbroken = "x".repeat(60_000);
    let mut longest = 0;
    let wrapped = measured_wrap_hinted(&unbroken, px(300.), WrappingIndent::Same, &hint, |part| {
        longest = longest.max(part.len());
        kerned(part)
    });
    assert!(!wrapped.boundaries.is_empty());
    assert!(longest < 16 * 1024, "shaped a {longest}-byte prefix");
}

#[cfg(target_os = "linux")]
#[test]
fn estimated_search_matches_the_galloping_one_on_real_shaping() {
    use super::tests::{shaping_test_context, test_font};
    let cx = shaping_test_context();
    cx.update(|cx| {
        let font = test_font();
        let font_size = px(14.);
        let text_system = gpui::WindowTextSystem::new(cx.text_system().clone());
        let tab_stop = tab_stop_width(&text_system, &font, font_size, TabSize::default().tab_size);
        let measure = |text: &str| {
            let layout = text_system.layout_line(
                text,
                font_size,
                &[gpui::TextRun {
                    len: text.len(),
                    font: font.clone(),
                    color: gpui::black(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }],
                None,
            );
            expanded_width(&layout, text, tab_stop)
        };
        // No `fi`, `ff` or `fl`: a font ligature there can make width non-monotone in the prefix,
        // where the two searches may legitimately pick different fitting breaks.
        let prose = format!("\t{ENGLISH}\tand\tthen\t{ENGLISH}");
        let lines = [
            prose.repeat(3),
            format!("    {prose}"),
            PORTUGUESE.repeat(4),
        ];
        let hint = Cell::default();
        for line in &lines {
            assert!(!line.contains("fi") && !line.contains("ff") && !line.contains("fl"));
            for indent in [WrappingIndent::None, WrappingIndent::Same] {
                for width in [60., 140., 333., 520., 800., 1400.] {
                    let expected = reference_wrap(line, px(width), indent, measure);
                    let actual = measured_wrap_hinted(line, px(width), indent, &hint, measure);
                    assert_eq!(boundaries(&actual), boundaries(&expected), "w={width}");
                }
            }
        }
    });
}
