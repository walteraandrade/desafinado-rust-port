use super::tokens::ThemeMode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaletteColor {
    pub name: &'static str,
    pub hex: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaletteJobs {
    pub canvas: &'static str,
    pub raised: &'static str,
    pub ink: &'static str,
    pub ai: &'static str,
    pub danger: &'static str,
    pub accent: Option<&'static str>,
    pub selection: Option<&'static str>,
    pub fill: Option<&'static str>,
    pub hairline: Option<&'static str>,
    pub scrollbar: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub id: &'static str,
    pub title: &'static str,
    pub source: (u32, u32),
    pub mode: ThemeMode,
    pub colors: &'static [PaletteColor],
    pub jobs: PaletteJobs,
}

pub const BOOK_PALETTES: &[Palette] = &[
    Palette {
        id: "solitude",
        title: "Solitude",
        source: (166, 303),
        mode: ThemeMode::Dark,
        colors: &[
            PaletteColor {
                name: "Deep Slate Green",
                hex: "#112f2c",
            },
            PaletteColor {
                name: "Deep Slate Olive",
                hex: "#253122",
            },
            PaletteColor {
                name: "Grenadine Pink",
                hex: "#f48067",
            },
            PaletteColor {
                name: "Naples Yellow",
                hex: "#fbe6a0",
            },
            PaletteColor {
                name: "Neutral Gray",
                hex: "#b6bfc1",
            },
            PaletteColor {
                name: "Peach Red",
                hex: "#f15a30",
            },
        ],
        jobs: PaletteJobs {
            canvas: "Deep Slate Green",
            raised: "Deep Slate Olive",
            ink: "Neutral Gray",
            ai: "Naples Yellow",
            danger: "Grenadine Pink",
            accent: None,
            selection: None,
            fill: None,
            hairline: Some("Peach Red"),
            scrollbar: None,
        },
    },
    Palette {
        id: "in-a-sentimental-mood",
        title: "In a Sentimental Mood",
        source: (166, 341),
        mode: ThemeMode::Dark,
        colors: &[
            PaletteColor {
                name: "Cossack Green",
                hex: "#437742",
            },
            PaletteColor {
                name: "Deep Slate Green",
                hex: "#112f2c",
            },
            PaletteColor {
                name: "Deep Slate Olive",
                hex: "#253122",
            },
            PaletteColor {
                name: "Grenadine Pink",
                hex: "#f48067",
            },
            PaletteColor {
                name: "Light Glaucous Blue",
                hex: "#a5c8d1",
            },
            PaletteColor {
                name: "Naples Yellow",
                hex: "#fbe6a0",
            },
        ],
        jobs: PaletteJobs {
            canvas: "Deep Slate Green",
            raised: "Deep Slate Olive",
            ink: "Light Glaucous Blue",
            ai: "Naples Yellow",
            danger: "Grenadine Pink",
            accent: None,
            selection: None,
            fill: None,
            hairline: Some("Cossack Green"),
            scrollbar: None,
        },
    },
    Palette {
        id: "black-and-tan-fantasy",
        title: "Black and Tan Fantasy",
        source: (182, 298),
        mode: ThemeMode::Dark,
        colors: &[
            PaletteColor {
                name: "Black",
                hex: "#111314",
            },
            PaletteColor {
                name: "Deep Indigo",
                hex: "#051230",
            },
            PaletteColor {
                name: "Lemon Yellow",
                hex: "#f8ed43",
            },
            PaletteColor {
                name: "Peach Red",
                hex: "#f15a30",
            },
            PaletteColor {
                name: "Raw Sienna",
                hex: "#bb7125",
            },
            PaletteColor {
                name: "Vandyke Brown",
                hex: "#4b3317",
            },
        ],
        jobs: PaletteJobs {
            canvas: "Black",
            raised: "Deep Indigo",
            ink: "Lemon Yellow",
            ai: "Raw Sienna",
            danger: "Peach Red",
            accent: None,
            selection: None,
            fill: Some("Vandyke Brown"),
            hairline: None,
            scrollbar: None,
        },
    },
    Palette {
        id: "mood-indigo",
        title: "Mood Indigo",
        source: (265, 211),
        mode: ThemeMode::Dark,
        colors: &[
            PaletteColor {
                name: "Apricot Orange",
                hex: "#f68c50",
            },
            PaletteColor {
                name: "Apricot Yellow",
                hex: "#ffdd00",
            },
            PaletteColor {
                name: "Deep Indigo",
                hex: "#051230",
            },
            PaletteColor {
                name: "Dull Violet Black",
                hex: "#1e0e3f",
            },
            PaletteColor {
                name: "Old Rose",
                hex: "#d46d7a",
            },
            PaletteColor {
                name: "Olive Yellow",
                hex: "#a6a159",
            },
        ],
        jobs: PaletteJobs {
            canvas: "Deep Indigo",
            raised: "Dull Violet Black",
            ink: "Apricot Yellow",
            ai: "Apricot Orange",
            danger: "Old Rose",
            accent: Some("Olive Yellow"),
            selection: None,
            fill: None,
            hairline: None,
            scrollbar: None,
        },
    },
    Palette {
        id: "sophisticated-lady",
        title: "Sophisticated Lady",
        source: (276, 141),
        mode: ThemeMode::Dark,
        colors: &[
            PaletteColor {
                name: "Black",
                hex: "#111314",
            },
            PaletteColor {
                name: "Dark Tyrian Blue",
                hex: "#12354e",
            },
            PaletteColor {
                name: "Eosine Pink",
                hex: "#f37f94",
            },
            PaletteColor {
                name: "Orange",
                hex: "#f37420",
            },
            PaletteColor {
                name: "Seashell Pink",
                hex: "#fdd4bd",
            },
            PaletteColor {
                name: "Yellow Green",
                hex: "#afd472",
            },
        ],
        jobs: PaletteJobs {
            canvas: "Black",
            raised: "Dark Tyrian Blue",
            ink: "Seashell Pink",
            ai: "Yellow Green",
            danger: "Eosine Pink",
            accent: None,
            selection: None,
            fill: None,
            hairline: Some("Orange"),
            scrollbar: None,
        },
    },
    Palette {
        id: "prelude-to-a-kiss",
        title: "Prelude to a Kiss",
        source: (277, 310),
        mode: ThemeMode::Dark,
        colors: &[
            PaletteColor {
                name: "Deep Slate Olive",
                hex: "#253122",
            },
            PaletteColor {
                name: "Dull Violet Black",
                hex: "#1e0e3f",
            },
            PaletteColor {
                name: "Olive",
                hex: "#837e31",
            },
            PaletteColor {
                name: "Pinkish Cinnamon",
                hex: "#eeb480",
            },
            PaletteColor {
                name: "Rosolanc Purple",
                hex: "#b73f74",
            },
            PaletteColor {
                name: "Spinel Red",
                hex: "#f27291",
            },
            PaletteColor {
                name: "Sulpher Yellow",
                hex: "#f5ecc2",
            },
        ],
        jobs: PaletteJobs {
            canvas: "Deep Slate Olive",
            raised: "Dull Violet Black",
            ink: "Sulpher Yellow",
            ai: "Pinkish Cinnamon",
            danger: "Spinel Red",
            accent: None,
            selection: None,
            fill: None,
            hairline: Some("Olive"),
            scrollbar: Some("Rosolanc Purple"),
        },
    },
    Palette {
        id: "harlem-air-shaft",
        title: "Harlem Air Shaft",
        source: (298, 289),
        mode: ThemeMode::Dark,
        colors: &[
            PaletteColor {
                name: "Black",
                hex: "#111314",
            },
            PaletteColor {
                name: "Dull Violet Black",
                hex: "#1e0e3f",
            },
            PaletteColor {
                name: "Lemon Yellow",
                hex: "#f8ed43",
            },
            PaletteColor {
                name: "Light Green Yellow",
                hex: "#c7d14f",
            },
            PaletteColor {
                name: "Peach Red",
                hex: "#f15a30",
            },
            PaletteColor {
                name: "Raw Sienna",
                hex: "#bb7125",
            },
            PaletteColor {
                name: "Violet Blue",
                hex: "#40456a",
            },
        ],
        jobs: PaletteJobs {
            canvas: "Black",
            raised: "Dull Violet Black",
            ink: "Lemon Yellow",
            ai: "Raw Sienna",
            danger: "Peach Red",
            accent: Some("Light Green Yellow"),
            selection: None,
            fill: Some("Violet Blue"),
            hairline: None,
            scrollbar: None,
        },
    },
    Palette {
        id: "the-mooche",
        title: "The Mooche",
        source: (340, 139),
        mode: ThemeMode::Dark,
        colors: &[
            PaletteColor {
                name: "Black",
                hex: "#111314",
            },
            PaletteColor {
                name: "Deep Indigo",
                hex: "#051230",
            },
            PaletteColor {
                name: "Neutral Gray",
                hex: "#b6bfc1",
            },
            PaletteColor {
                name: "Peach Red",
                hex: "#f15a30",
            },
            PaletteColor {
                name: "Salvia Blue",
                hex: "#97acc8",
            },
            PaletteColor {
                name: "Sea Green",
                hex: "#00b49b",
            },
        ],
        jobs: PaletteJobs {
            canvas: "Black",
            raised: "Deep Indigo",
            ink: "Neutral Gray",
            ai: "Sea Green",
            danger: "Peach Red",
            accent: Some("Salvia Blue"),
            selection: None,
            fill: None,
            hairline: None,
            scrollbar: None,
        },
    },
    Palette {
        id: "creole-love-call",
        title: "Creole Love Call",
        source: (341, 348),
        mode: ThemeMode::Dark,
        colors: &[
            PaletteColor {
                name: "Cossack Green",
                hex: "#437742",
            },
            PaletteColor {
                name: "Cotinga Purple",
                hex: "#501345",
            },
            PaletteColor {
                name: "Deep Slate Olive",
                hex: "#253122",
            },
            PaletteColor {
                name: "Grenadine Pink",
                hex: "#f48067",
            },
            PaletteColor {
                name: "Light Glaucous Blue",
                hex: "#a5c8d1",
            },
            PaletteColor {
                name: "Olive Buff",
                hex: "#c1c494",
            },
        ],
        jobs: PaletteJobs {
            canvas: "Cotinga Purple",
            raised: "Deep Slate Olive",
            ink: "Light Glaucous Blue",
            ai: "Olive Buff",
            danger: "Grenadine Pink",
            accent: None,
            selection: None,
            fill: None,
            hairline: Some("Cossack Green"),
            scrollbar: None,
        },
    },
    Palette {
        id: "black-brown-and-beige",
        title: "Black, Brown and Beige",
        source: (145, 323),
        mode: ThemeMode::Light,
        colors: &[
            PaletteColor {
                name: "Black",
                hex: "#111314",
            },
            PaletteColor {
                name: "Brown",
                hex: "#7c4226",
            },
            PaletteColor {
                name: "Cinnamon Buff",
                hex: "#fdc57e",
            },
            PaletteColor {
                name: "Citron Yellow",
                hex: "#b2b73e",
            },
            PaletteColor {
                name: "Dull Violet Black",
                hex: "#1e0e3f",
            },
            PaletteColor {
                name: "Madder Brown",
                hex: "#762c19",
            },
        ],
        jobs: PaletteJobs {
            canvas: "Cinnamon Buff",
            raised: "Citron Yellow",
            ink: "Black",
            ai: "Dull Violet Black",
            danger: "Madder Brown",
            accent: None,
            selection: None,
            fill: None,
            hairline: Some("Brown"),
            scrollbar: None,
        },
    },
    Palette {
        id: "rockin-in-rhythm",
        title: "Rockin' in Rhythm",
        source: (173, 253),
        mode: ThemeMode::Light,
        colors: &[
            PaletteColor {
                name: "Apricot Orange",
                hex: "#f68c50",
            },
            PaletteColor {
                name: "Cotinga Purple",
                hex: "#501345",
            },
            PaletteColor {
                name: "Lemon Yellow",
                hex: "#f8ed43",
            },
            PaletteColor {
                name: "Madder Brown",
                hex: "#762c19",
            },
            PaletteColor {
                name: "Slate Color",
                hex: "#34454c",
            },
            PaletteColor {
                name: "Turquoise Green",
                hex: "#b5decc",
            },
        ],
        jobs: PaletteJobs {
            canvas: "Lemon Yellow",
            raised: "Turquoise Green",
            ink: "Slate Color",
            ai: "Cotinga Purple",
            danger: "Madder Brown",
            accent: None,
            selection: None,
            fill: None,
            hairline: Some("Apricot Orange"),
            scrollbar: None,
        },
    },
    Palette {
        id: "im-beginning-to-see-the-light",
        title: "I'm Beginning to See the Light",
        source: (173, 289),
        mode: ThemeMode::Light,
        colors: &[
            PaletteColor {
                name: "Dull Violet Black",
                hex: "#1e0e3f",
            },
            PaletteColor {
                name: "Lemon Yellow",
                hex: "#f8ed43",
            },
            PaletteColor {
                name: "Light Green Yellow",
                hex: "#c7d14f",
            },
            PaletteColor {
                name: "Madder Brown",
                hex: "#762c19",
            },
            PaletteColor {
                name: "Turquoise Green",
                hex: "#b5decc",
            },
            PaletteColor {
                name: "Violet Blue",
                hex: "#40456a",
            },
        ],
        jobs: PaletteJobs {
            canvas: "Lemon Yellow",
            raised: "Turquoise Green",
            ink: "Dull Violet Black",
            ai: "Violet Blue",
            danger: "Madder Brown",
            accent: None,
            selection: Some("Light Green Yellow"),
            fill: None,
            hairline: None,
            scrollbar: None,
        },
    },
    Palette {
        id: "east-st-louis-toodle-oo",
        title: "East St. Louis Toodle-Oo",
        source: (210, 323),
        mode: ThemeMode::Light,
        colors: &[
            PaletteColor {
                name: "Black",
                hex: "#111314",
            },
            PaletteColor {
                name: "Cinnamon Buff",
                hex: "#fdc57e",
            },
            PaletteColor {
                name: "Citron Yellow",
                hex: "#b2b73e",
            },
            PaletteColor {
                name: "Lemon Yellow",
                hex: "#f8ed43",
            },
            PaletteColor {
                name: "Lincoln Green",
                hex: "#555832",
            },
            PaletteColor {
                name: "Madder Brown",
                hex: "#762c19",
            },
        ],
        jobs: PaletteJobs {
            canvas: "Cinnamon Buff",
            raised: "Lemon Yellow",
            ink: "Black",
            ai: "Lincoln Green",
            danger: "Madder Brown",
            accent: None,
            selection: None,
            fill: Some("Citron Yellow"),
            hairline: None,
            scrollbar: None,
        },
    },
    Palette {
        id: "it-dont-mean-a-thing",
        title: "It Don't Mean a Thing",
        source: (251, 253),
        mode: ThemeMode::Light,
        colors: &[
            PaletteColor {
                name: "Apricot Orange",
                hex: "#f68c50",
            },
            PaletteColor {
                name: "Cotinga Purple",
                hex: "#501345",
            },
            PaletteColor {
                name: "Diamine Green",
                hex: "#1a7444",
            },
            PaletteColor {
                name: "Lemon Yellow",
                hex: "#f8ed43",
            },
            PaletteColor {
                name: "Red",
                hex: "#a72144",
            },
            PaletteColor {
                name: "Slate Color",
                hex: "#34454c",
            },
            PaletteColor {
                name: "Yellow",
                hex: "#fff200",
            },
        ],
        jobs: PaletteJobs {
            canvas: "Lemon Yellow",
            raised: "Yellow",
            ink: "Cotinga Purple",
            ai: "Diamine Green",
            danger: "Red",
            accent: Some("Slate Color"),
            selection: None,
            fill: Some("Apricot Orange"),
            hairline: None,
            scrollbar: None,
        },
    },
    Palette {
        id: "satin-doll",
        title: "Satin Doll",
        source: (251, 261),
        mode: ThemeMode::Light,
        colors: &[
            PaletteColor {
                name: "Calamine BLue",
                hex: "#78cdd0",
            },
            PaletteColor {
                name: "Diamine Green",
                hex: "#1a7444",
            },
            PaletteColor {
                name: "Pale Lemon Yellow",
                hex: "#ffefae",
            },
            PaletteColor {
                name: "Red",
                hex: "#a72144",
            },
            PaletteColor {
                name: "Slate Color",
                hex: "#34454c",
            },
            PaletteColor {
                name: "Warm Gray",
                hex: "#a1a39a",
            },
            PaletteColor {
                name: "Yellow",
                hex: "#fff200",
            },
        ],
        jobs: PaletteJobs {
            canvas: "Pale Lemon Yellow",
            raised: "Yellow",
            ink: "Slate Color",
            ai: "Diamine Green",
            danger: "Red",
            accent: None,
            selection: None,
            fill: Some("Calamine BLue"),
            hairline: Some("Warm Gray"),
            scrollbar: None,
        },
    },
    Palette {
        id: "i-got-it-bad",
        title: "I Got It Bad",
        source: (251, 296),
        mode: ThemeMode::Light,
        colors: &[
            PaletteColor {
                name: "Diamine Green",
                hex: "#1a7444",
            },
            PaletteColor {
                name: "Ochraceous Salmon",
                hex: "#d8a37b",
            },
            PaletteColor {
                name: "Pale Raw Umber",
                hex: "#71502f",
            },
            PaletteColor {
                name: "Red",
                hex: "#a72144",
            },
            PaletteColor {
                name: "Slate Color",
                hex: "#34454c",
            },
            PaletteColor {
                name: "Sulpher Yellow",
                hex: "#f5ecc2",
            },
            PaletteColor {
                name: "Yellow",
                hex: "#fff200",
            },
        ],
        jobs: PaletteJobs {
            canvas: "Sulpher Yellow",
            raised: "Yellow",
            ink: "Slate Color",
            ai: "Diamine Green",
            danger: "Red",
            accent: Some("Pale Raw Umber"),
            selection: None,
            fill: None,
            hairline: Some("Ochraceous Salmon"),
            scrollbar: None,
        },
    },
    Palette {
        id: "caravan",
        title: "Caravan",
        source: (263, 335),
        mode: ThemeMode::Light,
        colors: &[
            PaletteColor {
                name: "Burnt Sienna",
                hex: "#ae5224",
            },
            PaletteColor {
                name: "Pinkish Cinnamon",
                hex: "#eeb480",
            },
            PaletteColor {
                name: "Slate Color",
                hex: "#34454c",
            },
            PaletteColor {
                name: "Turquoise Green",
                hex: "#b5decc",
            },
            PaletteColor {
                name: "Vandyke Red",
                hex: "#82241f",
            },
            PaletteColor {
                name: "Violet",
                hex: "#4f4086",
            },
            PaletteColor {
                name: "Yellow Orange",
                hex: "#f99d1b",
            },
        ],
        jobs: PaletteJobs {
            canvas: "Pinkish Cinnamon",
            raised: "Turquoise Green",
            ink: "Slate Color",
            ai: "Violet",
            danger: "Vandyke Red",
            accent: None,
            selection: None,
            fill: Some("Yellow Orange"),
            hairline: Some("Burnt Sienna"),
            scrollbar: None,
        },
    },
    Palette {
        id: "cotton-tail",
        title: "Cotton Tail",
        source: (276, 314),
        mode: ThemeMode::Light,
        colors: &[
            PaletteColor {
                name: "Black",
                hex: "#111314",
            },
            PaletteColor {
                name: "Deep Lyons Blue",
                hex: "#1c4286",
            },
            PaletteColor {
                name: "Dusky Madder Violet",
                hex: "#4e1d4c",
            },
            PaletteColor {
                name: "Eosine Pink",
                hex: "#f37f94",
            },
            PaletteColor {
                name: "Hay's Russet",
                hex: "#793327",
            },
            PaletteColor {
                name: "Seashell Pink",
                hex: "#fdd4bd",
            },
            PaletteColor {
                name: "Yellow Green",
                hex: "#afd472",
            },
        ],
        jobs: PaletteJobs {
            canvas: "Seashell Pink",
            raised: "Yellow Green",
            ink: "Black",
            ai: "Deep Lyons Blue",
            danger: "Hay's Russet",
            accent: Some("Dusky Madder Violet"),
            selection: None,
            fill: Some("Eosine Pink"),
            hairline: None,
            scrollbar: None,
        },
    },
    Palette {
        id: "dont-get-around-much-anymore",
        title: "Don't Get Around Much Anymore",
        source: (287, 314),
        mode: ThemeMode::Light,
        colors: &[
            PaletteColor {
                name: "Calamine BLue",
                hex: "#78cdd0",
            },
            PaletteColor {
                name: "Deep Lyons Blue",
                hex: "#1c4286",
            },
            PaletteColor {
                name: "Dusky Madder Violet",
                hex: "#4e1d4c",
            },
            PaletteColor {
                name: "Eosine Pink",
                hex: "#f37f94",
            },
            PaletteColor {
                name: "Hay's Russet",
                hex: "#793327",
            },
            PaletteColor {
                name: "Pale King's Blue",
                hex: "#a7d4e4",
            },
            PaletteColor {
                name: "Pyrite Yellow",
                hex: "#cab356",
            },
        ],
        jobs: PaletteJobs {
            canvas: "Calamine BLue",
            raised: "Pale King's Blue",
            ink: "Dusky Madder Violet",
            ai: "Deep Lyons Blue",
            danger: "Hay's Russet",
            accent: None,
            selection: Some("Pyrite Yellow"),
            fill: Some("Eosine Pink"),
            hairline: None,
            scrollbar: None,
        },
    },
    Palette {
        id: "concerto-for-cootie",
        title: "Concerto for Cootie",
        source: (298, 313),
        mode: ThemeMode::Light,
        colors: &[
            PaletteColor {
                name: "Black",
                hex: "#111314",
            },
            PaletteColor {
                name: "Carmine",
                hex: "#cc1236",
            },
            PaletteColor {
                name: "Diamine Green",
                hex: "#1a7444",
            },
            PaletteColor {
                name: "Lemon Yellow",
                hex: "#f8ed43",
            },
            PaletteColor {
                name: "Peach Red",
                hex: "#f15a30",
            },
            PaletteColor {
                name: "Raw Sienna",
                hex: "#bb7125",
            },
            PaletteColor {
                name: "Yellow",
                hex: "#fff200",
            },
        ],
        jobs: PaletteJobs {
            canvas: "Lemon Yellow",
            raised: "Yellow",
            ink: "Black",
            ai: "Diamine Green",
            danger: "Carmine",
            accent: None,
            selection: None,
            fill: Some("Raw Sienna"),
            hairline: Some("Peach Red"),
            scrollbar: None,
        },
    },
    Palette {
        id: "harlem",
        title: "Harlem",
        source: (314, 126),
        mode: ThemeMode::Light,
        colors: &[
            PaletteColor {
                name: "Deep Lyons Blue",
                hex: "#1c4286",
            },
            PaletteColor {
                name: "Dusky Madder Violet",
                hex: "#4e1d4c",
            },
            PaletteColor {
                name: "Eosine Pink",
                hex: "#f37f94",
            },
            PaletteColor {
                name: "Hay's Russet",
                hex: "#793327",
            },
            PaletteColor {
                name: "Ivory Buff",
                hex: "#ebd3a2",
            },
            PaletteColor {
                name: "Yellow Ocher",
                hex: "#e2b540",
            },
        ],
        jobs: PaletteJobs {
            canvas: "Ivory Buff",
            raised: "Yellow Ocher",
            ink: "Dusky Madder Violet",
            ai: "Deep Lyons Blue",
            danger: "Hay's Russet",
            accent: None,
            selection: None,
            fill: Some("Eosine Pink"),
            hairline: None,
            scrollbar: None,
        },
    },
    Palette {
        id: "ko-ko",
        title: "Ko-Ko",
        source: (335, 202),
        mode: ThemeMode::Light,
        colors: &[
            PaletteColor {
                name: "Cobalt Green",
                hex: "#96d1aa",
            },
            PaletteColor {
                name: "Slate Color",
                hex: "#34454c",
            },
            PaletteColor {
                name: "Turquoise Green",
                hex: "#b5decc",
            },
            PaletteColor {
                name: "Vandyke Red",
                hex: "#82241f",
            },
            PaletteColor {
                name: "Violet",
                hex: "#4f4086",
            },
            PaletteColor {
                name: "Yellow Orange",
                hex: "#f99d1b",
            },
        ],
        jobs: PaletteJobs {
            canvas: "Cobalt Green",
            raised: "Turquoise Green",
            ink: "Slate Color",
            ai: "Violet",
            danger: "Vandyke Red",
            accent: None,
            selection: None,
            fill: Some("Yellow Orange"),
            hairline: None,
            scrollbar: None,
        },
    },
    Palette {
        id: "azure",
        title: "Azure",
        source: (336, 345),
        mode: ThemeMode::Light,
        colors: &[
            PaletteColor {
                name: "Blackish Olive",
                hex: "#42533e",
            },
            PaletteColor {
                name: "Blue Violet",
                hex: "#6450a1",
            },
            PaletteColor {
                name: "Eosine Pink",
                hex: "#f37f94",
            },
            PaletteColor {
                name: "Hay's Russet",
                hex: "#793327",
            },
            PaletteColor {
                name: "Nile Blue",
                hex: "#bce4e5",
            },
            PaletteColor {
                name: "Pale Lemon Yellow",
                hex: "#ffefae",
            },
            PaletteColor {
                name: "Venice Green",
                hex: "#62c6bf",
            },
        ],
        jobs: PaletteJobs {
            canvas: "Nile Blue",
            raised: "Pale Lemon Yellow",
            ink: "Blackish Olive",
            ai: "Blue Violet",
            danger: "Hay's Russet",
            accent: None,
            selection: None,
            fill: None,
            hairline: Some("Eosine Pink"),
            scrollbar: Some("Venice Green"),
        },
    },
    Palette {
        id: "c-jam-blues",
        title: "C Jam Blues",
        source: (345, 189),
        mode: ThemeMode::Light,
        colors: &[
            PaletteColor {
                name: "Blue Violet",
                hex: "#6450a1",
            },
            PaletteColor {
                name: "Deep Slate Olive",
                hex: "#253122",
            },
            PaletteColor {
                name: "Hay's Russet",
                hex: "#793327",
            },
            PaletteColor {
                name: "Lemon Yellow",
                hex: "#f8ed43",
            },
            PaletteColor {
                name: "Nile Blue",
                hex: "#bce4e5",
            },
            PaletteColor {
                name: "Venice Green",
                hex: "#62c6bf",
            },
        ],
        jobs: PaletteJobs {
            canvas: "Lemon Yellow",
            raised: "Nile Blue",
            ink: "Deep Slate Olive",
            ai: "Blue Violet",
            danger: "Hay's Russet",
            accent: None,
            selection: None,
            fill: Some("Venice Green"),
            hairline: None,
            scrollbar: None,
        },
    },
];
