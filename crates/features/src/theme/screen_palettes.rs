use super::color::{oklch_hex, readable_on};
use super::tokens::{ThemeMode, ThemeRoles};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hue {
    pub hue: f64,
    pub chroma: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThemeSeed {
    pub id: &'static str,
    pub title: &'static str,
    pub source: Option<(u32, u32)>,
    pub mode: ThemeMode,
    pub base: Hue,
    pub heading: Hue,
    pub accent: f64,
    pub ai: f64,
    pub danger: f64,
    pub success: f64,
}

pub const HEADING_CONTRAST: f64 = 7.0;
pub const INK_CONTRAST: f64 = 8.5;
pub const MUTED_CONTRAST: f64 = 5.0;
pub const SIGNAL_CONTRAST: f64 = 5.0;
pub const FAINT_CONTRAST: f64 = 3.2;

struct Ladder {
    canvas: f64,
    raised: f64,
    overlay: f64,
    fill: f64,
    fill_hover: f64,
    hairline: f64,
    scrollbar: f64,
    selection: f64,
    signal_fill: f64,
    text_from: f64,
}

const DARK: Ladder = Ladder {
    canvas: 0.20,
    raised: 0.235,
    overlay: 0.275,
    fill: 0.30,
    fill_hover: 0.345,
    hairline: 0.31,
    scrollbar: 0.42,
    selection: 0.36,
    signal_fill: 0.29,
    text_from: 0.55,
};

const LIGHT: Ladder = Ladder {
    canvas: 0.975,
    raised: 0.945,
    overlay: 0.995,
    fill: 0.915,
    fill_hover: 0.88,
    hairline: 0.88,
    scrollbar: 0.76,
    selection: 0.87,
    signal_fill: 0.935,
    text_from: 0.6,
};

pub fn roles_of_seed(seed: &ThemeSeed) -> ThemeRoles {
    let dark = seed.mode == ThemeMode::Dark;
    let ladder = if dark { &DARK } else { &LIGHT };
    let base =
        |lightness: f64, share: f64| oklch_hex(lightness, seed.base.chroma * share, seed.base.hue);
    let canvas = base(ladder.canvas, 1.0);
    let raised = base(ladder.raised, 1.1);
    let overlay = base(ladder.overlay, if dark { 1.15 } else { 0.4 });
    let fill = base(ladder.fill, 1.15);
    let fill_hover = base(ladder.fill_hover, 1.2);
    let selection = oklch_hex(ladder.selection, 0.07, seed.accent);
    let surfaces = [
        canvas.as_str(),
        raised.as_str(),
        overlay.as_str(),
        fill.as_str(),
        fill_hover.as_str(),
        selection.as_str(),
    ];
    let floors = [canvas.as_str(), raised.as_str(), overlay.as_str()];
    let text = |chroma: f64, hue: f64, on: &[&str], target: f64| {
        readable_on(chroma, hue, on, target, ladder.text_from, dark)
    };
    let signal = |chroma: f64, hue: f64| {
        let fill = oklch_hex(ladder.signal_fill, 0.05, hue);
        let on = [floors[0], floors[1], floors[2], fill.as_str()];
        (text(chroma, hue, &on, SIGNAL_CONTRAST), fill)
    };
    let tint = (seed.base.chroma * 2.0).max(0.025);
    let (ai, ai_fill) = signal(0.16, seed.ai);
    let (danger, danger_fill) = signal(0.17, seed.danger);
    let (success, success_fill) = signal(0.15, seed.success);
    ThemeRoles {
        heading: text(
            seed.heading.chroma,
            seed.heading.hue,
            &surfaces,
            HEADING_CONTRAST,
        ),
        ink: text(
            seed.base.chroma * 0.6,
            seed.base.hue,
            &surfaces,
            INK_CONTRAST,
        ),
        muted: text(tint, seed.base.hue, &surfaces, MUTED_CONTRAST),
        faint: text(tint, seed.base.hue, &floors, FAINT_CONTRAST),
        accent: text(0.13, seed.accent, &floors, SIGNAL_CONTRAST),
        ai,
        ai_fill,
        danger,
        danger_fill,
        success,
        success_fill,
        selection,
        hairline: base(ladder.hairline, 1.0),
        scrollbar: base(ladder.scrollbar, 1.0),
        canvas,
        raised,
        overlay,
        fill,
        fill_hover,
    }
}

pub const DESAFINADO_SEED_ID: &str = "desafinado";

use ThemeMode::{Dark, Light};

const fn seed(
    id: &'static str,
    title: &'static str,
    source: Option<(u32, u32)>,
    mode: ThemeMode,
    base: (f64, f64),
    heading: (f64, f64),
    hues: [f64; 4],
) -> ThemeSeed {
    let [accent, ai, danger, success] = hues;
    ThemeSeed {
        id,
        title,
        source,
        mode,
        base: Hue {
            hue: base.0,
            chroma: base.1,
        },
        heading: Hue {
            hue: heading.0,
            chroma: heading.1,
        },
        accent,
        ai,
        danger,
        success,
    }
}

#[rustfmt::skip]
pub const THEME_SEEDS: &[ThemeSeed] = &[
    seed("desafinado", "Desafinado", None, Dark, (285.0, 0.008), (70.0, 0.08), [245.0, 50.0, 10.0, 148.0]),
    seed("solitude", "Solitude", Some((166, 303)), Dark, (187.0, 0.035), (93.0, 0.1), [205.0, 40.0, 10.0, 148.0]),
    seed("in-a-sentimental-mood", "In a Sentimental Mood", Some((166, 341)), Dark, (187.0, 0.035), (215.0, 0.07), [260.0, 93.0, 25.0, 148.0]),
    seed("black-and-tan-fantasy", "Black and Tan Fantasy", Some((182, 298)), Dark, (60.0, 0.015), (106.0, 0.15), [263.0, 61.0, 20.0, 148.0]),
    seed("mood-indigo", "Mood Indigo", Some((265, 211)), Dark, (268.0, 0.06), (98.0, 0.17), [290.0, 49.0, 10.0, 155.0]),
    seed("sophisticated-lady", "Sophisticated Lady", Some((276, 141)), Dark, (240.0, 0.025), (51.0, 0.09), [243.0, 126.0, 5.0, 170.0]),
    seed("prelude-to-a-kiss", "Prelude to a Kiss", Some((277, 310)), Dark, (140.0, 0.03), (97.0, 0.08), [300.0, 63.0, 5.0, 148.0]),
    seed("harlem-air-shaft", "Harlem Air Shaft", Some((298, 289)), Dark, (285.0, 0.04), (106.0, 0.15), [277.0, 61.0, 25.0, 148.0]),
    seed("the-mooche", "The Mooche", Some((340, 139)), Dark, (250.0, 0.03), (225.0, 0.08), [270.0, 185.0, 30.0, 135.0]),
    seed("creole-love-call", "Creole Love Call", Some((341, 348)), Dark, (336.0, 0.06), (215.0, 0.1), [260.0, 111.0, 25.0, 155.0]),
    seed("black-brown-and-beige", "Black, Brown and Beige", Some((145, 323)), Light, (72.0, 0.03), (45.0, 0.09), [112.0, 293.0, 15.0, 148.0]),
    seed("rockin-in-rhythm", "Rockin' in Rhythm", Some((173, 253)), Light, (106.0, 0.025), (336.0, 0.12), [195.0, 49.0, 15.0, 135.0]),
    seed("im-beginning-to-see-the-light", "I'm Beginning to See the Light", Some((173, 289)), Light, (106.0, 0.025), (293.0, 0.12), [230.0, 180.0, 25.0, 140.0]),
    seed("east-st-louis-toodle-oo", "East St. Louis Toodle-Oo", Some((210, 323)), Light, (72.0, 0.03), (112.0, 0.08), [230.0, 60.0, 25.0, 148.0]),
    seed("it-dont-mean-a-thing", "It Don't Mean a Thing", Some((251, 253)), Light, (106.0, 0.02), (336.0, 0.12), [225.0, 175.0, 12.0, 130.0]),
    seed("satin-doll", "Satin Doll", Some((251, 261)), Light, (96.0, 0.02), (225.0, 0.08), [250.0, 170.0, 12.0, 135.0]),
    seed("i-got-it-bad", "I Got It Bad", Some((251, 296)), Light, (97.0, 0.02), (65.0, 0.08), [225.0, 170.0, 12.0, 135.0]),
    seed("caravan", "Caravan", Some((263, 335)), Light, (63.0, 0.03), (225.0, 0.07), [200.0, 291.0, 25.0, 145.0]),
    seed("cotton-tail", "Cotton Tail", Some((276, 314)), Light, (51.0, 0.02), (329.0, 0.11), [200.0, 261.0, 30.0, 140.0]),
    seed("dont-get-around-much-anymore", "Don't Get Around Much Anymore", Some((287, 314)), Light, (199.0, 0.03), (329.0, 0.11), [96.0, 261.0, 30.0, 148.0]),
    seed("concerto-for-cootie", "Concerto for Cootie", Some((298, 313)), Light, (106.0, 0.02), (61.0, 0.1), [250.0, 170.0, 20.0, 135.0]),
    seed("harlem", "Harlem", Some((314, 126)), Light, (85.0, 0.025), (329.0, 0.11), [87.0, 261.0, 30.0, 148.0]),
    seed("ko-ko", "Ko-Ko", Some((335, 202)), Light, (155.0, 0.025), (225.0, 0.07), [67.0, 291.0, 27.0, 145.0]),
    seed("azure", "Azure", Some((336, 345)), Light, (198.0, 0.025), (189.0, 0.08), [96.0, 293.0, 30.0, 145.0]),
    seed("c-jam-blues", "C Jam Blues", Some((345, 189)), Light, (106.0, 0.03), (139.0, 0.08), [250.0, 293.0, 30.0, 148.0]),
];
