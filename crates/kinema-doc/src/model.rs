//! The document itself: comps, layers, sources and assets.

use crate::ops::pointer_for;
use crate::prop::EvalError;
use crate::{DocError, Prop, Rgba};
use kinema_time::{Fps, TICKS_PER_SECOND};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const SCHEMA_TAG: &str = "kinema/1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Doc {
    #[serde(default = "default_schema")]
    pub schema: String,
    /// Bumped by every applied op; used for `--expect-version` compare-and-swap.
    #[serde(default)]
    pub version: u64,
    #[serde(default = "default_fps")]
    pub fps: Fps,
    #[serde(default)]
    pub comps: BTreeMap<String, Comp>,
    #[serde(default)]
    pub assets: BTreeMap<String, Asset>,
    /// The project's own design system: one accent, one scale, one pairing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub look: Option<Look>,
}

/// A look is the small set of decisions that make a project read as one design.
///
/// It is deliberately narrow: an accent, which end of the ramp to build from, the type
/// scale, and the pairing id. Everything else is derived from those, so a document can
/// state its system once and the checks can hold it to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Look {
    /// The one accent hue. Every other chromatic colour must share it.
    pub accent: Rgba,
    #[serde(default)]
    pub scheme: Scheme,
    #[serde(default)]
    pub scale: Scale,
    /// Pairing id from `kinema-type`, e.g. `editorial`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pairing: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
    #[default]
    Dark,
    Light,
}

/// The modular scale the document commits to.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Scale {
    #[serde(default = "default_scale_base")]
    pub base: f64,
    #[serde(default = "default_scale_ratio")]
    pub ratio: f64,
    #[serde(default = "default_scale_steps")]
    pub steps: usize,
}

fn default_scale_base() -> f64 {
    16.0
}

fn default_scale_ratio() -> f64 {
    1.25
}

fn default_scale_steps() -> usize {
    10
}

impl Default for Scale {
    fn default() -> Self {
        Scale {
            base: default_scale_base(),
            ratio: default_scale_ratio(),
            steps: default_scale_steps(),
        }
    }
}

fn default_schema() -> String {
    SCHEMA_TAG.to_string()
}

fn default_fps() -> Fps {
    Fps::new(30, 1)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comp {
    pub size: [u32; 2],
    /// Seconds.
    pub duration: f64,
    /// Shutter angle in degrees. 180 is the film default and 0 switches motion blur off,
    /// so a document that does not ask for it stays as sharp as it always was.
    #[serde(default)]
    pub shutter: f64,
    /// Subframe samples used when `shutter > 0`.
    #[serde(default = "default_samples")]
    pub samples: u32,
    #[serde(default)]
    pub layers: Vec<Layer>,
}

fn default_samples() -> u32 {
    8
}

impl Comp {
    pub fn duration_ticks(&self) -> i64 {
        kinema_time::seconds_to_ticks(self.duration)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayerKind {
    Solid,
    Text,
    Shape,
    Image,
    Comp,
    Adjustment,
}

impl LayerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LayerKind::Solid => "solid",
            LayerKind::Text => "text",
            LayerKind::Shape => "shape",
            LayerKind::Image => "image",
            LayerKind::Comp => "comp",
            LayerKind::Adjustment => "adjustment",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Blend {
    #[default]
    Normal,
    Screen,
    Multiply,
    Overlay,
    Add,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Transform {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<Prop>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<Prop>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<Prop>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<[f64; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer {
    pub id: String,
    pub kind: LayerKind,
    #[serde(default)]
    pub start: f64,
    #[serde(default = "default_layer_duration")]
    pub duration: f64,
    #[serde(default)]
    pub transform: Transform,
    #[serde(default = "default_opacity")]
    pub opacity: Prop,
    #[serde(default)]
    pub blend: Blend,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solid: Option<Solid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<Text>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<Shape>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mattes: Option<Matte>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modifiers: Vec<Modifier>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

fn default_layer_duration() -> f64 {
    5.0
}

fn default_opacity() -> Prop {
    Prop::number(1.0)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Solid {
    pub color: Rgba,
    /// When present, it fills the layer instead of `color`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gradient: Option<Gradient>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShapeKind {
    #[default]
    Rect,
    Ellipse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shape {
    #[serde(default)]
    pub kind: ShapeKind,
    pub size: [f64; 2],
    pub color: Rgba,
    #[serde(default)]
    pub radius: f64,
    /// Painted over `color` across the shape's box.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gradient: Option<Gradient>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GradientKind {
    #[default]
    Linear,
    Radial,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stop {
    /// Position along the gradient, 0 to 1.
    pub at: f64,
    pub color: Rgba,
}

/// A ramp. Stops are interpolated in linear light, and `dither` decides whether the
/// result is quantised to 8 bits with a fixed ordered pattern or left to band.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Gradient {
    #[serde(default)]
    pub kind: GradientKind,
    /// Degrees for linear: 0 runs left to right, 90 runs bottom to top.
    #[serde(default)]
    pub angle: f64,
    pub stops: Vec<Stop>,
    #[serde(default)]
    pub dither: bool,
}

impl Gradient {
    /// Linear-light channels at position `t`, clamped at the ends.
    ///
    /// This is what a renderer wants: the caller quantises, so it can dither first.
    pub fn sample_linear(&self, t: f64) -> [f64; 3] {
        let t = t.clamp(0.0, 1.0);
        let Some(first) = self.stops.first() else {
            return [0.0; 3];
        };
        let mix_into = |a: Rgba, b: Rgba, u: f64| -> [f64; 3] {
            let ch = |x: u8, y: u8| {
                let x = Rgba::linear_channel(x);
                let y = Rgba::linear_channel(y);
                x + (y - x) * u
            };
            [ch(a.r, b.r), ch(a.g, b.g), ch(a.b, b.b)]
        };
        if t <= first.at {
            return to_linear(first.color);
        }
        let mut prev = first;
        for stop in self.stops.iter().skip(1) {
            if t <= stop.at {
                let span = (stop.at - prev.at).max(f64::EPSILON);
                return mix_into(prev.color, stop.color, (t - prev.at) / span);
            }
            prev = stop;
        }
        to_linear(prev.color)
    }

    /// The colour at position `t`, quantised to 8 bits and clamped at the ends.
    pub fn sample(&self, t: f64) -> Rgba {
        let lin = self.sample_linear(t);
        Rgba::from_linear(lin[0], lin[1], lin[2])
    }

    /// Where a pixel sits along the gradient, for a box in canvas pixels.
    pub fn project(&self, px: f64, py: f64, bx: f64, by: f64, bw: f64, bh: f64) -> f64 {
        self.projector(bx, by, bw, bh).at(px, py)
    }

    /// A projector precomputes the trigonometry once, so a per-pixel loop does not call
    /// `cos` two million times.
    pub fn projector(&self, bx: f64, by: f64, bw: f64, bh: f64) -> Projector {
        let a = self.angle.to_radians();
        Projector {
            cx: bx + bw / 2.0,
            cy: by + bh / 2.0,
            dx: a.cos(),
            dy: -a.sin(),
            span: (bw * a.cos()).abs() + (bh * a.sin()).abs(),
            radius: bw.min(bh) / 2.0,
            radial: matches!(self.kind, GradientKind::Radial),
        }
    }

    pub fn validate(&self) -> Option<String> {
        if self.stops.len() < 2 {
            return Some("a gradient needs at least two stops".into());
        }
        let mut last = f64::NEG_INFINITY;
        for s in &self.stops {
            if !(0.0..=1.0).contains(&s.at) {
                return Some("stop positions are between 0 and 1".into());
            }
            if s.at < last {
                return Some("stops must be ordered by position".into());
            }
            last = s.at;
        }
        None
    }
}

/// Precomputed geometry for sampling a gradient across a box.
#[derive(Debug, Clone, Copy)]
pub struct Projector {
    cx: f64,
    cy: f64,
    dx: f64,
    dy: f64,
    span: f64,
    radius: f64,
    radial: bool,
}

impl Projector {
    pub fn at(&self, px: f64, py: f64) -> f64 {
        if self.radial {
            if self.radius <= 0.0 {
                return 0.0;
            }
            return (((px - self.cx).powi(2) + (py - self.cy).powi(2)).sqrt() / self.radius)
                .clamp(0.0, 1.0);
        }
        if self.span <= f64::EPSILON {
            return 0.0;
        }
        (0.5 + ((px - self.cx) * self.dx + (py - self.cy) * self.dy) / self.span).clamp(0.0, 1.0)
    }
}

fn to_linear(c: Rgba) -> [f64; 3] {
    [
        Rgba::linear_channel(c.r),
        Rgba::linear_channel(c.g),
        Rgba::linear_channel(c.b),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    Left,
    #[default]
    Center,
    Right,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Text {
    /// Lines are separated by `\n`; there is no automatic wrapping yet.
    pub string: String,
    pub size: f64,
    #[serde(default = "default_font")]
    pub font: String,
    pub color: Rgba,
    #[serde(default = "default_weight")]
    pub weight: u16,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub tracking: f64,
    #[serde(default)]
    pub align: Align,
    /// Multiple of the type size between baselines. Ignored for a single line.
    #[serde(default = "default_line_height")]
    pub line_height: f64,
    /// Optional staggered entrance, applied per glyph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anim: Option<TextAnim>,
}

/// What a staggered entrance does to each unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnimKind {
    /// Fade in place.
    Reveal,
    /// Fade while rising into place.
    Rise,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AnimUnit {
    #[default]
    Char,
    Word,
    Line,
}

/// A staggered entrance, evaluated per glyph against the layer's own clock.
///
/// `stagger` is the delay between units and `duration` the length of one unit's
/// entrance, both in seconds of local time, so the animation travels with the layer
/// rather than with the comp.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextAnim {
    #[serde(rename = "type")]
    pub kind: AnimKind,
    #[serde(default = "default_anim_stagger")]
    pub stagger: f64,
    #[serde(default = "default_anim_duration")]
    pub duration: f64,
    #[serde(default)]
    pub unit: AnimUnit,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ease: Option<String>,
    /// Pixels travelled by `rise`; ignored by `reveal`.
    #[serde(default = "default_rise_distance")]
    pub distance: f64,
}

fn default_anim_stagger() -> f64 {
    0.035
}

fn default_anim_duration() -> f64 {
    0.45
}

fn default_rise_distance() -> f64 {
    24.0
}

impl TextAnim {
    fn unit_index(&self, char_index: usize, word_index: usize, line_index: usize) -> usize {
        match self.unit {
            AnimUnit::Char => char_index,
            AnimUnit::Word => word_index,
            AnimUnit::Line => line_index,
        }
    }

    /// `(alpha multiplier, vertical offset in pixels)` for one unit at local time `t`.
    pub fn at(&self, t: f64, unit_index: usize) -> (f64, f64) {
        let start = unit_index as f64 * self.stagger.max(0.0);
        let u = if self.duration <= 0.0 {
            1.0
        } else {
            (t - start) / self.duration
        };
        let e = crate::prop::Ease::parse(self.ease.as_deref()).apply(u);
        match self.kind {
            AnimKind::Reveal => (e, 0.0),
            AnimKind::Rise => (e, -self.distance * (1.0 - e)),
        }
    }

    /// `(alpha, dy)` for a specific glyph.
    pub fn at_glyph(
        &self,
        t: f64,
        char_index: usize,
        word_index: usize,
        line_index: usize,
    ) -> (f64, f64) {
        self.at(t, self.unit_index(char_index, word_index, line_index))
    }

    /// How long the whole entrance takes for `units` units.
    pub fn total(&self, units: usize) -> f64 {
        units.saturating_sub(1) as f64 * self.stagger.max(0.0) + self.duration.max(0.0)
    }
}

fn default_line_height() -> f64 {
    1.2
}

fn default_font() -> String {
    kinema_text::DEFAULT_FAMILY.to_string()
}

fn default_weight() -> u16 {
    400
}

impl Text {
    pub fn lines(&self) -> Vec<&str> {
        self.string.split('\n').collect()
    }

    /// Advance width of one line in pixels, from the real face's glyph advances plus
    /// tracking.
    pub fn line_width(&self, line: &str) -> f64 {
        kinema_text::line_width(
            &self.font,
            self.weight,
            self.italic,
            line,
            self.size as f32,
            self.tracking as f32,
        ) as f64
    }

    /// `[width, height]` of the block. Height is the face's own ascent plus descent for one
    /// line, plus the leading between further lines — so a box is as tall as its ink, not as
    /// tall as the em square.
    pub fn metrics(&self) -> [f64; 2] {
        let (w, h) = kinema_text::measure(
            &self.string,
            &self.font,
            self.weight,
            self.italic,
            self.size as f32,
            self.tracking as f32,
            self.line_height as f32,
        );
        [w as f64, h as f64]
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Matte {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alpha: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub luma: Option<String>,
}

/// A modifier is `{"type": "glow", ...}`: the type plus free parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Modifier {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(flatten)]
    pub params: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    pub id: String,
    pub kind: String,
    pub path: String,
    /// blake3 of the file, so a swapped asset cannot hide behind the same path.
    #[serde(default)]
    pub digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub licence: Option<Licence>,
}

/// Licence is data, never a guess. `unknown` blocks a release render.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Licence {
    pub spdx: String,
    pub source_url: String,
    pub author: String,
    pub retrieved_at: String,
    #[serde(default)]
    pub attribution_required: bool,
    #[serde(default)]
    pub commercial_ok: bool,
    #[serde(default)]
    pub derivatives_ok: bool,
    #[serde(default)]
    pub share_alike: bool,
    #[serde(default)]
    pub model_release: bool,
}

impl Licence {
    /// The exact line that must appear in credits, when one is required.
    pub fn credit_line(&self, asset: &Asset) -> Option<String> {
        if !self.attribution_required {
            return None;
        }
        Some(format!(
            "\"{}\" by {} — {} ({})",
            asset.id, self.author, self.spdx, self.source_url
        ))
    }

    pub fn is_known(&self) -> bool {
        !self.spdx.eq_ignore_ascii_case("unknown") && !self.spdx.trim().is_empty()
    }
}

/// A layer resolved at one instant.
#[derive(Debug, Clone, Serialize)]
pub struct EvaluatedLayer {
    pub id: String,
    pub kind: &'static str,
    pub visible: bool,
    pub opacity: f64,
    pub position: [f64; 2],
    pub scale: [f64; 2],
    pub rotation: f64,
    /// `[x, y, w, h]`, top-left origin.
    pub bbox: [f64; 4],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
}

impl Doc {
    pub fn empty() -> Self {
        Doc {
            schema: SCHEMA_TAG.to_string(),
            version: 0,
            fps: Fps::new(30, 1),
            comps: BTreeMap::new(),
            assets: BTreeMap::new(),
            look: Some(Look {
                accent: Rgba::opaque(0x4c, 0x8f, 0xff),
                scheme: Scheme::Dark,
                scale: Scale::default(),
                pairing: Some("editorial".into()),
            }),
        }
    }

    /// The project `kin new` writes: one comp, three layers, nothing exotic.
    pub fn starter() -> Self {
        let mut comps = BTreeMap::new();
        let layers = vec![
            Layer {
                id: "bg".into(),
                kind: LayerKind::Solid,
                start: 0.0,
                duration: 5.0,
                transform: Transform::default(),
                opacity: Prop::number(1.0),
                blend: Blend::Normal,
                solid: Some(Solid {
                    color: Rgba::opaque(0x0b, 0x0d, 0x12),
                    gradient: None,
                }),
                text: None,
                shape: None,
                mattes: None,
                modifiers: vec![],
                source: None,
            },
            Layer {
                id: "accent".into(),
                kind: LayerKind::Shape,
                // Entrances are staggered by three frames so the eye can follow.
                start: 0.1,
                duration: 4.9,
                transform: Transform {
                    // The rule rides with the title, 8 px below its baseline.
                    position: Some(Prop::keyframed(
                        vec![
                            crate::Keyframe {
                                t: 0.0,
                                v: serde_json::json!([960.0, 760.0]),
                            },
                            crate::Keyframe {
                                t: 0.6,
                                v: serde_json::json!([960.0, 660.0]),
                            },
                        ],
                        Some("bezier(.2,.9,.1,1)".into()),
                    )),
                    scale: None,
                    rotation: None,
                    anchor: None,
                },
                opacity: Prop::number(1.0),
                blend: Blend::Normal,
                solid: None,
                text: None,
                shape: Some(Shape {
                    kind: ShapeKind::Rect,
                    size: [420.0, 8.0],
                    color: Rgba::opaque(0x4c, 0x8f, 0xff),
                    radius: 4.0,
                    gradient: None,
                }),
                mattes: None,
                modifiers: vec![],
                source: None,
            },
            Layer {
                id: "title".into(),
                kind: LayerKind::Text,
                start: 0.2,
                duration: 4.8,
                transform: Transform {
                    position: Some(Prop::keyframed(
                        vec![
                            crate::Keyframe {
                                t: 0.0,
                                v: serde_json::json!([960.0, 700.0]),
                            },
                            crate::Keyframe {
                                t: 0.6,
                                v: serde_json::json!([960.0, 600.0]),
                            },
                        ],
                        Some("bezier(.2,.9,.1,1)".into()),
                    )),
                    scale: None,
                    rotation: None,
                    anchor: None,
                },
                opacity: Prop::number(1.0),
                blend: Blend::Normal,
                solid: None,
                text: Some(Text {
                    string: "HELLO KINEMA".into(),
                    size: 96.0,
                    font: kinema_text::DEFAULT_FAMILY.into(),
                    color: Rgba::opaque(0xf4, 0xf6, 0xfa),
                    weight: 700,
                    italic: false,
                    tracking: 0.0,
                    align: Align::Center,
                    line_height: default_line_height(),
                    // The letters resolve left to right, which is the one entrance that
                    // never looks like a template.
                    anim: Some(TextAnim {
                        kind: AnimKind::Reveal,
                        stagger: 0.03,
                        duration: 0.4,
                        unit: AnimUnit::Char,
                        ease: Some("bezier(.2,.9,.1,1)".into()),
                        distance: 24.0,
                    }),
                }),
                shape: None,
                mattes: None,
                modifiers: vec![],
                source: None,
            },
        ];
        comps.insert(
            "main".into(),
            Comp {
                size: [1920, 1080],
                duration: 5.0,
                // The title crosses 100 px in 18 frames, so it needs a shutter to not
                // strobe. 180 degrees is the film default.
                shutter: 180.0,
                samples: default_samples(),
                layers,
            },
        );
        Doc {
            comps,
            ..Doc::empty()
        }
    }

    pub fn parse(src: &str) -> Result<Self, DocError> {
        Ok(serde_json::from_str(src)?)
    }

    pub fn to_pretty(&self) -> Result<String, DocError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn comp(&self, id: &str) -> Result<&Comp, DocError> {
        self.comps.get(id).ok_or_else(|| DocError::NotFound {
            path: format!("comp:{id}"),
            hint: format!(
                "known comps: {}",
                self.comps.keys().cloned().collect::<Vec<_>>().join(", ")
            ),
        })
    }

    pub fn layer(&self, comp: &str, id: &str) -> Result<&Layer, DocError> {
        self.comp(comp)?
            .layers
            .iter()
            .find(|l| l.id == id)
            .ok_or_else(|| DocError::NotFound {
                path: format!("comp:{comp}/layer:{id}"),
                hint: "run `kin probe` to list layer ids".to_string(),
            })
    }

    pub fn frame_count(&self, comp: &str) -> Result<u32, DocError> {
        Ok(self.fps.duration_frames(self.comp(comp)?.duration_ticks()))
    }

    /// Read any value by kinema path, used by `link` properties and by `probe`.
    pub fn value_at(&self, path: &str) -> Result<Value, DocError> {
        let ptr = pointer_for(self, path)?;
        let v = serde_json::to_value(self)?;
        v.pointer(&ptr).cloned().ok_or_else(|| DocError::NotFound {
            path: path.to_string(),
            hint: "check the path shape: comp:main/layer:title/transform.position".to_string(),
        })
    }

    /// Resolve one layer at a time in seconds.
    pub fn eval_layer(
        &self,
        comp: &str,
        layer: &Layer,
        t: f64,
    ) -> Result<EvaluatedLayer, EvalError> {
        let c = self.comp(comp).map_err(|_| EvalError::Empty)?;
        let start = layer.start;
        let end = layer.start + layer.duration;
        let visible = t >= start && t < end;
        let opacity = layer
            .opacity
            .eval_f64(t, self)
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);
        let position = match &layer.transform.position {
            Some(p) => p.eval_vec2(t, self)?,
            None => [c.size[0] as f64 / 2.0, c.size[1] as f64 / 2.0],
        };
        let scale = match &layer.transform.scale {
            Some(p) => p.eval_vec2(t, self)?,
            None => [1.0, 1.0],
        };
        let rotation = match &layer.transform.rotation {
            Some(p) => p.eval_f64(t, self)?,
            None => 0.0,
        };
        let mut size = [0.0f64, 0.0f64];
        let mut text = None;
        let mut font = None;
        match layer.kind {
            LayerKind::Text => {
                if let Some(txt) = &layer.text {
                    size = txt.metrics();
                    text = Some(txt.string.clone());
                    font = Some(txt.font.clone());
                }
            }
            LayerKind::Shape => {
                if let Some(s) = &layer.shape {
                    size = s.size;
                }
            }
            LayerKind::Solid | LayerKind::Adjustment => {
                // Both cover the frame unless a transform says otherwise: an adjustment with
                // no size would grade nothing.
                size = [c.size[0] as f64, c.size[1] as f64];
            }
            LayerKind::Comp => {
                // A nested comp takes its own frame size, so its bbox is that size.
                let inner = layer
                    .source
                    .as_ref()
                    .and_then(|s| self.comps.get(s))
                    .unwrap_or(c);
                size = [inner.size[0] as f64, inner.size[1] as f64];
            }
            _ => {}
        }
        let w = size[0] * scale[0].abs();
        let h = size[1] * scale[1].abs();
        Ok(EvaluatedLayer {
            id: layer.id.clone(),
            kind: layer.kind.as_str(),
            visible,
            opacity,
            position,
            scale,
            rotation,
            bbox: [position[0] - w / 2.0, position[1] - h / 2.0, w, h],
            text,
            font,
        })
    }

    /// Every layer in paint order at `t`.
    pub fn eval_comp(&self, comp: &str, t: f64) -> Result<Vec<EvaluatedLayer>, DocError> {
        let c = self.comp(comp)?;
        let mut out = Vec::with_capacity(c.layers.len());
        for l in &c.layers {
            out.push(self.eval_layer(comp, l, t).map_err(DocError::Eval)?);
        }
        Ok(out)
    }

    pub fn frame_time(&self, frame: u32) -> f64 {
        kinema_time::ticks_to_seconds(self.fps.frame_to_ticks(frame))
    }

    /// Structural problems that would make a render lie about itself.
    pub fn validate(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.fps.0.num <= 0 || self.fps.0.den <= 0 {
            out.push("fps must be positive".into());
        }
        if self.comps.is_empty() {
            out.push("document has no comps".into());
        }
        for (id, c) in &self.comps {
            if c.size[0] == 0 || c.size[1] == 0 {
                out.push(format!("comp `{id}` has a zero dimension"));
            }
            if c.duration <= 0.0 {
                out.push(format!("comp `{id}` has no duration"));
            }
            if let Some(look) = &self.look {
                if look.scale.base <= 0.0 {
                    out.push("look scale base must be positive".into());
                }
                if look.scale.ratio <= 1.0 {
                    out.push(
                        "look scale ratio must be above 1, or the scale runs backwards".into(),
                    );
                }
                if !(2..=16).contains(&look.scale.steps) {
                    out.push("look scale steps must be between 2 and 16".into());
                }
            }
            let mut seen = std::collections::BTreeSet::new();
            for l in &c.layers {
                if !seen.insert(l.id.clone()) {
                    out.push(format!("comp `{id}` has duplicate layer id `{}`", l.id));
                }
                if l.duration <= 0.0 {
                    out.push(format!("layer `{}` has no duration", l.id));
                }
                if l.kind == LayerKind::Text && l.text.is_none() {
                    out.push(format!("text layer `{}` has no text block", l.id));
                }
                if l.kind == LayerKind::Shape && l.shape.is_none() {
                    out.push(format!("shape layer `{}` has no shape block", l.id));
                }
                if l.kind == LayerKind::Comp {
                    match &l.source {
                        None => out.push(format!("comp layer `{}` has no source comp", l.id)),
                        Some(src) if src == id => {
                            out.push(format!("comp layer `{}` contains its own comp", l.id))
                        }
                        Some(src) if !self.comps.contains_key(src) => out.push(format!(
                            "comp layer `{}` points at `{src}`, which is not a comp",
                            l.id
                        )),
                        Some(_) => {}
                    }
                }
                if let Some(g) = l.solid.as_ref().and_then(|s| s.gradient.as_ref()) {
                    if let Some(e) = g.validate() {
                        out.push(format!("layer `{}` solid gradient: {e}", l.id));
                    }
                }
                if let Some(g) = l.shape.as_ref().and_then(|s| s.gradient.as_ref()) {
                    if let Some(e) = g.validate() {
                        out.push(format!("layer `{}` shape gradient: {e}", l.id));
                    }
                }
                if let Some(m) = &l.mattes {
                    if m.alpha.is_some() && m.luma.is_some() {
                        out.push(format!(
                            "layer `{}` declares both an alpha and a luma matte",
                            l.id
                        ));
                    }
                    for src in [m.alpha.as_ref(), m.luma.as_ref()].into_iter().flatten() {
                        if src == &l.id {
                            out.push(format!("layer `{}` mattes itself", l.id));
                        } else if !c.layers.iter().any(|o| &o.id == src) {
                            out.push(format!(
                                "layer `{}` mattes `{src}`, which is not in comp `{id}`",
                                l.id
                            ));
                        }
                    }
                }
            }
        }
        out
    }

    pub fn ticks_per_second(&self) -> i64 {
        TICKS_PER_SECOND
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_comp_layer_needs_a_real_source() {
        let mut doc = Doc::starter();
        let c = doc.comps.get_mut("main").unwrap();
        let mut l = c.layers[0].clone();
        l.id = "nested".into();
        l.kind = LayerKind::Comp;
        l.source = Some("nope".into());
        c.layers.push(l);
        let problems = doc.validate();
        assert!(
            problems.iter().any(|p| p.contains("nope")),
            "a comp layer pointing nowhere must be reported: {problems:?}"
        );
    }

    #[test]
    fn starter_round_trips() {
        let doc = Doc::starter();
        let json = doc.to_pretty().unwrap();
        let back = Doc::parse(&json).unwrap();
        assert_eq!(back.comps["main"].layers.len(), 3);
        assert!(back.validate().is_empty(), "{:?}", back.validate());
    }

    #[test]
    fn text_metrics_scale_with_size() {
        let mut t = Text {
            string: "HELLO".into(),
            size: 70.0,
            font: "Lato".into(),
            color: Rgba::WHITE,
            weight: 400,
            italic: false,
            tracking: 0.0,
            align: Align::Center,
            line_height: 1.2,
            anim: None,
        };
        let small = t.metrics();
        t.size = 140.0;
        let large = t.metrics();
        assert!((large[0] / small[0] - 2.0).abs() < 1e-3);
        assert!((large[1] / small[1] - 2.0).abs() < 1e-3);
    }

    #[test]
    fn multiline_metrics_stack_lines() {
        let mut t = Text {
            string: "ONE\nTWO TWO".into(),
            size: 50.0,
            font: "Lato".into(),
            color: Rgba::WHITE,
            weight: 400,
            italic: false,
            tracking: 0.0,
            align: Align::Left,
            line_height: 1.5,
            anim: None,
        };
        let two = t.metrics();
        assert_eq!(t.lines().len(), 2);
        // The widest line sets the block width.
        assert!((two[0] - t.line_width("TWO TWO")).abs() < 1e-3);
        // The second line sits one leading (50 x 1.5) below the first.
        t.string = "ONE".into();
        let one = t.metrics();
        assert!(
            (two[1] - one[1] - 75.0).abs() < 1e-3,
            "{} {}",
            two[1],
            one[1]
        );
        assert!(
            (one[1] - t.size * 1.2).abs() < t.size * 0.25,
            "a line is about its em box tall"
        );
    }

    fn bw_gradient() -> Gradient {
        Gradient {
            kind: GradientKind::Linear,
            angle: 0.0,
            stops: vec![
                Stop {
                    at: 0.0,
                    color: Rgba::BLACK,
                },
                Stop {
                    at: 1.0,
                    color: Rgba::WHITE,
                },
            ],
            dither: false,
        }
    }

    #[test]
    fn a_gradient_mixes_in_linear_light_and_clamps() {
        let g = bw_gradient();
        assert_eq!(g.sample(-1.0), Rgba::BLACK);
        assert_eq!(g.sample(2.0), Rgba::WHITE);
        let mid = g.sample(0.5);
        // Half way in linear light is sRGB ~188, not 128: mixing gamma-encoded values is the
        // mistake that makes black-to-white go grey.
        assert!(mid.r > 170 && mid.r < 200, "mid was {}", mid.r);
        let lin = g.sample_linear(0.5);
        assert!(
            (lin[0] - 0.5).abs() < 0.01,
            "linear mid is 0.5, got {}",
            lin[0]
        );
    }

    #[test]
    fn gradients_reject_stops_that_cannot_be_drawn() {
        let mut g = bw_gradient();
        g.stops.truncate(1);
        assert!(g.validate().is_some());
        let mut g = bw_gradient();
        g.stops[0].at = 0.6;
        g.stops[1].at = 0.2;
        assert!(g.validate().unwrap().contains("ordered"));
        let mut g = bw_gradient();
        g.stops[1].at = 1.4;
        assert!(g.validate().unwrap().contains("between 0 and 1"));
        assert!(bw_gradient().validate().is_none());
    }

    #[test]
    fn a_gradient_projects_across_its_box_in_the_stated_direction() {
        let g = bw_gradient();
        let left = g.project(0.0, 50.0, 0.0, 0.0, 100.0, 100.0);
        let right = g.project(100.0, 50.0, 0.0, 0.0, 100.0, 100.0);
        assert!(left < 0.05 && right > 0.95, "{left} {right}");
        // 90 degrees runs upwards, and y grows downwards, so the bottom is the start.
        let up = Gradient {
            angle: 90.0,
            ..g.clone()
        };
        assert!(up.project(50.0, 100.0, 0.0, 0.0, 100.0, 100.0) < 0.05);
        assert!(up.project(50.0, 0.0, 0.0, 0.0, 100.0, 100.0) > 0.95);
    }

    #[test]
    fn visibility_follows_the_window() {
        let doc = Doc::starter();
        let layer = doc.layer("main", "title").unwrap();
        // The starter staggers entrances, so the window is [start, start + duration).
        assert!(
            !doc.eval_layer("main", layer, layer.start - 0.01)
                .unwrap()
                .visible
        );
        assert!(doc.eval_layer("main", layer, layer.start).unwrap().visible);
        assert!(
            !doc.eval_layer("main", layer, layer.start + layer.duration)
                .unwrap()
                .visible
        );
    }

    #[test]
    fn keyframed_position_animates() {
        let doc = Doc::starter();
        let layer = doc.layer("main", "title").unwrap();
        let a = doc.eval_layer("main", layer, 0.0).unwrap().position[1];
        let b = doc.eval_layer("main", layer, 0.6).unwrap().position[1];
        assert!(b < a, "title should rise: {a} -> {b}");
    }

    #[test]
    fn duplicate_layer_ids_are_reported() {
        let mut doc = Doc::starter();
        let mut clone = doc.comps["main"].layers[2].clone();
        clone.id = "title".into();
        doc.comps.get_mut("main").unwrap().layers.push(clone);
        assert!(doc
            .validate()
            .iter()
            .any(|m| m.contains("duplicate layer id")));
    }

    #[test]
    fn licence_credit_lines() {
        let asset = Asset {
            id: "whoosh".into(),
            kind: "sfx".into(),
            path: "assets/whoosh.wav".into(),
            digest: "00".into(),
            licence: None,
        };
        let lic = Licence {
            spdx: "CC0-1.0".into(),
            source_url: "https://example.test/a".into(),
            author: "Someone".into(),
            retrieved_at: "2026-11-12".into(),
            attribution_required: false,
            commercial_ok: true,
            derivatives_ok: true,
            share_alike: false,
            model_release: false,
        };
        assert!(lic.credit_line(&asset).is_none());
        assert!(lic.is_known());
        let unknown = Licence {
            spdx: "unknown".into(),
            ..lic
        };
        assert!(!unknown.is_known());
    }
}
