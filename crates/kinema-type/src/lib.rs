//! The type system: one scale, optical spacing, and pairings that come with reasons.
//!
//! Agents pick sizes one at a time and end up with six of them, all within 15% of each
//! other, and tracking that never changes with size. A modular scale fixes the first
//! problem; optical tracking and leading fix the second. Both are rules, so both are
//! numbers here rather than advice in a prompt.

/// The house scale. Every size in a document should land on one of these steps, which is
/// what `TYPE-SCALE` reports.
pub const HOUSE_BASE: f64 = 16.0;
pub const HOUSE_RATIO: f64 = 1.25;
pub const HOUSE_STEPS: usize = 10;

/// A step of the scale with the spacing it needs at that size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Step {
    pub index: usize,
    pub size: f64,
    /// Extra advance per gap, in pixels, for this size.
    pub tracking: f64,
    /// Multiple of the size between baselines.
    pub leading: f64,
}

/// Size at a step: `base * ratio^index`.
pub fn size_at(base: f64, ratio: f64, index: usize) -> f64 {
    base * ratio.powi(index as i32)
}

/// Optical tracking: display type wants to be tightened, small type wants air.
///
/// The curve is logarithmic in the ratio to a 16 px reference and clamped at 3% of the
/// size either way, so it never produces the "spaced out caps" tell or letters that
/// collide.
pub fn optical_tracking(size: f64) -> f64 {
    if size <= 0.0 {
        return 0.0;
    }
    let em = -0.018 * (size / HOUSE_BASE).ln();
    let em = em.clamp(-0.03, 0.03);
    // `+ 0.0` normalises -0.0, which reads as a bug in JSON output.
    (em * size * 1000.0).round() / 1000.0 + 0.0
}

/// Optical leading: display sets tight, body sets open. Interpolated on a log scale
/// between 16 px (1.5) and 96 px (1.1).
pub fn optical_leading(size: f64) -> f64 {
    if size <= 0.0 {
        return 1.5;
    }
    let t = ((size / HOUSE_BASE).ln() / (96.0f64 / HOUSE_BASE).ln()).clamp(0.0, 1.0);
    let v = 1.5 - 0.4 * t;
    (v * 1000.0).round() / 1000.0 + 0.0
}

/// The ladder an agent should be choosing from.
pub fn ladder(base: f64, ratio: f64, steps: usize) -> Vec<Step> {
    (0..steps)
        .map(|index| {
            let size = (size_at(base, ratio, index) * 100.0).round() / 100.0;
            Step {
                index,
                size,
                tracking: optical_tracking(size),
                leading: optical_leading(size),
            }
        })
        .collect()
}

/// The house ladder.
pub fn house_ladder() -> Vec<Step> {
    ladder(HOUSE_BASE, HOUSE_RATIO, HOUSE_STEPS)
}

/// The nearest step to `size`, and how far off it is (`|log(size / step)|`).
pub fn nearest_step(size: f64, steps: &[Step]) -> Option<(Step, f64)> {
    if size <= 0.0 || steps.is_empty() {
        return None;
    }
    steps
        .iter()
        .map(|s| (*s, (size / s.size).ln().abs()))
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

/// Step names, so a layer can be described rather than numbered.
pub fn role_of(index: usize) -> &'static str {
    match index {
        0 => "caption",
        1 => "body",
        2 => "body-large",
        3 => "subtitle",
        4 => "title",
        5 => "display",
        6 => "display-large",
        7 => "hero",
        8 => "banner",
        _ => "oversize",
    }
}

/// A pairing that the bundle can actually set: families, a ladder, and the reason.
#[derive(Debug, Clone)]
pub struct Pairing {
    pub id: &'static str,
    pub display: &'static str,
    pub body: &'static str,
    pub mono: &'static str,
    pub why: &'static str,
    pub use_when: &'static [&'static str],
}

/// Every pairing the bundled faces can honour. The display face carries the title, the
/// body face the running text, and the mono anything that looks like data.
pub const PAIRINGS: &[Pairing] = &[
    Pairing {
        id: "editorial",
        display: "Lato",
        body: "Lato",
        mono: "Space Mono",
        why: "one grotesque at two weights keeps the page calm; the mono is reserved for \
              figures, labels and code so it reads as information rather than decoration",
        use_when: &["editorial", "explainer", "brand", "documentary", "poster"],
    },
    Pairing {
        id: "technical",
        display: "Space Mono",
        body: "Lato",
        mono: "Space Mono",
        why: "a mono display face signals the machine subject immediately, and the grotesque \
              body keeps the explanation readable at small sizes",
        use_when: &[
            "technical",
            "developer",
            "terminal",
            "code",
            "data",
            "product",
        ],
    },
    Pairing {
        id: "quiet",
        display: "Lato",
        body: "Lato",
        mono: "Lato",
        why: "a single family, hierarchy carried by size, weight and space; the safest choice \
              when the content has no data to annotate",
        use_when: &["minimal", "luxury", "cinematic", "manifesto", "quiet"],
    },
];

/// Choose a pairing from a brief.
///
/// Scoring is keyword overlap on `use_when`, and the tie-break is declaration order, so the
/// answer is deterministic and explainable — an agent can read `why` and disagree with it.
pub fn recommend(brief: &str) -> (&'static Pairing, f64) {
    let words: Vec<String> = brief
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_string())
        .collect();
    let mut best: (&'static Pairing, f64) = (&PAIRINGS[0], 0.0);
    for p in PAIRINGS {
        let mut score = 0.0;
        for w in p.use_when.iter() {
            if words.iter().any(|b| b == w) {
                score += 1.0;
            }
        }
        if score > best.1 {
            best = (p, score);
        }
    }
    best
}

/// The face a role maps to, given a pairing.
pub fn family_for(pairing: &Pairing, role: &str) -> &'static str {
    match role {
        "mono" => pairing.mono,
        "display" | "title" => pairing.display,
        _ => pairing.body,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_house_scale_hits_round_numbers_at_the_bottom() {
        let l = house_ladder();
        assert_eq!(l.len(), HOUSE_STEPS);
        assert!((l[0].size - 16.0).abs() < 0.01);
        assert!((l[4].size - 39.06).abs() < 0.01, "{}", l[4].size);
        assert!(l.windows(2).all(|w| w[1].size > w[0].size));
        assert_eq!(role_of(0), "caption");
        assert_eq!(role_of(4), "title");
    }

    #[test]
    fn display_is_tightened_and_small_text_opens_up() {
        let big = optical_tracking(96.0);
        let body = optical_tracking(16.0);
        let small = optical_tracking(11.0);
        assert!(big < 0.0, "display should tighten, got {big}");
        assert!(body.abs() < 0.001, "16 px is the neutral reference");
        assert!(small > 0.0, "small type should open up, got {small}");
        assert!(optical_tracking(400.0) >= -0.03 * 400.0 - 0.001, "clamped");
    }

    #[test]
    fn leading_tightens_as_type_grows() {
        assert!((optical_leading(16.0) - 1.5).abs() < 0.001);
        assert!((optical_leading(96.0) - 1.1).abs() < 0.001);
        assert!(optical_leading(48.0) < optical_leading(16.0));
        assert!(optical_leading(48.0) > optical_leading(96.0));
        assert!(
            (optical_leading(400.0) - 1.1).abs() < 0.001,
            "clamped at display"
        );
    }

    #[test]
    fn the_starter_size_is_on_the_scale() {
        let (step, off) = nearest_step(96.0, &house_ladder()).unwrap();
        assert!((step.size - 95.37).abs() < 0.01);
        assert!(off < 0.02, "96 px is within 2% of a step, off by {off}");
    }

    #[test]
    fn an_arbitrary_size_is_reported_off_scale() {
        let (step, off) = nearest_step(24.0, &house_ladder()).unwrap();
        assert!((step.size - 25.0).abs() < 0.01);
        assert!(off > 0.03, "24 px is a made-up size, off by {off}");
    }

    #[test]
    fn pairing_recommendation_is_keyword_driven_and_stable() {
        let (p, score) = recommend("a technical developer video about data");
        assert_eq!(p.id, "technical");
        assert!(score >= 2.0);
        let (p, _) = recommend("an editorial explainer for a brand");
        assert_eq!(p.id, "editorial");
        let (p, _) = recommend("something else entirely");
        assert_eq!(p.id, "editorial", "the first pairing is the tie-break");
    }

    #[test]
    fn every_pairing_names_faces_the_bundle_has() {
        for p in PAIRINGS {
            for family in [p.display, p.body, p.mono] {
                assert!(
                    kinema_text::find(family, 400, false).is_some(),
                    "{} names {family}, which is not bundled",
                    p.id
                );
            }
        }
    }

    #[test]
    fn roles_map_to_faces() {
        let p = &PAIRINGS[1];
        assert_eq!(family_for(p, "mono"), "Space Mono");
        assert_eq!(family_for(p, "title"), "Space Mono");
        assert_eq!(family_for(p, "body"), "Lato");
    }
}
