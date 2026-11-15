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

pub const THEME_SEEDS: &[ThemeSeed] = &[
    ThemeSeed {
        id: "desafinado",
        title: "Desafinado",
        source: None,
        mode: ThemeMode::Dark,
        base: Hue {
            hue: 285.0,
            chroma: 0.008,
        },
        heading: Hue {
            hue: 70.0,
            chroma: 0.08,
        },
        accent: 245.0,
        ai: 50.0,
        danger: 10.0,
        success: 148.0,
    },
    ThemeSeed {
        id: "solitude",
        title: "Solitude",
        source: Some((166, 303)),
        mode: ThemeMode::Dark,
        base: Hue {
            hue: 187.0,
            chroma: 0.035,
        },
        heading: Hue {
            hue: 93.0,
            chroma: 0.1,
        },
        accent: 205.0,
        ai: 40.0,
        danger: 10.0,
        success: 148.0,
    },
    ThemeSeed {
        id: "in-a-sentimental-mood",
        title: "In a Sentimental Mood",
        source: Some((166, 341)),
        mode: ThemeMode::Dark,
        base: Hue {
            hue: 187.0,
            chroma: 0.035,
        },
        heading: Hue {
            hue: 215.0,
            chroma: 0.07,
        },
        accent: 260.0,
        ai: 93.0,
        danger: 25.0,
        success: 148.0,
    },
    ThemeSeed {
        id: "black-and-tan-fantasy",
        title: "Black and Tan Fantasy",
        source: Some((182, 298)),
        mode: ThemeMode::Dark,
        base: Hue {
            hue: 60.0,
            chroma: 0.015,
        },
        heading: Hue {
            hue: 106.0,
            chroma: 0.15,
        },
        accent: 263.0,
        ai: 61.0,
        danger: 20.0,
        success: 148.0,
    },
    ThemeSeed {
        id: "mood-indigo",
        title: "Mood Indigo",
        source: Some((265, 211)),
        mode: ThemeMode::Dark,
        base: Hue {
            hue: 268.0,
            chroma: 0.06,
        },
        heading: Hue {
            hue: 98.0,
            chroma: 0.17,
        },
        accent: 290.0,
        ai: 49.0,
        danger: 10.0,
        success: 155.0,
    },
    ThemeSeed {
        id: "sophisticated-lady",
        title: "Sophisticated Lady",
        source: Some((276, 141)),
        mode: ThemeMode::Dark,
        base: Hue {
            hue: 240.0,
            chroma: 0.025,
        },
        heading: Hue {
            hue: 51.0,
            chroma: 0.09,
        },
        accent: 243.0,
        ai: 126.0,
        danger: 5.0,
        success: 170.0,
    },
    ThemeSeed {
        id: "prelude-to-a-kiss",
        title: "Prelude to a Kiss",
        source: Some((277, 310)),
        mode: ThemeMode::Dark,
        base: Hue {
            hue: 140.0,
            chroma: 0.03,
        },
        heading: Hue {
            hue: 97.0,
            chroma: 0.08,
        },
        accent: 300.0,
        ai: 63.0,
        danger: 5.0,
        success: 148.0,
    },
    ThemeSeed {
        id: "harlem-air-shaft",
        title: "Harlem Air Shaft",
        source: Some((298, 289)),
        mode: ThemeMode::Dark,
        base: Hue {
            hue: 285.0,
            chroma: 0.04,
        },
        heading: Hue {
            hue: 106.0,
            chroma: 0.15,
        },
        accent: 277.0,
        ai: 61.0,
        danger: 25.0,
        success: 148.0,
    },
    ThemeSeed {
        id: "the-mooche",
        title: "The Mooche",
        source: Some((340, 139)),
        mode: ThemeMode::Dark,
        base: Hue {
            hue: 250.0,
            chroma: 0.03,
        },
        heading: Hue {
            hue: 225.0,
            chroma: 0.08,
        },
        accent: 270.0,
        ai: 185.0,
        danger: 30.0,
        success: 135.0,
    },
    ThemeSeed {
        id: "creole-love-call",
        title: "Creole Love Call",
        source: Some((341, 348)),
        mode: ThemeMode::Dark,
        base: Hue {
            hue: 336.0,
            chroma: 0.06,
        },
        heading: Hue {
            hue: 215.0,
            chroma: 0.1,
        },
        accent: 260.0,
        ai: 111.0,
        danger: 25.0,
        success: 155.0,
    },
    ThemeSeed {
        id: "black-brown-and-beige",
        title: "Black, Brown and Beige",
        source: Some((145, 323)),
        mode: ThemeMode::Light,
        base: Hue {
            hue: 72.0,
            chroma: 0.03,
        },
        heading: Hue {
            hue: 45.0,
            chroma: 0.09,
        },
        accent: 112.0,
        ai: 293.0,
        danger: 15.0,
        success: 148.0,
    },
    ThemeSeed {
        id: "rockin-in-rhythm",
        title: "Rockin' in Rhythm",
        source: Some((173, 253)),
        mode: ThemeMode::Light,
        base: Hue {
            hue: 106.0,
            chroma: 0.025,
        },
        heading: Hue {
            hue: 336.0,
            chroma: 0.12,
        },
        accent: 195.0,
        ai: 49.0,
        danger: 15.0,
        success: 135.0,
    },
    ThemeSeed {
        id: "im-beginning-to-see-the-light",
        title: "I'm Beginning to See the Light",
        source: Some((173, 289)),
        mode: ThemeMode::Light,
        base: Hue {
            hue: 106.0,
            chroma: 0.025,
        },
        heading: Hue {
            hue: 293.0,
            chroma: 0.12,
        },
        accent: 230.0,
        ai: 180.0,
        danger: 25.0,
        success: 140.0,
    },
    ThemeSeed {
        id: "east-st-louis-toodle-oo",
        title: "East St. Louis Toodle-Oo",
        source: Some((210, 323)),
        mode: ThemeMode::Light,
        base: Hue {
            hue: 72.0,
            chroma: 0.03,
        },
        heading: Hue {
            hue: 112.0,
            chroma: 0.08,
        },
        accent: 230.0,
        ai: 60.0,
        danger: 25.0,
        success: 148.0,
    },
    ThemeSeed {
        id: "it-dont-mean-a-thing",
        title: "It Don't Mean a Thing",
        source: Some((251, 253)),
        mode: ThemeMode::Light,
        base: Hue {
            hue: 106.0,
            chroma: 0.02,
        },
        heading: Hue {
            hue: 336.0,
            chroma: 0.12,
        },
        accent: 225.0,
        ai: 175.0,
        danger: 12.0,
        success: 130.0,
    },
    ThemeSeed {
        id: "satin-doll",
        title: "Satin Doll",
        source: Some((251, 261)),
        mode: ThemeMode::Light,
        base: Hue {
            hue: 96.0,
            chroma: 0.02,
        },
        heading: Hue {
            hue: 225.0,
            chroma: 0.08,
        },
        accent: 250.0,
        ai: 170.0,
        danger: 12.0,
        success: 135.0,
    },
    ThemeSeed {
        id: "i-got-it-bad",
        title: "I Got It Bad",
        source: Some((251, 296)),
        mode: ThemeMode::Light,
        base: Hue {
            hue: 97.0,
            chroma: 0.02,
        },
        heading: Hue {
            hue: 65.0,
            chroma: 0.08,
        },
        accent: 225.0,
        ai: 170.0,
        danger: 12.0,
        success: 135.0,
    },
    ThemeSeed {
        id: "caravan",
        title: "Caravan",
        source: Some((263, 335)),
        mode: ThemeMode::Light,
        base: Hue {
            hue: 63.0,
            chroma: 0.03,
        },
        heading: Hue {
            hue: 225.0,
            chroma: 0.07,
        },
        accent: 200.0,
        ai: 291.0,
        danger: 25.0,
        success: 145.0,
    },
    ThemeSeed {
        id: "cotton-tail",
        title: "Cotton Tail",
        source: Some((276, 314)),
        mode: ThemeMode::Light,
        base: Hue {
            hue: 51.0,
            chroma: 0.02,
        },
        heading: Hue {
            hue: 329.0,
            chroma: 0.11,
        },
        accent: 200.0,
        ai: 261.0,
        danger: 30.0,
        success: 140.0,
    },
    ThemeSeed {
        id: "dont-get-around-much-anymore",
        title: "Don't Get Around Much Anymore",
        source: Some((287, 314)),
        mode: ThemeMode::Light,
        base: Hue {
            hue: 199.0,
            chroma: 0.03,
        },
        heading: Hue {
            hue: 329.0,
            chroma: 0.11,
        },
        accent: 96.0,
        ai: 261.0,
        danger: 30.0,
        success: 148.0,
    },
    ThemeSeed {
        id: "concerto-for-cootie",
        title: "Concerto for Cootie",
        source: Some((298, 313)),
        mode: ThemeMode::Light,
        base: Hue {
            hue: 106.0,
            chroma: 0.02,
        },
        heading: Hue {
            hue: 61.0,
            chroma: 0.1,
        },
        accent: 250.0,
        ai: 170.0,
        danger: 20.0,
        success: 135.0,
    },
    ThemeSeed {
        id: "harlem",
        title: "Harlem",
        source: Some((314, 126)),
        mode: ThemeMode::Light,
        base: Hue {
            hue: 85.0,
            chroma: 0.025,
        },
        heading: Hue {
            hue: 329.0,
            chroma: 0.11,
        },
        accent: 87.0,
        ai: 261.0,
        danger: 30.0,
        success: 148.0,
    },
    ThemeSeed {
        id: "ko-ko",
        title: "Ko-Ko",
        source: Some((335, 202)),
        mode: ThemeMode::Light,
        base: Hue {
            hue: 155.0,
            chroma: 0.025,
        },
        heading: Hue {
            hue: 225.0,
            chroma: 0.07,
        },
        accent: 67.0,
        ai: 291.0,
        danger: 27.0,
        success: 145.0,
    },
    ThemeSeed {
        id: "azure",
        title: "Azure",
        source: Some((336, 345)),
        mode: ThemeMode::Light,
        base: Hue {
            hue: 198.0,
            chroma: 0.025,
        },
        heading: Hue {
            hue: 189.0,
            chroma: 0.08,
        },
        accent: 96.0,
        ai: 293.0,
        danger: 30.0,
        success: 145.0,
    },
    ThemeSeed {
        id: "c-jam-blues",
        title: "C Jam Blues",
        source: Some((345, 189)),
        mode: ThemeMode::Light,
        base: Hue {
            hue: 106.0,
            chroma: 0.03,
        },
        heading: Hue {
            hue: 139.0,
            chroma: 0.08,
        },
        accent: 250.0,
        ai: 293.0,
        danger: 30.0,
        success: 148.0,
    },
];
