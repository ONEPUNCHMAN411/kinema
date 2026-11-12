//! Colour, as hex strings in the document and as linear maths in the checks.
//!
//! Nothing here is display-referred guesswork: contrast is computed on relative
//! luminance, and palette work happens in OKLab so hue distance means what it looks like.

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Rgba { r, g, b, a }
    }

    pub const fn opaque(r: u8, g: u8, b: u8) -> Self {
        Rgba { r, g, b, a: 255 }
    }

    pub const BLACK: Rgba = Rgba::opaque(0, 0, 0);
    pub const WHITE: Rgba = Rgba::opaque(255, 255, 255);

    pub fn parse(s: &str) -> Result<Self, String> {
        let t = s.trim().trim_start_matches('#');
        let hex = |i: usize, n: usize| -> Result<u8, String> {
            u8::from_str_radix(&t[i..i + n], 16).map_err(|_| format!("bad hex in `{s}`"))
        };
        match t.len() {
            3 => Ok(Rgba::opaque(
                hex(0, 1)? * 17,
                hex(1, 1)? * 17,
                hex(2, 1)? * 17,
            )),
            6 => Ok(Rgba::opaque(hex(0, 2)?, hex(2, 2)?, hex(4, 2)?)),
            8 => Ok(Rgba::new(hex(0, 2)?, hex(2, 2)?, hex(4, 2)?, hex(6, 2)?)),
            _ => Err(format!("`{s}` is not #rgb, #rrggbb or #rrggbbaa")),
        }
    }

    pub fn to_hex(self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }

    /// The sRGB transfer function, linear to display, unquantised (0 to 1).
    ///
    /// Quantisation has to be the *last* thing that happens, or dithering has nothing to
    /// work with: a bias applied to an already-rounded value can only ever return it.
    pub fn srgb_from_linear(v: f64) -> f64 {
        let v = v.clamp(0.0, 1.0);
        if v <= 0.003_130_8 {
            12.92 * v
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        }
    }

    /// Build a colour from linear-light channels, encoding through the sRGB transfer.
    ///
    /// Ramp interpolation happens in linear light: mixing two sRGB values directly is what
    /// makes a black-to-white gradient go grey in the middle rather than bright.
    pub fn from_linear(r: f64, g: f64, b: f64) -> Rgba {
        let enc = |v: f64| -> u8 { (Self::srgb_from_linear(v) * 255.0).round() as u8 };
        Rgba::opaque(enc(r), enc(g), enc(b))
    }

    /// sRGB EOTF: decode one channel to linear light.
    pub fn linear_channel(c: u8) -> f64 {
        let v = c as f64 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    }

    /// WCAG relative luminance.
    pub fn relative_luminance(self) -> f64 {
        0.2126 * Self::linear_channel(self.r)
            + 0.7152 * Self::linear_channel(self.g)
            + 0.0722 * Self::linear_channel(self.b)
    }

    /// WCAG 2 contrast ratio, 1.0 to 21.0.
    pub fn contrast_ratio(self, other: Rgba) -> f64 {
        let (a, b) = (self.relative_luminance(), other.relative_luminance());
        let (hi, lo) = if a > b { (a, b) } else { (b, a) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// OKLab, for perceptual palette distance.
    pub fn oklab(self) -> (f64, f64, f64) {
        let (r, g, b) = (
            Self::linear_channel(self.r),
            Self::linear_channel(self.g),
            Self::linear_channel(self.b),
        );
        let l = 0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b;
        let m = 0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b;
        let s = 0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b;
        let (l_, m_, s_) = (l.cbrt(), m.cbrt(), s.cbrt());
        (
            0.210_454_255_3 * l_ + 0.793_617_785_0 * m_ - 0.004_072_046_8 * s_,
            1.977_998_495_1 * l_ - 2.428_592_205_0 * m_ + 0.450_593_709_9 * s_,
            0.025_904_037_1 * l_ + 0.782_771_766_2 * m_ - 0.808_675_766_0 * s_,
        )
    }

    pub fn oklab_chroma(self) -> f64 {
        let (_, a, b) = self.oklab();
        (a * a + b * b).sqrt()
    }

    /// Hue in degrees, 0 to 360.
    pub fn oklab_hue(self) -> f64 {
        let (_, a, b) = self.oklab();
        let deg = b.atan2(a).to_degrees();
        if deg < 0.0 {
            deg + 360.0
        } else {
            deg
        }
    }

    /// True when the colour carries enough chroma to read as an accent rather than a neutral.
    pub fn is_accent(self) -> bool {
        self.oklab_chroma() > 0.09
    }

    pub fn lerp(self, other: Rgba, u: f64) -> Rgba {
        let mix = |a: u8, b: u8| {
            (a as f64 + (b as f64 - a as f64) * u)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        Rgba::new(
            mix(self.r, other.r),
            mix(self.g, other.g),
            mix(self.b, other.b),
            mix(self.a, other.a),
        )
    }

    /// The readable partner of this colour: black on light grounds, white on dark ones.
    pub fn ideal_text(self) -> Rgba {
        if self.relative_luminance() > 0.35 {
            Rgba::BLACK
        } else {
            Rgba::WHITE
        }
    }

    /// The nearest colour to `self` at or above `target` contrast against `ground`.
    pub fn readable_on(self, ground: Rgba, target: f64) -> Rgba {
        if self.contrast_ratio(ground) >= target {
            return self;
        }
        let toward = if ground.relative_luminance() > 0.18 {
            Rgba::BLACK
        } else {
            Rgba::WHITE
        };
        // Bisect toward the readable end until the target is met.
        let (mut lo, mut hi) = (0.0f64, 1.0f64);
        let mut best = toward;
        for _ in 0..24 {
            let mid = (lo + hi) / 2.0;
            let cand = self.lerp(toward, mid);
            if cand.contrast_ratio(ground) >= target {
                best = cand;
                hi = mid;
            } else {
                lo = mid;
            }
        }
        best
    }
}

impl fmt::Display for Rgba {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl Serialize for Rgba {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Rgba {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Rgba::parse(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_prints_hex() {
        assert_eq!(Rgba::parse("#0af").unwrap(), Rgba::opaque(0, 170, 255));
        assert_eq!(Rgba::parse("f2b3c4").unwrap().to_hex(), "#f2b3c4");
        assert_eq!(Rgba::parse("#11223344").unwrap().a, 0x44);
        assert!(Rgba::parse("#12345").is_err());
    }

    #[test]
    fn contrast_matches_wcag_reference() {
        // Black on white is the 21:1 maximum.
        let ratio = Rgba::BLACK.contrast_ratio(Rgba::WHITE);
        assert!((ratio - 21.0).abs() < 0.01, "got {ratio}");
        // #767676 on white is the classic boundary grey at 4.54:1: it passes.
        let boundary = Rgba::opaque(0x76, 0x76, 0x76);
        assert!(boundary.contrast_ratio(Rgba::WHITE) >= 4.5);
        // A step lighter fails body text.
        let grey = Rgba::opaque(0x80, 0x80, 0x80);
        assert!(grey.contrast_ratio(Rgba::WHITE) < 4.5);
    }

    #[test]
    fn accents_are_separated_from_neutrals() {
        assert!(Rgba::opaque(0xff, 0x00, 0x66).is_accent());
        assert!(!Rgba::opaque(0x80, 0x80, 0x80).is_accent());
    }

    #[test]
    fn readable_on_reaches_the_target() {
        let ground = Rgba::opaque(0xf5, 0xf5, 0xf5);
        let weak = Rgba::opaque(0x99, 0x99, 0x99);
        let fixed = weak.readable_on(ground, 4.5);
        assert!(
            fixed.contrast_ratio(ground) >= 4.5,
            "{} on {}",
            fixed,
            ground
        );
    }

    #[test]
    fn hue_is_perceptual() {
        // Pure red and pure blue are far apart in hue but similar in chroma.
        let red = Rgba::opaque(0xff, 0, 0);
        let blue = Rgba::opaque(0, 0, 0xff);
        assert!((red.oklab_hue() - blue.oklab_hue()).abs() > 90.0);
    }
}
