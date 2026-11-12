//! Real typefaces.
//!
//! The engine carries its own faces so a render never depends on what the machine has
//! installed. Every face is a static OFL font with its licence text beside it, and the
//! document names a family; if the family is not here the checks say so (`TYPE-FAKE`)
//! rather than silently substituting something the reviewer never saw.

use fontdue::{Font, FontSettings};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// The metadata that travels with a bundled face. Licence and attribution are data, not
/// prose buried in a README.
#[derive(Debug, Clone, Copy)]
pub struct FaceInfo {
    pub family: &'static str,
    pub weight: u16,
    pub italic: bool,
    /// Repository path, so an agent can find the file it is being measured with.
    pub file: &'static str,
    pub spdx: &'static str,
    pub author: &'static str,
    pub source_url: &'static str,
    pub licence_file: &'static str,
}

macro_rules! face {
    ($family:literal, $weight:literal, $italic:literal, $file:literal, $author:expr, $licence:expr) => {
        FaceInfo {
            family: $family,
            weight: $weight,
            italic: $italic,
            file: $file,
            spdx: "OFL-1.1",
            author: $author,
            source_url: "https://fonts.google.com/",
            licence_file: $licence,
        }
    };
}

const LATO_R: &[u8] = include_bytes!("../../../assets/fonts/Lato-Regular.ttf");
const LATO_B: &[u8] = include_bytes!("../../../assets/fonts/Lato-Bold.ttf");
const LATO_I: &[u8] = include_bytes!("../../../assets/fonts/Lato-Italic.ttf");
const MONO_R: &[u8] = include_bytes!("../../../assets/fonts/SpaceMono-Regular.ttf");
const MONO_B: &[u8] = include_bytes!("../../../assets/fonts/SpaceMono-Bold.ttf");

const LATO_AUTHOR: &str = "Łukasz Dziedzic (tyPoland)";
const MONO_AUTHOR: &str = "The Space Mono Project Authors";
const LATO_LICENCE: &str = "assets/fonts/OFL-Lato.txt";
const MONO_LICENCE: &str = "assets/fonts/OFL-SpaceMono.txt";

pub const FACES: &[FaceInfo] = &[
    face!(
        "Lato",
        400,
        false,
        "assets/fonts/Lato-Regular.ttf",
        LATO_AUTHOR,
        LATO_LICENCE
    ),
    face!(
        "Lato",
        700,
        false,
        "assets/fonts/Lato-Bold.ttf",
        LATO_AUTHOR,
        LATO_LICENCE
    ),
    face!(
        "Lato",
        400,
        true,
        "assets/fonts/Lato-Italic.ttf",
        LATO_AUTHOR,
        LATO_LICENCE
    ),
    face!(
        "Space Mono",
        400,
        false,
        "assets/fonts/SpaceMono-Regular.ttf",
        MONO_AUTHOR,
        MONO_LICENCE
    ),
    face!(
        "Space Mono",
        700,
        false,
        "assets/fonts/SpaceMono-Bold.ttf",
        MONO_AUTHOR,
        MONO_LICENCE
    ),
];

/// The family a document gets when it does not name one, and when it names one that is
/// not bundled.
pub const DEFAULT_FAMILY: &str = "Lato";

pub struct Face {
    pub info: &'static FaceInfo,
    bytes: &'static [u8],
    /// Parsed on first use: a render that sets one weight should not pay for five faces.
    font: OnceLock<Font>,
}

impl std::fmt::Debug for Face {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Face({} {} {})",
            self.info.family,
            self.info.weight,
            if self.info.italic { "italic" } else { "roman" }
        )
    }
}

/// Rasterised coverage, shared between every use of the same glyph at the same size.
/// Motion blur redraws each subframe, and a sequence redraws the same glyph every frame,
/// so without this the outline is rasterised dozens of times for one pixel-identical result.
type Glyph = Arc<(fontdue::Metrics, Vec<u8>)>;

type GlyphKey = (usize, char, i32);
type GlyphCache = Mutex<HashMap<GlyphKey, Glyph>>;

fn glyph_cache() -> &'static GlyphCache {
    static CACHE: OnceLock<GlyphCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

impl Face {
    fn font(&self) -> &Font {
        self.font.get_or_init(|| {
            Font::from_bytes(self.bytes, FontSettings::default()).expect("bundled face must parse")
        })
    }

    pub fn rasterize(&self, ch: char, size: f32) -> (fontdue::Metrics, Vec<u8>) {
        self.font().rasterize(ch, size.max(1.0))
    }

    fn index(&self) -> usize {
        FACES
            .iter()
            .position(|f| std::ptr::eq(f, self.info))
            .unwrap_or(0)
    }

    /// The glyph's metrics and coverage at this size, from the cache when possible.
    pub fn glyph(&self, ch: char, size: f32) -> Glyph {
        // A thousandth of a pixel is finer than any size a document can express.
        let key = (self.index(), ch, (size.max(1.0) * 1000.0).round() as i32);
        if let Ok(cache) = glyph_cache().lock() {
            if let Some(hit) = cache.get(&key) {
                return hit.clone();
            }
        }
        let (metrics, bitmap) = self.rasterize(ch, size);
        let glyph: Glyph = Arc::new((metrics, bitmap));
        if let Ok(mut cache) = glyph_cache().lock() {
            cache.insert(key, glyph.clone());
        }
        glyph
    }

    pub fn metrics(&self, ch: char, size: f32) -> fontdue::Metrics {
        self.font().metrics(ch, size.max(1.0))
    }

    /// Ascent and descent in pixels, descent given as a positive distance.
    pub fn vertical(&self, size: f32) -> (f32, f32) {
        match self.font().horizontal_line_metrics(size.max(1.0)) {
            Some(m) => (m.ascent, m.descent.abs()),
            // Every bundled face has hhea metrics; this is only a safety net.
            None => (size * 0.8, size * 0.2),
        }
    }

    pub fn kern(&self, left: char, right: char, size: f32) -> f32 {
        self.font()
            .horizontal_kern(left, right, size.max(1.0))
            .unwrap_or(0.0)
    }
}

fn registry() -> &'static Vec<Face> {
    static REGISTRY: OnceLock<Vec<Face>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let bytes: [&'static [u8]; 5] = [LATO_R, LATO_B, LATO_I, MONO_R, MONO_B];
        FACES
            .iter()
            .zip(bytes)
            .map(|(info, data)| Face {
                info,
                bytes: data,
                font: OnceLock::new(),
            })
            .collect()
    })
}

/// Bundled families, in the order they were declared.
pub fn families() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for f in FACES {
        if !out.contains(&f.family) {
            out.push(f.family);
        }
    }
    out
}

/// Weights a family actually ships.
pub fn weights(family: &str) -> Vec<u16> {
    let mut out: Vec<u16> = Vec::new();
    for f in FACES {
        if f.family.eq_ignore_ascii_case(family) && !out.contains(&f.weight) {
            out.push(f.weight);
        }
    }
    out.sort_unstable();
    out
}

pub fn has_italic(family: &str) -> bool {
    FACES
        .iter()
        .any(|f| f.family.eq_ignore_ascii_case(family) && f.italic)
}

/// Resolve a family, weight and style to a real face.
///
/// Falling back is allowed here — a render must always produce something — but the
/// fallback is visible: `find` returning a different weight than asked is exactly what the
/// `TYPE-FAKE` check reports.
pub fn find(family: &str, weight: u16, italic: bool) -> Option<&'static Face> {
    let reg = registry();
    let family_matches = |f: &&Face| f.info.family.eq_ignore_ascii_case(family);
    let italic_ok = has_italic(family) && italic;
    // Exact style first, then the same family in any weight, then the default family.
    reg.iter()
        .filter(family_matches)
        .filter(|f| f.info.italic == italic_ok)
        .min_by_key(|f| (f.info.weight as i64 - weight as i64).abs())
        .or_else(|| {
            reg.iter()
                .filter(family_matches)
                .min_by_key(|f| (f.info.weight as i64 - weight as i64).abs())
        })
}

/// Like [`find`], but never empty: an unknown family is drawn in the default face so a
/// render always produces pixels. `TYPE-FAKE` is what turns that into a reported defect.
fn resolve(family: &str, weight: u16, italic: bool) -> &'static Face {
    find(family, weight, italic).unwrap_or_else(|| {
        let reg = registry();
        reg.iter()
            .filter(|f| f.info.family == DEFAULT_FAMILY && !f.info.italic)
            .min_by_key(|f| (f.info.weight as i64 - weight as i64).abs())
            .unwrap_or(&reg[0])
    })
}

/// Width of one line, without kerning (measurement stays additive so tracking is exact).
pub fn line_width(
    family: &str,
    weight: u16,
    italic: bool,
    line: &str,
    size: f32,
    tracking: f32,
) -> f32 {
    if line.is_empty() {
        return 0.0;
    }
    let face = resolve(family, weight, italic);
    let mut width = 0.0;
    let chars: Vec<char> = line.chars().collect();
    for ch in &chars {
        width += face.metrics(*ch, size).advance_width;
    }
    width + tracking * (chars.len() as f32 - 1.0)
}

/// `[width, height]` of a block, with leading. One line is exactly the em box tall.
pub fn measure(
    text: &str,
    family: &str,
    weight: u16,
    italic: bool,
    size: f32,
    tracking: f32,
    line_height: f32,
) -> (f32, f32) {
    let lines: Vec<&str> = text.split('\n').collect();
    let width = lines
        .iter()
        .map(|l| line_width(family, weight, italic, l, size, tracking))
        .fold(0.0f32, f32::max);
    let face = resolve(family, weight, italic);
    let (ascent, descent) = face.vertical(size);
    let single = ascent + descent;
    let height = single + (lines.len() as f32 - 1.0) * size * line_height;
    (width, height.max(0.0))
}

/// Where one glyph goes: pen position and the baseline it sits on.
#[derive(Debug, Clone, Copy)]
pub struct Placed {
    pub ch: char,
    pub face: &'static Face,
    pub x: f32,
    pub baseline: f32,
    /// Position in the block counting every character, which is what a `char` stagger
    /// indexes.
    pub char_index: usize,
    /// Word number, incremented on whitespace, for a `word` stagger.
    pub word_index: usize,
    pub line_index: usize,
}

/// Lay a block out inside a box. `box_y` is the top of the block, not the baseline.
#[allow(clippy::too_many_arguments)]
pub fn layout(
    text: &str,
    family: &str,
    weight: u16,
    italic: bool,
    size: f32,
    tracking: f32,
    line_height: f32,
    align: Align,
    box_x: f32,
    box_y: f32,
    box_w: f32,
) -> Vec<Placed> {
    let face = resolve(family, weight, italic);
    let (ascent, _) = face.vertical(size);
    let mut out = Vec::new();
    let mut char_index = 0usize;
    let mut word_index = 0usize;
    for (i, line) in text.split('\n').enumerate() {
        let width = line_width(family, weight, italic, line, size, tracking);
        let start = match align {
            Align::Left => box_x,
            Align::Center => box_x + (box_w - width) / 2.0,
            Align::Right => box_x + box_w - width,
        };
        let baseline = box_y + ascent + i as f32 * size * line_height;
        let mut pen = start;
        let mut prev: Option<char> = None;
        for ch in line.chars() {
            if let Some(p) = prev {
                pen += face.kern(p, ch, size);
            }
            if ch.is_whitespace() {
                word_index += 1;
            }
            out.push(Placed {
                ch,
                face,
                x: pen,
                baseline,
                char_index,
                word_index,
                line_index: i,
            });
            char_index += 1;
            pen += face.metrics(ch, size).advance_width + tracking;
            prev = Some(ch);
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// Where a rasterised glyph's top-left corner sits, given its pen position and baseline.
pub fn glyph_origin(placed: &Placed, metrics: &fontdue::Metrics) -> (f32, f32) {
    let x = placed.x + metrics.xmin as f32;
    let y = placed.baseline - (metrics.ymin as f32 + metrics.height as f32);
    (x, y)
}

/// A catalogue entry as an agent sees it.
pub fn catalogue() -> Vec<(FaceInfo, Option<u16>)> {
    FACES.iter().map(|f| (*f, Some(f.weight))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_faces_parse_and_are_listed() {
        assert_eq!(families(), vec!["Lato", "Space Mono"]);
        assert_eq!(weights("Lato"), vec![400, 700]);
        assert_eq!(weights("Space Mono"), vec![400, 700]);
        assert!(has_italic("Lato"));
        assert!(
            !has_italic("Space Mono"),
            "no oblique is bundled for the mono"
        );
        assert_eq!(FACES.len(), 5);
        for f in FACES {
            assert_eq!(f.spdx, "OFL-1.1");
            let on_disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join(f.file);
            assert!(on_disk.exists(), "{} missing", f.file);
        }
    }

    #[test]
    fn measurement_scales_with_size() {
        let small = measure("HELLO", "Lato", 700, false, 50.0, 0.0, 1.2);
        let large = measure("HELLO", "Lato", 700, false, 100.0, 0.0, 1.2);
        assert!(small.0 > 100.0, "{small:?}");
        assert!(
            (large.0 / small.0 - 2.0).abs() < 0.02,
            "{} {}",
            large.0,
            small.0
        );
        assert!((large.1 / small.1 - 2.0).abs() < 0.02);
    }

    #[test]
    fn the_widest_line_sets_the_block_width() {
        let one = measure("IIII", "Lato", 400, false, 60.0, 0.0, 1.2);
        let two = measure("IIII\nWWWWWW", "Lato", 400, false, 60.0, 0.0, 1.2);
        assert!(two.0 > one.0);
        assert!(two.1 > one.1, "two lines are taller than one");
        let taller = measure("IIII\nWWWWWW", "Lato", 400, false, 60.0, 0.0, 2.0);
        assert!(taller.1 > two.1, "leading adds height");
    }

    #[test]
    fn tracking_widens_a_line() {
        let plain = line_width("Lato", 400, false, "TRACKING", 40.0, 0.0);
        let loose = line_width("Lato", 400, false, "TRACKING", 40.0, 5.0);
        assert!(loose > plain + 30.0, "{loose} vs {plain}");
    }

    #[test]
    fn layout_places_glyphs_left_to_right_and_down() {
        let placed = layout(
            "AB\nCD",
            "Lato",
            400,
            false,
            40.0,
            0.0,
            1.5,
            Align::Left,
            0.0,
            0.0,
            400.0,
        );
        assert_eq!(placed.len(), 4);
        assert!(placed[1].x > placed[0].x);
        assert!((placed[2].baseline - placed[0].baseline - 60.0).abs() < 0.01);
        assert_eq!(placed[2].x, placed[0].x);
    }

    #[test]
    fn alignment_moves_the_line_inside_its_box() {
        let start = |align| {
            layout(
                "HI", "Lato", 400, false, 60.0, 0.0, 1.2, align, 0.0, 0.0, 600.0,
            )[0]
            .x
        };
        let left = start(Align::Left);
        let center = start(Align::Center);
        let right = start(Align::Right);
        assert!(left < center && center < right, "{left} {center} {right}");
        assert!(
            (right - (left + 600.0 - line_width("Lato", 400, false, "HI", 60.0, 0.0))).abs() < 0.01
        );
    }

    #[test]
    fn glyphs_rasterise_with_coverage_and_stay_deterministic() {
        let face = find("Lato", 700, false).unwrap();
        let (metrics, bitmap) = face.rasterize('A', 64.0);
        assert!(metrics.width > 10 && metrics.height > 10);
        let ink = bitmap.iter().filter(|c| **c > 32).count();
        assert!(ink > 150, "expected letterform ink, got {ink}");
        let (m2, again) = face.rasterize('A', 64.0);
        assert_eq!(bitmap, again);
        assert_eq!(metrics.width, m2.width);
    }

    #[test]
    fn an_unknown_family_falls_back_but_find_reports_nothing() {
        assert!(find("Helvetica", 400, false).is_none());
        let (w, h) = measure("HI", "Helvetica", 400, false, 40.0, 0.0, 1.2);
        assert!(w > 0.0 && h > 0.0);
        let resolved = resolve("Helvetica", 400, false);
        assert_eq!(resolved.info.family, DEFAULT_FAMILY);
    }

    #[test]
    fn italic_is_only_used_when_the_family_has_one() {
        let italic = find("Lato", 400, true).unwrap();
        assert!(italic.info.italic);
        let mono = find("Space Mono", 400, true).unwrap();
        assert!(
            !mono.info.italic,
            "the mono has no italic, so it must not fake one"
        );
    }

    #[test]
    fn weights_resolve_to_the_nearest_real_face() {
        assert_eq!(find("Lato", 700, false).unwrap().info.weight, 700);
        assert_eq!(find("Lato", 650, false).unwrap().info.weight, 700);
        assert_eq!(find("Lato", 100, false).unwrap().info.weight, 400);
    }
}
