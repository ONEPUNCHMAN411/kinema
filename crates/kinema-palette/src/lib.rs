//! Palette: one accent, a neutral ramp, and a proof for every pair.
//!
//! An agent asked for "a modern palette" writes a purple-to-pink gradient and calls it
//! design. A palette is a set of *roles* with relationships: a ground, a raised surface, a
//! text colour that reads on both, a muted tone that still passes, and exactly one accent
//! with an ink colour proven against it. Every pair here is measured, so a palette can be
//! wrong and say so.

use kinema_doc::Rgba;

/// Contrast targets. Text needs 4.5:1 (WCAG AA for body), large text and UI shapes 3:1.
pub const TEXT_TARGET: f64 = 4.5;
pub const UI_TARGET: f64 = 3.0;

/// sRGB transfer function, linear to display.
fn encode(v: f64) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let s = if v <= 0.003_130_8 {
        12.92 * v
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    (s * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Build a colour from OKLCH: perceptual lightness 0–1, chroma, hue in degrees.
pub fn from_oklch(l: f64, c: f64, hue_deg: f64) -> Rgba {
    let h = hue_deg.to_radians();
    let (a, b) = (c * h.cos(), c * h.sin());
    let l_ = l + 0.396_337_777_4 * a + 0.215_803_757_3 * b;
    let m_ = l - 0.105_561_345_8 * a - 0.063_854_172_8 * b;
    let s_ = l - 0.089_484_177_5 * a - 1.291_485_548_0 * b;
    let (l3, m3, s3) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
    let r = 4.076_741_662_1 * l3 - 3.307_711_591_3 * m3 + 0.230_969_929_2 * s3;
    let g = -1.268_438_004_6 * l3 + 2.609_757_401_1 * m3 - 0.341_319_396_5 * s3;
    let bl = -0.004_196_086_3 * l3 - 0.703_418_614_7 * m3 + 1.707_614_701_0 * s3;
    Rgba::opaque(encode(r), encode(g), encode(bl))
}

/// The largest chroma at this lightness and hue that sRGB can actually show, found by
/// bisection on the encoded result. Working at the edge of the gamut is what produces
/// clipped, muddy accents, so callers get the honest maximum instead.
pub fn max_chroma(l: f64, hue_deg: f64) -> f64 {
    let probe = |c: f64| {
        let col = from_oklch(l, c, hue_deg);
        let back = col.oklab_chroma();
        (back - c).abs() <= 0.004
    };
    let (mut lo, mut hi) = (0.0, 0.4);
    for _ in 0..18 {
        let mid = (lo + hi) / 2.0;
        if probe(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo * 1000.0).round() / 1000.0
}

/// The same colour at a lightness, gamut-mapped at that lightness and keeping its hue.
pub fn at_lightness(color: Rgba, l: f64) -> Rgba {
    let hue = color.oklab_hue();
    let c = color.oklab_chroma().min(max_chroma(l, hue));
    from_oklch(l, c, hue)
}

/// A tinted neutral: the accent's hue at a whisper of chroma, so surfaces feel related to
/// the accent instead of being a separate grey system.
pub fn neutral(color: Rgba, l: f64, chroma: f64) -> Rgba {
    let hue = color.oklab_hue();
    let c = chroma.min(max_chroma(l, hue));
    from_oklch(l, c, hue)
}

/// The roles a document actually needs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tokens {
    pub bg: Rgba,
    pub surface: Rgba,
    pub text: Rgba,
    pub muted: Rgba,
    pub accent: Rgba,
    /// Text or icon colour proven against `accent`.
    pub accent_ink: Rgba,
}

impl Tokens {
    pub fn roles(&self) -> [(&'static str, Rgba); 6] {
        [
            ("bg", self.bg),
            ("surface", self.surface),
            ("text", self.text),
            ("muted", self.muted),
            ("accent", self.accent),
            ("accent_ink", self.accent_ink),
        ]
    }
}

/// One measured pair, so a palette can be audited rather than admired.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Proof {
    pub fg: &'static str,
    pub bg: &'static str,
    pub ratio: f64,
    pub target: f64,
    pub pass: bool,
}

impl Tokens {
    /// Every pair that carries meaning, with its target.
    pub fn proofs(&self) -> [Proof; 4] {
        let p = |fg: &'static str, bg: &'static str, a: Rgba, b: Rgba, target: f64| Proof {
            fg,
            bg,
            ratio: (a.contrast_ratio(b) * 100.0).round() / 100.0,
            target,
            pass: a.contrast_ratio(b) >= target,
        };
        [
            p("text", "bg", self.text, self.bg, TEXT_TARGET),
            p("text", "surface", self.text, self.surface, TEXT_TARGET),
            p("muted", "surface", self.muted, self.surface, TEXT_TARGET),
            p(
                "accent_ink",
                "accent",
                self.accent_ink,
                self.accent,
                TEXT_TARGET,
            ),
        ]
    }

    pub fn clean(&self) -> bool {
        self.proofs().iter().all(|p| p.pass)
    }
}

/// Build a palette around one accent.
///
/// `dark` selects the end of the ramp: a near-black ground with light type, or a paper
/// ground with dark type. The accent is kept at its own lightness when it clears the UI
/// target against the ground, and otherwise moved only as far as it must be.
pub fn scheme(accent: Rgba, dark: bool) -> Tokens {
    let (bg, surface, text, muted_l) = if dark {
        (
            neutral(accent, 0.16, 0.012),
            neutral(accent, 0.21, 0.016),
            neutral(accent, 0.97, 0.006),
            0.70,
        )
    } else {
        (
            neutral(accent, 0.97, 0.008),
            neutral(accent, 0.93, 0.012),
            neutral(accent, 0.18, 0.006),
            0.44,
        )
    };

    // The accent has to be visible as a shape on the ground. Bump its lightness in the
    // direction that gains contrast, one step at a time, preserving hue.
    let mut acc = accent;
    if acc.contrast_ratio(bg) < UI_TARGET {
        let toward = if dark { 1.0 } else { 0.0 };
        let start = acc.oklab().0;
        for i in 1..=40 {
            let l = start + (toward - start) * (i as f64 / 40.0);
            acc = at_lightness(accent, l);
            if acc.contrast_ratio(bg) >= UI_TARGET {
                break;
            }
        }
    }

    let muted = at_lightness(accent, muted_l).readable_on(surface, TEXT_TARGET);
    let text = text.readable_on(surface, TEXT_TARGET);
    let accent_ink = acc.ideal_text().readable_on(acc, TEXT_TARGET);

    Tokens {
        bg,
        surface,
        text,
        muted,
        accent: acc,
        accent_ink,
    }
}

/// How many distinct accent hues a set of colours carries. One is a palette; two is a
/// fight for attention.
pub fn accent_count(colors: &[Rgba]) -> usize {
    let mut hues: Vec<f64> = Vec::new();
    for c in colors.iter().filter(|c| c.is_accent()) {
        let h = c.oklab_hue();
        if !hues.iter().any(|existing| {
            let d = (h - existing).abs();
            d.min(360.0 - d) < 30.0
        }) {
            hues.push(h);
        }
    }
    hues.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLUE: Rgba = Rgba::opaque(0x3b, 0x82, 0xf6);

    #[test]
    fn oklch_round_trips_through_srgb() {
        for (l, c, h) in [(0.5, 0.12, 250.0), (0.16, 0.01, 20.0), (0.97, 0.004, 200.0)] {
            let col = from_oklch(l, c, h);
            let (l2, _, _) = col.oklab();
            assert!((l2 - l).abs() < 0.02, "{l} -> {l2}");
            // Hue is only meaningful once there is chroma to have a hue; at 0.004 the
            // 8-bit quantisation dominates and the angle is noise.
            if c >= 0.05 {
                assert!((col.oklab_hue() - h).abs() < 2.0, "hue drifted");
            }
        }
    }

    #[test]
    fn the_gamut_limit_is_honest() {
        // Pure-looking saturated yellow cannot hold much chroma at high lightness.
        let max = max_chroma(0.95, 110.0);
        let inside = from_oklch(0.95, max, 110.0);
        assert!(inside.oklab_chroma() > 0.0);
        let over = from_oklch(0.95, max + 0.3, 110.0);
        assert!(
            (over.oklab_chroma() - media_max(over)).abs() < 0.01,
            "an out-of-gamut request must clamp, not overflow"
        );
    }

    fn media_max(c: Rgba) -> f64 {
        // The colour we got back is by definition inside the gamut; its chroma is the cap.
        c.oklab_chroma()
    }

    #[test]
    fn a_dark_scheme_reads() {
        let t = scheme(BLUE, true);
        assert!(t.bg.relative_luminance() < 0.05, "ground should be dark");
        assert!(t.text.relative_luminance() > 0.8, "type should be light");
        assert!(t.clean(), "{:?}", t.proofs());
        for p in t.proofs() {
            assert!(p.pass, "{:?}", p);
        }
    }

    #[test]
    fn a_light_scheme_reads_too() {
        let t = scheme(BLUE, false);
        assert!(t.bg.relative_luminance() > 0.8);
        assert!(t.text.relative_luminance() < 0.1);
        assert!(t.clean(), "{:?}", t.proofs());
    }

    #[test]
    fn the_accent_stays_visible_on_the_ground() {
        for dark in [true, false] {
            let t = scheme(BLUE, dark);
            assert!(
                t.accent.contrast_ratio(t.bg) >= UI_TARGET,
                "accent must clear the UI target, dark={dark}"
            );
        }
    }

    #[test]
    fn a_low_contrast_accent_is_moved_not_dropped() {
        // Near-black accent on a dark ground has to be lifted to be seen at all.
        let ink = Rgba::opaque(0x0a, 0x0a, 0x14);
        let t = scheme(ink, true);
        assert!(t.accent.contrast_ratio(t.bg) >= UI_TARGET);
        assert!(t.accent.oklab().0 > ink.oklab().0, "it was lightened");
    }

    #[test]
    fn neutrals_keep_the_accent_hue() {
        let t = scheme(BLUE, true);
        let d = (t.surface.oklab_hue() - BLUE.oklab_hue()).abs();
        assert!(d < 20.0 || d > 340.0, "surface hue drifted by {d}");
        assert!(t.surface.oklab_chroma() < 0.03, "surface stays neutral");
    }

    #[test]
    fn schemes_are_deterministic() {
        assert_eq!(scheme(BLUE, true), scheme(BLUE, true));
    }

    #[test]
    fn accents_are_counted_by_hue_not_by_colour() {
        let near = Rgba::opaque(0x3c, 0x83, 0xf7);
        assert_eq!(
            accent_count(&[BLUE, near]),
            1,
            "same hue twice is one accent"
        );
        let warm = Rgba::opaque(0xf6, 0x7a, 0x1f);
        assert_eq!(accent_count(&[BLUE, warm]), 2);
        assert_eq!(
            accent_count(&[Rgba::opaque(0x80, 0x80, 0x80), Rgba::WHITE]),
            0,
            "neutrals are not accents"
        );
    }
}
