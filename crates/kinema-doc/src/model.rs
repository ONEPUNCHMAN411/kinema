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
    /// Palette, type scale and motion curves, once `taste` lands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub look: Option<Value>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Text {
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
}

fn default_font() -> String {
    "Inter".to_string()
}

fn default_weight() -> u16 {
    400
}

impl Text {
    /// Glyph box is 5 x 7 units, the bitmap face in `kinema-raster`.
    pub const GLYPH_W: f64 = 5.0;
    pub const GLYPH_H: f64 = 7.0;
    pub const ADVANCE: f64 = 6.0;

    pub fn unit(&self) -> f64 {
        self.size / Self::GLYPH_H
    }

    /// Advance width of one character in pixels.
    pub fn advance(&self) -> f64 {
        Self::ADVANCE * self.unit() + self.tracking
    }

    pub fn metrics(&self) -> [f64; 2] {
        let n = self.string.chars().count().max(1) as f64;
        let unit = self.unit();
        let width = n * (Self::ADVANCE * unit + self.tracking)
            - self.tracking.max(0.0)
            - (Self::ADVANCE - Self::GLYPH_W) * unit;
        [width.max(0.0), self.size]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
            look: None,
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
                    font: "Inter".into(),
                    color: Rgba::opaque(0xf4, 0xf6, 0xfa),
                    weight: 700,
                    italic: false,
                    tracking: 0.0,
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
            LayerKind::Solid => {
                size = [c.size[0] as f64, c.size[1] as f64];
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
            font: "Inter".into(),
            color: Rgba::WHITE,
            weight: 400,
            italic: false,
            tracking: 0.0,
        };
        let small = t.metrics();
        t.size = 140.0;
        let large = t.metrics();
        assert!((large[0] / small[0] - 2.0).abs() < 1e-9);
        assert!((large[1] - 140.0).abs() < 1e-9);
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
