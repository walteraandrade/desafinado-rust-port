use desafinado_features::theme::{catalog::theme_options, color::contrast};
#[test]
fn pairs() {
    let mut fails = std::collections::BTreeMap::<String, usize>::new();
    for option in theme_options() {
        let r = &option.tokens.roles;
        let pairs: Vec<(&str, &String, &String, f64)> = vec![
            ("faint/canvas", &r.faint, &r.canvas, 4.5),
            ("faint/raised", &r.faint, &r.raised, 4.5),
            ("faint/overlay", &r.faint, &r.overlay, 4.5),
            ("faint/fill", &r.faint, &r.fill, 4.5),
            ("canvas/danger", &r.canvas, &r.danger, 4.5),
            ("canvas/success", &r.canvas, &r.success, 4.5),
            ("ink/ai_fill", &r.ink, &r.ai_fill, 4.5),
            ("ink/danger_fill", &r.ink, &r.danger_fill, 4.5),
            ("ink/success_fill", &r.ink, &r.success_fill, 4.5),
            ("ai/fill", &r.ai, &r.fill, 4.5),
            ("accent/fill", &r.accent, &r.fill, 4.5),
            ("danger/fill", &r.danger, &r.fill, 4.5),
            ("muted/ai_fill", &r.muted, &r.ai_fill, 4.5),
            ("accent/raised", &r.accent, &r.raised, 4.5),
            ("heading/fill", &r.heading, &r.fill, 4.5),
            ("faint/selection", &r.faint, &r.selection, 3.0),
            ("ink/selection", &r.ink, &r.selection, 4.5),
            ("scrollbar/canvas", &r.scrollbar, &r.canvas, 3.0),
            ("accent/canvas nontext", &r.accent, &r.canvas, 3.0),
            ("fill/canvas", &r.fill, &r.canvas, 1.0),
        ];
        for (name, a, b, floor) in pairs {
            if contrast(a, b) < floor { *fails.entry(name.into()).or_default() += 1; }
        }
    }
    println!("{} themes; fails: {:?}", theme_options().len(), fails);
}
