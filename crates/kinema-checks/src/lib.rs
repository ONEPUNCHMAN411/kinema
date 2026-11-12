//! The craft detectors.
//!
//! Each one is a measurement with a threshold and a remedy. The remedy is a real `Op`,
//! so an agent can apply it without translating prose into an edit.

use kinema_doc::{Doc, LayerKind, Rgba};
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct Defect {
    pub code: &'static str,
    pub severity: &'static str,
    pub comp: String,
    pub layer: Option<String>,
    pub frame: Option<u32>,
    pub evidence: Value,
    pub hint: String,
    pub fix_op: Option<Value>,
}

impl Defect {
    pub fn to_json(&self) -> Value {
        json!({
            "code": self.code,
            "severity": self.severity,
            "at": { "comp": self.comp, "frame": self.frame },
            "layer": self.layer,
            "evidence": self.evidence,
            "hint": self.hint,
            "fix_op": self.fix_op,
        })
    }
}

/// Families shipped in this build, taken from the text crate so the two cannot drift.
/// Anything else is a machine fallback, which is a tell.
pub fn bundled_fonts() -> Vec<&'static str> {
    kinema_text::families()
}

/// Weights a family actually ships. Anything between them would be synthesised.
pub fn bundled_weights(family: &str) -> Vec<u16> {
    kinema_text::weights(family)
}

fn set_prop(path: String, value: Value, reason: &str) -> Value {
    json!({ "op": "set-prop", "path": path, "value": value, "reason": reason })
}

/// Run every check over one comp.
pub fn run(doc: &Doc, comp: &str, frame: Option<u32>) -> Vec<Defect> {
    let mut out = Vec::new();
    let Ok(c) = doc.comp(comp) else {
        return out;
    };
    let fps = doc.fps;
    let frame_of = |t: f64| fps.ticks_to_frame(kinema_time::seconds_to_ticks(t));

    type_fake(doc, comp, &mut out);
    type_scale(doc, comp, &mut out);
    read(doc, comp, &mut out);
    chroma(doc, comp, &mut out);
    palette(doc, comp, &mut out);
    look_drift(doc, comp, &mut out);
    banding(doc, comp, &mut out);
    ease_flat(doc, comp, &mut out);
    hold_none(doc, comp, &mut out);
    shutter(doc, comp, &mut out);
    stagger(doc, comp, frame, &mut out, frame_of);
    matte_cover(doc, comp, &mut out);
    empty_comp(doc, comp, &mut out);
    modifier(doc, comp, &mut out);
    let _ = c;
    out
}

/// Modifiers the renderer knows. Kept next to the detector rather than the raster so a
/// check can name the problem before a render has to fail on it.
const MODIFIERS: [&str; 6] = [
    "glow",
    "blur",
    "drop-shadow",
    "exposure",
    "contrast",
    "saturate",
];

/// An unknown modifier is caught here, with the fix that removes just it: the renderer
/// refuses the frame, but a check should say so before a render is spent.
fn modifier(doc: &Doc, comp: &str, out: &mut Vec<Defect>) {
    let Ok(c) = doc.comp(comp) else { return };
    for layer in &c.layers {
        let unknown: Vec<_> = layer
            .modifiers
            .iter()
            .filter(|m| !MODIFIERS.contains(&m.kind.as_str()))
            .map(|m| m.kind.clone())
            .collect();
        if unknown.is_empty() {
            continue;
        }
        let kept: Vec<_> = layer
            .modifiers
            .iter()
            .filter(|m| MODIFIERS.contains(&m.kind.as_str()))
            .cloned()
            .collect();
        out.push(Defect {
            code: "MODIFIER",
            severity: "error",
            comp: comp.to_string(),
            layer: Some(layer.id.clone()),
            frame: None,
            evidence: json!({"unknown": unknown, "known": MODIFIERS}),
            hint: format!(
                "layer `{}` asks for {}; this build applies {}",
                layer.id,
                unknown.join(", "),
                MODIFIERS.join(", ")
            ),
            fix_op: Some(json!({
                "op": "set-modifier",
                "comp": comp,
                "layer": layer.id,
                "modifiers": kept,
                "reason": "drop a modifier this build cannot draw"
            })),
        });
    }
}

fn overlap(a: [f64; 4], b: [f64; 4]) -> f64 {
    let w = (a[0] + a[2]).min(b[0] + b[2]) - a[0].max(b[0]);
    let h = (a[1] + a[3]).min(b[1] + b[3]) - a[1].max(b[1]);
    if w <= 0.0 || h <= 0.0 {
        0.0
    } else {
        w * h
    }
}

/// A layer cut by a matte that never touches it is a layer you cannot see, and nothing in
/// the render says so. This is advisory: where the matte should sit is a directing call.
fn matte_cover(doc: &Doc, comp: &str, out: &mut Vec<Defect>) {
    let Ok(c) = doc.comp(comp) else { return };
    for layer in &c.layers {
        let Some(m) = &layer.mattes else { continue };
        let Some(source_id) = m.alpha.as_ref().or(m.luma.as_ref()) else {
            continue;
        };
        let Some(src) = c.layers.iter().find(|l| &l.id == source_id) else {
            continue;
        };
        let start = layer.start.max(src.start);
        let end = (layer.start + layer.duration).min(src.start + src.duration);
        if end <= start {
            continue;
        }
        let mut best = 0.0f64;
        let mut checked = Vec::new();
        let mut boxes = Vec::new();
        for i in 0..5 {
            let t = start + (i as f64 + 0.5) / 5.0 * (end - start);
            let (Ok(a), Ok(b)) = (doc.eval_layer(comp, layer, t), doc.eval_layer(comp, src, t))
            else {
                continue;
            };
            let area = overlap(a.bbox, b.bbox);
            best = best.max(area);
            checked.push(kinema_time::timecode(
                kinema_time::seconds_to_ticks(t),
                doc.fps,
            ));
            boxes.push(json!({"layer": a.bbox, "matte": b.bbox}));
        }
        if best > 0.0 {
            continue;
        }
        out.push(Defect {
            code: "MATTE-COVER",
            severity: "warn",
            comp: comp.to_string(),
            layer: Some(layer.id.clone()),
            frame: None,
            evidence: json!({
                "source": source_id,
                "overlap_area": best,
                "checked": checked,
                "bboxes": boxes,
                "reason": "a matte that never reaches the layer hides it",
            }),
            hint: format!(
                "`{}` is cut by `{source_id}` and the two never overlap; move the matte or drop it",
                layer.id
            ),
            fix_op: None,
        });
    }
}

/// A nested comp with nothing visible in the window its layer covers renders as nothing.
fn empty_comp(doc: &Doc, comp: &str, out: &mut Vec<Defect>) {
    let Ok(c) = doc.comp(comp) else { return };
    for layer in &c.layers {
        if layer.kind != LayerKind::Comp {
            continue;
        }
        let Some(source) = &layer.source else {
            continue;
        };
        let Some(nested) = doc.comps.get(source) else {
            continue;
        };
        let start = layer.start.max(0.0);
        let end = layer.start + layer.duration;
        if end <= start {
            continue;
        }
        let mut live_layers = 0;
        for i in 0..5 {
            let t = layer.start + (i as f64 + 0.5) / 5.0 * layer.duration;
            let inner = t - layer.start;
            if inner >= nested.duration {
                continue;
            }
            let live = nested
                .layers
                .iter()
                .filter(|l| inner >= l.start && inner < l.start + l.duration)
                .count();
            live_layers = live_layers.max(live);
        }
        if live_layers > 0 {
            continue;
        }
        out.push(Defect {
            code: "EMPTY-COMP",
            severity: "warn",
            comp: comp.to_string(),
            layer: Some(layer.id.clone()),
            frame: None,
            evidence: json!({
                "source": source,
                "nested_layers": nested.layers.len(),
                "nested_duration": nested.duration,
                "layer_duration": layer.duration,
            }),
            hint: format!(
                "layer `{}` draws comp `{source}`, which has nothing live while it shows",
                layer.id
            ),
            fix_op: None,
        });
    }
}

/// Properties a detector can reason about, with the path segment that addresses them.
fn animated(layer: &kinema_doc::Layer) -> Vec<(&'static str, &kinema_doc::Prop)> {
    let mut v: Vec<(&'static str, &kinema_doc::Prop)> = Vec::new();
    if let Some(p) = &layer.transform.position {
        v.push(("transform.position", p));
    }
    if let Some(p) = &layer.transform.scale {
        v.push(("transform.scale", p));
    }
    if let Some(p) = &layer.transform.rotation {
        v.push(("transform.rotation", p));
    }
    v.push(("opacity", &layer.opacity));
    v.retain(|(_, p)| p.kf.as_ref().map(|k| k.len() >= 2).unwrap_or(false));
    v
}

/// How far a property travels between two keyframe values, in its own units.
fn travel(field: &str, a: &Value, b: &Value) -> f64 {
    let pair = |v: &Value| -> Option<[f64; 2]> {
        let arr = v.as_array()?;
        Some([arr.first()?.as_f64()?, arr.get(1)?.as_f64()?])
    };
    match (pair(a), pair(b)) {
        (Some(x), Some(y)) if field == "transform.position" => {
            ((y[0] - x[0]).powi(2) + (y[1] - x[1]).powi(2)).sqrt()
        }
        (Some(x), Some(y)) => (y[0] - x[0]).abs().max((y[1] - x[1]).abs()),
        _ => match (a.as_f64(), b.as_f64()) {
            (Some(x), Some(y)) => (y - x).abs(),
            _ => 0.0,
        },
    }
}

/// A curve that accelerates and decelerates by the same amount is the default
/// interpolation of every slide template. Real motion is biased: quick out, slow in.
fn is_symmetric(ease: &kinema_doc::Ease) -> bool {
    let f = |u: f64| ease.apply(u);
    (f(0.5) - 0.5).abs() < 0.02 && (f(0.25) + f(0.75) - 1.0).abs() < 0.04
}

fn ease_flat(doc: &Doc, comp: &str, out: &mut Vec<Defect>) {
    let Ok(c) = doc.comp(comp) else { return };
    for layer in &c.layers {
        for (field, prop) in animated(layer) {
            let Some(kfs) = prop.kf.as_ref() else {
                continue;
            };
            let ease = kinema_doc::Ease::parse(prop.ease.as_deref());
            let flat = matches!(ease, kinema_doc::Ease::Linear);
            if !flat && !is_symmetric(&ease) {
                continue;
            }
            let threshold = match field {
                "transform.position" => 0.02 * c.size[0].min(c.size[1]) as f64,
                "transform.rotation" => 2.0,
                _ => 0.02,
            };
            let mut worst_dur = 0.0f64;
            let mut worst_travel = 0.0f64;
            for pair in kfs.windows(2) {
                let dur = (pair[1].t - pair[0].t).max(0.0);
                let dist = travel(field, &pair[0].v, &pair[1].v);
                if dur > 0.2 || dist > threshold {
                    worst_dur = worst_dur.max(dur);
                    worst_travel = worst_travel.max(dist);
                }
            }
            if worst_dur == 0.0 && worst_travel == 0.0 {
                continue;
            }
            let mut fixed = serde_json::to_value(prop).unwrap_or_else(|_| json!({}));
            if let Some(obj) = fixed.as_object_mut() {
                obj.insert("ease".into(), json!("bezier(.2,.9,.1,1)"));
            }
            out.push(Defect {
                code: "EASE-FLAT",
                severity: "warn",
                comp: comp.into(),
                layer: Some(layer.id.clone()),
                frame: None,
                evidence: json!({
                    "property": field,
                    "easing": ease.canonical(),
                    "verdict": if flat { "linear" } else { "symmetric" },
                    "longest_segment_s": (worst_dur * 1000.0).round() / 1000.0,
                    "furthest_travel": (worst_travel * 100.0).round() / 100.0,
                }),
                hint: "linear and symmetric easing read as mechanical on long or large moves; bias the curve so it leaves fast and settles slowly".into(),
                fix_op: Some(set_prop(
                    format!("comp:{comp}/layer:{}/{field}", layer.id),
                    fixed,
                    "bias the easing: fast out, slow in",
                )),
            });
        }
    }
}

fn moved(a: &[kinema_doc::EvaluatedLayer], b: &[kinema_doc::EvaluatedLayer]) -> bool {
    if a.len() != b.len() {
        return true;
    }
    for (x, y) in a.iter().zip(b.iter()) {
        if x.visible != y.visible {
            return true;
        }
        if (x.position[0] - y.position[0]).abs() > 0.5
            || (x.position[1] - y.position[1]).abs() > 0.5
            || (x.scale[0] - y.scale[0]).abs() > 0.005
            || (x.scale[1] - y.scale[1]).abs() > 0.005
            || (x.rotation - y.rotation).abs() > 0.25
            || (x.opacity - y.opacity).abs() > 0.005
        {
            return true;
        }
    }
    false
}

/// Constant motion is the loudest generated tell: nothing ever resolves.
fn hold_none(doc: &Doc, comp: &str, out: &mut Vec<Defect>) {
    let Ok(c) = doc.comp(comp) else { return };
    if c.duration < 3.0 {
        return;
    }
    let step = 0.125;
    let mut prev: Option<Vec<kinema_doc::EvaluatedLayer>> = None;
    let mut last_still = 0.0f64;
    let mut worst_gap = 0.0f64;
    let mut t = 0.0;
    while t <= c.duration + 1e-9 {
        let cur = doc.eval_comp(comp, t).ok();
        if let (Some(a), Some(b)) = (prev.as_ref(), cur.as_ref()) {
            if moved(a, b) {
                worst_gap = worst_gap.max(t - last_still);
            } else {
                last_still = t;
            }
        }
        prev = cur;
        t += step;
    }
    if worst_gap <= 3.0 {
        return;
    }
    out.push(Defect {
        code: "HOLD-NONE",
        severity: "warn",
        comp: comp.into(),
        layer: None,
        frame: None,
        evidence: json!({
            "longest_motion_s": (worst_gap * 1000.0).round() / 1000.0,
            "comp_duration_s": c.duration,
            "sampled_every_s": step,
        }),
        hint: "no still moment in over three seconds: let a composition settle before the next move, by repeating a keyframe value or ending the animation early".into(),
        // The remedy is a directing choice: which beat to hold is not something an
        // engine can pick for the agent.
        fix_op: None,
    });
}

/// Above 1.5 px per frame a move strobes unless the shutter integrates it. The fix is the
/// film default, and it is a number the renderer actually reads.
fn shutter(doc: &Doc, comp: &str, out: &mut Vec<Defect>) {
    let Ok(c) = doc.comp(comp) else { return };
    if c.shutter > 0.0 {
        return;
    }
    let fps = doc.fps.as_f64().max(1.0);
    for layer in &c.layers {
        let Some(pos) = &layer.transform.position else {
            continue;
        };
        let Some(kfs) = pos.kf.as_ref() else { continue };
        let mut worst = 0.0f64;
        let mut at = 0.0f64;
        for pair in kfs.windows(2) {
            let dt = (pair[1].t - pair[0].t).max(1e-9);
            let speed = travel("transform.position", &pair[0].v, &pair[1].v) / (dt * fps);
            if speed > worst {
                worst = speed;
                at = pair[0].t;
            }
        }
        if worst <= 1.5 {
            continue;
        }
        out.push(Defect {
            code: "SHUTTER",
            severity: "warn",
            comp: comp.into(),
            layer: Some(layer.id.clone()),
            frame: Some(doc.fps.ticks_to_frame(kinema_time::seconds_to_ticks(at))),
            evidence: json!({
                "pixels_per_frame": (worst * 100.0).round() / 100.0,
                "threshold": 1.5,
                "shutter_deg": c.shutter,
                "fps": fps,
            }),
            hint: "moves above 1.5 px per frame strobe without motion blur; a 180 degree shutter is the film default".into(),
            fix_op: Some(set_prop(
                format!("comp:{comp}/shutter"),
                json!(180.0),
                "integrate the exposure with a 180 degree shutter",
            )),
        });
    }
}

/// Saturated small text on a saturated ground is the first thing 4:2:0 destroys.
fn chroma(doc: &Doc, comp: &str, out: &mut Vec<Defect>) {
    let Ok(c) = doc.comp(comp) else { return };
    for (i, layer) in c.layers.iter().enumerate() {
        let Some(text) = &layer.text else { continue };
        if text.size >= 32.0 || !text.color.is_accent() {
            continue;
        }
        let Ok(ev) = doc.eval_layer(comp, layer, layer.start) else {
            continue;
        };
        let ground = ground_for(doc, comp, i, [ev.position[0], ev.position[1]]);
        if !ground.is_accent() {
            continue;
        }
        let fixed = ground.ideal_text();
        out.push(Defect {
            code: "CHROMA",
            severity: "warn",
            comp: comp.into(),
            layer: Some(layer.id.clone()),
            frame: None,
            evidence: json!({
                "text_size_px": text.size,
                "text": text.color.to_hex(),
                "ground": ground.to_hex(),
                "text_chroma": (text.color.oklab_chroma() * 1000.0).round() / 1000.0,
                "ground_chroma": (ground.oklab_chroma() * 1000.0).round() / 1000.0,
            }),
            hint: "saturated text under 32 px on a saturated ground loses its edges to 4:2:0 chroma subsampling; go neutral or go bigger".into(),
            fix_op: Some(set_prop(
                format!("comp:{comp}/layer:{}/text.color", layer.id),
                json!(fixed.to_hex()),
                "drop the text to a neutral so chroma subsampling cannot smear it",
            )),
        });
    }
}

/// Sizes should come from the modular scale, and there should be few of them.
fn type_scale(doc: &Doc, comp: &str, out: &mut Vec<Defect>) {
    let Ok(c) = doc.comp(comp) else { return };
    // A document with a look is measured against its own scale; otherwise the house one.
    let steps = match &doc.look {
        Some(look) => kinema_type::ladder(look.scale.base, look.scale.ratio, look.scale.steps),
        None => kinema_type::house_ladder(),
    };
    let mut sizes: Vec<f64> = Vec::new();
    for layer in &c.layers {
        let Some(text) = &layer.text else { continue };
        if !sizes.iter().any(|s| (s - text.size).abs() < 0.5) {
            sizes.push(text.size);
        }
        let Some((step, off)) = kinema_type::nearest_step(text.size, &steps) else {
            continue;
        };
        // 3% is about a quarter of a step at a 1.25 ratio: below it the eye reads the
        // size as the step, above it the size is simply off the system.
        if off > 0.03 {
            out.push(Defect {
                code: "TYPE-SCALE",
                severity: "warn",
                comp: comp.into(),
                layer: Some(layer.id.clone()),
                frame: None,
                evidence: json!({
                    "reason": "off the scale",
                    "size": text.size,
                    "nearest": step.size,
                    "role": kinema_type::role_of(step.index),
                    "off_by_ratio": (off * 1000.0).round() / 1000.0,
                }),
                hint: format!(
                    "{} px is not on the house scale; the nearest step is {} px ({})",
                    text.size,
                    step.size,
                    kinema_type::role_of(step.index)
                ),
                fix_op: Some(set_prop(
                    format!("comp:{comp}/layer:{}/text.size", layer.id),
                    json!(step.size),
                    "put the size on the modular scale",
                )),
            });
        }
    }
    if sizes.len() > 4 {
        out.push(Defect {
            code: "TYPE-SCALE",
            severity: "warn",
            comp: comp.into(),
            layer: None,
            frame: None,
            evidence: json!({
                "reason": "too many sizes",
                "distinct": sizes.len(),
                "sizes": sizes,
                "budget": 4,
            }),
            hint: "four sizes is a hierarchy; more is a slide deck. Which size to drop is a directing call, so `kin type` shows the ladder to choose from".into(),
            fix_op: None,
        });
    }
}

/// A ramp quantised to 8 bits per channel shows its steps. Measure the longest run of one
/// colour across the gradient: that run is the band, and dithering is the remedy.
fn banding(doc: &Doc, comp: &str, out: &mut Vec<Defect>) {
    let Ok(c) = doc.comp(comp) else { return };
    let (w, h) = (c.size[0] as f64, c.size[1] as f64);
    for layer in &c.layers {
        let (field, g) = if let Some(g) = layer.solid.as_ref().and_then(|s| s.gradient.as_ref()) {
            ("solid.gradient", g)
        } else if let Some(g) = layer.shape.as_ref().and_then(|s| s.gradient.as_ref()) {
            ("shape.gradient", g)
        } else {
            continue;
        };
        if g.dither {
            continue;
        }
        let y = h / 2.0;
        let mut prev = g.sample(g.project(0.5, y, 0.0, 0.0, w, h));
        let mut run = 1usize;
        let mut longest = 1usize;
        let mut x = 1usize;
        while (x as f64) < w {
            let colour = g.sample(g.project(x as f64 + 0.5, y, 0.0, 0.0, w, h));
            if colour == prev {
                run += 1;
            } else {
                run = 1;
                prev = colour;
            }
            longest = longest.max(run);
            x += 1;
        }
        // Sixteen pixels of one colour is where a ramp stops reading as a ramp.
        if longest > 16 {
            out.push(Defect {
                code: "BANDING",
                severity: "warn",
                comp: comp.into(),
                layer: Some(layer.id.clone()),
                frame: None,
                evidence: json!({
                    "reason": "flat run in a ramp",
                    "longest_run_px": longest,
                    "span_px": w,
                    "stops": g.stops.len(),
                    "dither": g.dither,
                }),
                hint: format!(
                    "the ramp holds {} px of a single colour ({:.1}% of the width), which reads as a band on a good display",
                    longest,
                    longest as f64 / w * 100.0
                ),
                fix_op: Some(set_prop(
                    format!("comp:{comp}/layer:{}/{field}.dither", layer.id),
                    json!(true),
                    "dither the ramp so 8-bit steps dissolve into noise",
                )),
            });
        }
    }
}

/// A look commits to one accent hue. A chromatic colour that is not near it is a second
/// idea, and second ideas are what make a frame look assembled rather than designed.
fn look_drift(doc: &Doc, comp: &str, out: &mut Vec<Defect>) {
    let Some(look) = &doc.look else { return };
    let Ok(c) = doc.comp(comp) else { return };
    let accent_hue = look.accent.oklab_hue();
    for layer in &c.layers {
        let mut colours: Vec<(&'static str, kinema_doc::Rgba)> = Vec::new();
        if let Some(s) = &layer.solid {
            colours.push(("solid.color", s.color));
        }
        if let Some(s) = &layer.shape {
            colours.push(("shape.color", s.color));
        }
        if let Some(t) = &layer.text {
            colours.push(("text.color", t.color));
        }
        for (field, colour) in colours {
            if colour.a < 8 || colour.oklab_chroma() <= 0.03 {
                continue; // transparent or neutral: a neutral is not a second idea
            }
            let hue = colour.oklab_hue();
            let mut distance = (hue - accent_hue).abs();
            if distance > 180.0 {
                distance = 360.0 - distance;
            }
            if distance <= 25.0 {
                continue;
            }
            out.push(Defect {
                code: "LOOK-DRIFT",
                severity: "warn",
                comp: comp.into(),
                layer: Some(layer.id.clone()),
                frame: None,
                evidence: json!({
                    "reason": "second hue",
                    "field": field,
                    "colour": colour.to_hex(),
                    "hue": (hue * 10.0).round() / 10.0,
                    "accent_hue": (accent_hue * 10.0).round() / 10.0,
                    "distance_deg": (distance * 10.0).round() / 10.0,
                }),
                hint: format!(
                    "the look is built on one accent hue ({accent_hue:.0}°); {} sits {distance:.0}° away, which reads as a second idea",
                    colour.to_hex()
                ),
                fix_op: Some(set_prop(
                    format!("comp:{comp}/layer:{}/{field}", layer.id),
                    json!(look.accent.to_hex()),
                    "bring the colour back onto the look's accent hue",
                )),
            });
        }
    }
}

fn join_weights(w: &[u16]) -> String {
    w.iter()
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn type_fake(doc: &Doc, comp: &str, out: &mut Vec<Defect>) {
    let Ok(c) = doc.comp(comp) else { return };
    for layer in &c.layers {
        let Some(text) = &layer.text else { continue };
        let bundled = kinema_text::find(&text.font, text.weight, text.italic).is_some();
        // A real italic is a separate face, not a slant. Only complain when the family
        // genuinely has none.
        if text.italic && bundled && !kinema_text::has_italic(&text.font) {
            out.push(Defect {
                code: "TYPE-FAKE",
                severity: "error",
                comp: comp.into(),
                layer: Some(layer.id.clone()),
                frame: None,
                evidence: json!({ "reason": "synthetic oblique", "font": text.font }),
                hint: format!(
                    "{} ships no italic, so italic would render as a slanted copy; use weight or size for emphasis",
                    text.font
                ),
                fix_op: Some(set_prop(
                    format!("comp:{comp}/layer:{}/text.italic", layer.id),
                    json!(false),
                    "remove synthesised italic; emphasise with weight instead",
                )),
            });
        }
        let available = kinema_text::weights(&text.font);
        if bundled && !available.is_empty() && !available.contains(&text.weight) {
            let nearest = available
                .iter()
                .min_by_key(|w| (**w as i64 - text.weight as i64).abs())
                .copied()
                .unwrap_or(400);
            out.push(Defect {
                code: "TYPE-FAKE",
                severity: "warn",
                comp: comp.into(),
                layer: Some(layer.id.clone()),
                frame: None,
                evidence: json!({ "reason": "weight not available", "weight": text.weight, "available": available }),
                hint: format!("{} ships {}; asking for {} makes the renderer interpolate a fake cut", text.font, join_weights(&available), text.weight),
                fix_op: Some(set_prop(
                    format!("comp:{comp}/layer:{}/text.weight", layer.id),
                    json!(nearest),
                    "snap to a real bundled weight",
                )),
            });
        }
        if !bundled {
            out.push(Defect {
                code: "TYPE-FAKE",
                severity: "warn",
                comp: comp.into(),
                layer: Some(layer.id.clone()),
                frame: None,
                evidence: json!({ "reason": "font not bundled", "font": text.font, "bundled": bundled_fonts() }),
                hint: "unbundled families fall back to whatever the machine has, so the render stops being reproducible".into(),
                fix_op: Some(set_prop(
                    format!("comp:{comp}/layer:{}/text.font", layer.id),
                    json!(kinema_text::DEFAULT_FAMILY),
                    "use a bundled family so the render is reproducible",
                )),
            });
        }
    }
}

/// The ground a text layer sits on: the nearest opaque layer beneath its centre.
fn ground_for(doc: &Doc, comp: &str, index: usize, center: [f64; 2]) -> Rgba {
    let Ok(c) = doc.comp(comp) else {
        return Rgba::BLACK;
    };
    for layer in c.layers[..index].iter().rev() {
        match layer.kind {
            LayerKind::Solid => {
                if let Some(s) = &layer.solid {
                    return s.color;
                }
            }
            LayerKind::Shape => {
                if let Some(s) = &layer.shape {
                    if let Ok(ev) = doc.eval_layer(comp, layer, layer.start) {
                        let [x, y, w, h] = ev.bbox;
                        if center[0] >= x
                            && center[0] <= x + w
                            && center[1] >= y
                            && center[1] <= y + h
                        {
                            return s.color;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Rgba::BLACK
}

fn read(doc: &Doc, comp: &str, out: &mut Vec<Defect>) {
    let Ok(c) = doc.comp(comp) else { return };
    for (i, layer) in c.layers.iter().enumerate() {
        let Some(text) = &layer.text else { continue };
        let Ok(ev) = doc.eval_layer(comp, layer, layer.start) else {
            continue;
        };
        let center = [ev.position[0], ev.position[1]];
        let ground = ground_for(doc, comp, i, center);
        let ratio = text.color.contrast_ratio(ground);
        let target = if text.size >= 32.0 { 3.0 } else { 4.5 };
        if ratio < target {
            let fixed = text.color.readable_on(ground, target);
            out.push(Defect {
                code: "READ",
                severity: "error",
                comp: comp.into(),
                layer: Some(layer.id.clone()),
                frame: None,
                evidence: json!({
                    "contrast": (ratio * 100.0).round() / 100.0,
                    "target": target,
                    "text": text.color.to_hex(),
                    "ground": ground.to_hex(),
                }),
                hint: format!(
                    "text at {:.2}:1 against its ground needs {:.1}:1; {} would pass",
                    ratio, target, fixed
                ),
                fix_op: Some(set_prop(
                    format!("comp:{comp}/layer:{}/text.color", layer.id),
                    json!(fixed.to_hex()),
                    "raise text contrast against its ground",
                )),
            });
        }
    }
}

/// Colour roles: one accent, one neutral ramp. Two accent hues is how projects start
/// to look like a template rather than a design.
fn palette(doc: &Doc, comp: &str, out: &mut Vec<Defect>) {
    let Ok(c) = doc.comp(comp) else { return };
    let mut accents: Vec<(String, f64, Rgba, String)> = Vec::new();
    for layer in &c.layers {
        for (field, color) in layer_colors(layer) {
            if color.is_accent() {
                accents.push((layer.id.clone(), color.oklab_hue(), color, field));
            }
        }
    }
    if accents.len() < 2 {
        return;
    }
    // Cluster by hue within 25 degrees.
    let mut clusters: Vec<Vec<(String, f64, Rgba, String)>> = Vec::new();
    for a in accents {
        match clusters.iter_mut().find(|cl| {
            let h = cl[0].1;
            let d = (a.1 - h).abs().min(360.0 - (a.1 - h).abs());
            d <= 25.0
        }) {
            Some(cl) => cl.push(a),
            None => clusters.push(vec![a]),
        }
    }
    if clusters.len() > 1 {
        clusters.sort_by_key(|c| std::cmp::Reverse(c.len()));
        let loser = &clusters[clusters.len() - 1];
        let (layer_id, hue, color, field) = &loser[0];
        let neutral = Rgba::opaque(0x9a, 0xa0, 0xa6);
        out.push(Defect {
            code: "PALETTE",
            severity: "warn",
            comp: comp.into(),
            layer: Some(layer_id.clone()),
            frame: None,
            evidence: json!({
                "accent_hues": clusters.iter().map(|cl| (cl[0].1 * 10.0).round() / 10.0).collect::<Vec<_>>(),
                "offending": { "hue": (hue * 10.0).round() / 10.0, "colour": color.to_hex(), "field": field },
            }),
            hint: "more than one accent hue; keep one accent and let neutrals carry the rest".into(),
            fix_op: Some(set_prop(
                format!("comp:{comp}/layer:{layer_id}/{field}"),
                json!(neutral.to_hex()),
                "demote the second accent to a neutral",
            )),
        });
    }
}

fn layer_colors(layer: &kinema_doc::Layer) -> Vec<(String, Rgba)> {
    let mut v = Vec::new();
    if let Some(s) = &layer.solid {
        v.push(("solid.color".to_string(), s.color));
    }
    if let Some(s) = &layer.shape {
        v.push(("shape.color".to_string(), s.color));
    }
    if let Some(t) = &layer.text {
        v.push(("text.color".to_string(), t.color));
    }
    v
}

/// Pros offset entrances by two to four frames so the eye can follow. Everything
/// arriving on one frame is the most common generated-looking tell.
fn stagger<F>(doc: &Doc, comp: &str, _frame: Option<u32>, out: &mut Vec<Defect>, frame_of: F)
where
    F: Fn(f64) -> u32,
{
    let Ok(c) = doc.comp(comp) else { return };
    if c.layers.len() < 3 {
        return;
    }
    let mut groups: std::collections::BTreeMap<u32, Vec<String>> =
        std::collections::BTreeMap::new();
    for layer in &c.layers {
        groups
            .entry(frame_of(layer.start))
            .or_default()
            .push(layer.id.clone());
    }
    let (frame, ids) = groups
        .iter()
        .max_by_key(|(_, ids)| ids.len())
        .map(|(f, ids)| (*f, ids.clone()))
        .expect("non-empty");
    let share = ids.len() as f64 / c.layers.len() as f64;
    if share <= 0.7 {
        return;
    }
    let last = ids.last().cloned().unwrap_or_default();
    let offset = doc.frame_time(frame + 3);
    out.push(Defect {
        code: "STAGGER",
        severity: "warn",
        comp: comp.into(),
        layer: Some(last.clone()),
        frame: Some(frame),
        evidence: json!({
            "entering_on_frame": frame,
            "layers": ids,
            "share": (share * 100.0).round() / 100.0,
        }),
        hint:
            "more than 70% of layers enter on one frame; offset by 2-4 frames so the eye can follow"
                .into(),
        fix_op: Some(set_prop(
            format!("comp:{comp}/layer:{last}/start"),
            json!(offset),
            "offset this entrance by three frames",
        )),
    });
}

pub struct FixStep {
    pub code: &'static str,
    pub layer: Option<String>,
    pub op: kinema_doc::Op,
    pub version: u64,
}

pub struct FixReport {
    pub doc: Doc,
    pub steps: Vec<FixStep>,
    /// Set when two remedies disagree and the document starts cycling.
    pub conflict: Option<String>,
    pub remaining: Vec<Defect>,
}

/// Apply remedies until nothing fixable is left.
///
/// Fix ops are computed against one snapshot, so CHROMA's remedy and READ's remedy can
/// contradict each other. Applying them as a batch is therefore wrong: the document has
/// to be re-measured after each step, which is exactly what this loop does. One step per
/// defect keeps the trail auditable, and a state that repeats means two remedies fight.
pub fn fixpoint(
    doc: &Doc,
    comp: &str,
    frame: Option<u32>,
    max: usize,
) -> Result<FixReport, kinema_doc::DocError> {
    let mut cur = doc.clone();
    let mut steps = Vec::new();
    let mut conflict = None;
    let mut seen = vec![state_key(&cur)];
    for _ in 0..max {
        let found = run(&cur, comp, frame);
        let Some(defect) = found.iter().find(|d| d.fix_op.is_some()) else {
            break;
        };
        let Some(fix) = defect.fix_op.clone() else {
            break;
        };
        let op: kinema_doc::Op = match serde_json::from_value(fix) {
            Ok(op) => op,
            Err(e) => {
                conflict = Some(format!("fix_op for {} does not parse: {e}", defect.code));
                break;
            }
        };
        cur.apply(&op)?;
        steps.push(FixStep {
            code: defect.code,
            layer: defect.layer.clone(),
            op,
            version: cur.version,
        });
        let key = state_key(&cur);
        if seen.contains(&key) {
            conflict = Some(format!(
                "the document returned to an earlier state while fixing {}: two remedies disagree",
                defect.code
            ));
            break;
        }
        seen.push(key);
    }
    Ok(FixReport {
        remaining: run(&cur, comp, frame),
        doc: cur,
        steps,
        conflict,
    })
}

fn state_key(doc: &Doc) -> String {
    serde_json::to_string(doc).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kinema_doc::{Prop, Rgba};

    fn defects(doc: &Doc) -> Vec<Defect> {
        run(doc, "main", None)
    }

    #[test]
    fn starter_passes_every_check() {
        let found = defects(&Doc::starter());
        assert!(
            found.is_empty(),
            "starter should be clean, got {:?}",
            found.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn an_unknown_modifier_is_caught_before_a_render() {
        let mut doc = Doc::starter();
        let layer: kinema_doc::Layer = serde_json::from_value(json!({
            "id": "bloomed", "kind": "solid", "start": 0.0, "duration": 4.0,
            "modifiers": [{"type": "bloom"}]
        }))
        .unwrap();
        doc.apply(&kinema_doc::Op::InsertLayer {
            comp: "main".into(),
            index: None,
            layer: Box::new(layer),
            reason: "test".into(),
        })
        .unwrap();
        let found = defects(&doc);
        let d = found
            .iter()
            .find(|d| d.code == "MODIFIER")
            .expect("MODIFIER");
        let fix: kinema_doc::Op = serde_json::from_value(d.fix_op.clone().unwrap()).unwrap();
        doc.apply(&fix).unwrap();
        assert!(
            !defects(&doc).iter().any(|d| d.code == "MODIFIER"),
            "the fix clears the modifier the build cannot draw"
        );
    }

    #[test]
    fn a_matte_that_never_reaches_its_layer_is_reported() {
        let mut doc = Doc::starter();
        let ops: Vec<kinema_doc::Op> = serde_json::from_value(json!([
            {"op": "set-mattes", "comp": "main", "layer": "title",
             "mattes": {"alpha": "accent"}, "reason": "test"},
            {"op": "set-prop", "path": "comp:main/layer:accent/transform.position",
             "value": [120.0, 120.0], "reason": "test"}
        ]))
        .unwrap();
        for op in &ops {
            doc.apply(op).unwrap();
        }
        let codes: Vec<_> = defects(&doc).iter().map(|d| d.code).collect();
        assert!(codes.contains(&"MATTE-COVER"), "{codes:?}");
    }

    #[test]
    fn a_matte_that_does_overlap_is_left_alone() {
        let mut doc = Doc::starter();
        let op: kinema_doc::Op = serde_json::from_value(json!({
            "op": "set-mattes", "comp": "main", "layer": "title",
            "mattes": {"alpha": "accent"}, "reason": "test"
        }))
        .unwrap();
        doc.apply(&op).unwrap();
        let codes: Vec<_> = defects(&doc).iter().map(|d| d.code).collect();
        assert!(!codes.contains(&"MATTE-COVER"), "{codes:?}");
    }

    #[test]
    fn a_nested_comp_with_nothing_in_it_is_reported() {
        let mut doc = Doc::starter();
        doc.comps.insert(
            "empty".into(),
            kinema_doc::Comp {
                size: [320, 180],
                duration: 4.0,
                shutter: 0.0,
                samples: 8,
                layers: vec![],
            },
        );
        let layer: kinema_doc::Layer = serde_json::from_value(json!({
            "id": "nested", "kind": "comp", "start": 0.0, "duration": 4.0,
            "source": "empty"
        }))
        .unwrap();
        doc.apply(&kinema_doc::Op::InsertLayer {
            comp: "main".into(),
            index: None,
            layer: Box::new(layer),
            reason: "test".into(),
        })
        .unwrap();
        let codes: Vec<_> = defects(&doc).iter().map(|d| d.code).collect();
        assert!(codes.contains(&"EMPTY-COMP"), "{codes:?}");
    }

    #[test]
    fn everything_entering_on_one_frame_is_caught() {
        let mut doc = Doc::starter();
        for layer in doc.comps.get_mut("main").unwrap().layers.iter_mut() {
            layer.start = 0.0;
        }
        let found = defects(&doc);
        let stagger = found.iter().find(|d| d.code == "STAGGER").expect("STAGGER");
        let fix = stagger.fix_op.clone().unwrap();
        let op: kinema_doc::Op = serde_json::from_value(fix).unwrap();
        doc.apply(&op).unwrap();
        assert!(
            !defects(&doc).iter().any(|d| d.code == "STAGGER"),
            "fix should clear it"
        );
        for d in &found {
            assert!(d.fix_op.is_some(), "{} must carry a fix_op", d.code);
        }
    }

    #[test]
    fn low_contrast_is_caught_with_a_working_fix() {
        let mut doc = Doc::starter();
        doc.comps.get_mut("main").unwrap().layers[2]
            .text
            .as_mut()
            .unwrap()
            .color = Rgba::opaque(0x11, 0x11, 0x14);
        let found = defects(&doc);
        let read = found
            .iter()
            .find(|d| d.code == "READ")
            .expect("READ defect");
        let fix = read.fix_op.clone().unwrap();
        let op: kinema_doc::Op = serde_json::from_value(fix).unwrap();
        let mut doc2 = doc.clone();
        doc2.apply(&op).unwrap();
        assert!(
            !defects(&doc2).iter().any(|d| d.code == "READ"),
            "fix should clear the defect"
        );
    }

    #[test]
    fn linear_easing_on_a_long_move_is_caught_and_fixed() {
        let mut doc = Doc::starter();
        doc.comps.get_mut("main").unwrap().layers[2]
            .transform
            .position
            .as_mut()
            .unwrap()
            .ease = Some("linear".into());
        let found = defects(&doc);
        let d = found
            .iter()
            .find(|d| d.code == "EASE-FLAT")
            .expect("EASE-FLAT");
        assert_eq!(d.evidence["verdict"], "linear");
        let op: kinema_doc::Op = serde_json::from_value(d.fix_op.clone().unwrap()).unwrap();
        doc.apply(&op).unwrap();
        assert!(
            !defects(&doc).iter().any(|d| d.code == "EASE-FLAT"),
            "the biased curve should clear it"
        );
    }

    #[test]
    fn symmetric_beziers_are_caught() {
        let mut doc = Doc::starter();
        doc.comps.get_mut("main").unwrap().layers[2]
            .transform
            .position
            .as_mut()
            .unwrap()
            .ease = Some("bezier(.42,0,.58,1)".into());
        let found = defects(&doc);
        let d = found
            .iter()
            .find(|d| d.code == "EASE-FLAT")
            .expect("EASE-FLAT");
        assert_eq!(d.evidence["verdict"], "symmetric");
    }

    #[test]
    fn constant_motion_is_caught_and_is_advisory() {
        let mut doc = Doc::starter();
        doc.comps.get_mut("main").unwrap().layers[2]
            .transform
            .position = Some(serde_json::from_str(r#"{"expr":"[160 + t * 120, 600]"}"#).unwrap());
        let found = defects(&doc);
        let d = found
            .iter()
            .find(|d| d.code == "HOLD-NONE")
            .expect("HOLD-NONE");
        assert!(
            d.fix_op.is_none(),
            "choosing a beat to hold is a directing call"
        );
    }

    #[test]
    fn fast_moves_without_a_shutter_are_caught() {
        let mut doc = Doc::starter();
        doc.comps.get_mut("main").unwrap().shutter = 0.0;
        let found = defects(&doc);
        let d = found.iter().find(|d| d.code == "SHUTTER").expect("SHUTTER");
        assert!(d.evidence["pixels_per_frame"].as_f64().unwrap() > 1.5);
        let op: kinema_doc::Op = serde_json::from_value(d.fix_op.clone().unwrap()).unwrap();
        doc.apply(&op).unwrap();
        assert_eq!(doc.comps["main"].shutter, 180.0);
        assert!(!defects(&doc).iter().any(|d| d.code == "SHUTTER"));
    }

    #[test]
    fn saturated_small_text_on_saturated_ground_is_caught() {
        let mut doc = Doc::starter();
        let layers = &mut doc.comps.get_mut("main").unwrap().layers;
        layers[0].solid = Some(kinema_doc::Solid {
            color: Rgba::opaque(0x40, 0x10, 0xa0),
            gradient: None,
        });
        let text = layers[2].text.as_mut().unwrap();
        text.size = 24.0;
        text.color = Rgba::opaque(0xff, 0x00, 0x60);
        let found = defects(&doc);
        assert!(found.iter().any(|d| d.code == "CHROMA"), "{found:?}");
    }

    #[test]
    fn fixpoint_converges_on_a_document_full_of_tells() {
        let mut doc = Doc::starter();
        let layers = &mut doc.comps.get_mut("main").unwrap().layers;
        layers[0].solid = Some(kinema_doc::Solid {
            color: Rgba::opaque(0x40, 0x10, 0xa0),
            gradient: None,
        });
        layers[2].transform.position.as_mut().unwrap().ease = Some("linear".into());
        let text = layers[2].text.as_mut().unwrap();
        text.size = 24.0;
        text.color = Rgba::opaque(0xff, 0x00, 0x60);
        assert!(!defects(&doc).is_empty());

        let report = fixpoint(&doc, "main", None, 20).unwrap();
        assert!(report.conflict.is_none(), "{:?}", report.conflict);
        assert!(
            report.steps.len() >= 3,
            "expected several steps, got {}",
            report.steps.len()
        );
        assert!(
            report.remaining.iter().all(|d| d.fix_op.is_none()),
            "left fixable: {:?}",
            report.remaining.iter().map(|d| d.code).collect::<Vec<_>>()
        );
        assert!(
            !report.remaining.iter().any(|d| d.severity == "error"),
            "errors remain: {:?}",
            report.remaining.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn fixpoint_leaves_a_clean_document_alone() {
        let doc = Doc::starter();
        let report = fixpoint(&doc, "main", None, 20).unwrap();
        assert!(report.steps.is_empty());
        assert!(report.remaining.is_empty());
    }

    #[test]
    fn fake_italic_is_caught_only_for_a_family_with_no_italic() {
        let mut doc = Doc::starter();
        doc.comps.get_mut("main").unwrap().layers[2]
            .text
            .as_mut()
            .unwrap()
            .italic = true;
        assert!(
            !defects(&doc).iter().any(|d| d.code == "TYPE-FAKE"),
            "Lato has a real italic, so italic is not a fake"
        );

        doc.comps.get_mut("main").unwrap().layers[2]
            .text
            .as_mut()
            .unwrap()
            .font = "Space Mono".into();
        let found = defects(&doc);
        assert!(
            found
                .iter()
                .any(|d| d.code == "TYPE-FAKE" && d.evidence["reason"] == "synthetic oblique"),
            "the mono has no italic, so asking for one is a synthetic oblique: {found:?}"
        );
    }

    #[test]
    fn a_missing_weight_is_caught_and_snapped_to_a_real_one() {
        let mut doc = Doc::starter();
        doc.comps.get_mut("main").unwrap().layers[2]
            .text
            .as_mut()
            .unwrap()
            .weight = 300;
        let found = defects(&doc);
        let d = found
            .iter()
            .find(|d| d.code == "TYPE-FAKE" && d.evidence["reason"] == "weight not available")
            .expect("weight defect");
        assert_eq!(d.evidence["available"], serde_json::json!([400, 700]));
        let op: kinema_doc::Op = serde_json::from_value(d.fix_op.clone().unwrap()).unwrap();
        doc.apply(&op).unwrap();
        assert_eq!(
            doc.comps["main"].layers[2].text.as_ref().unwrap().weight,
            400
        );
        assert!(!defects(&doc).iter().any(|d| d.code == "TYPE-FAKE"));
    }

    #[test]
    fn an_off_scale_size_is_caught_and_snapped() {
        let mut doc = Doc::starter();
        doc.comps.get_mut("main").unwrap().layers[2]
            .text
            .as_mut()
            .unwrap()
            .size = 24.0;
        let found = defects(&doc);
        let d = found
            .iter()
            .find(|d| d.code == "TYPE-SCALE")
            .expect("TYPE-SCALE");
        assert_eq!(d.evidence["nearest"], 25.0);
        let op: kinema_doc::Op = serde_json::from_value(d.fix_op.clone().unwrap()).unwrap();
        doc.apply(&op).unwrap();
        assert_eq!(
            doc.comps["main"].layers[2].text.as_ref().unwrap().size,
            25.0
        );
        assert!(!defects(&doc).iter().any(|d| d.code == "TYPE-SCALE"));
    }

    #[test]
    fn a_second_hue_is_caught_and_snapped_to_the_accent() {
        let mut doc = Doc::starter();
        assert!(
            !defects(&doc).iter().any(|d| d.code == "LOOK-DRIFT"),
            "the starter is built on its look"
        );
        doc.comps.get_mut("main").unwrap().layers[1]
            .shape
            .as_mut()
            .unwrap()
            .color = kinema_doc::Rgba::opaque(0xff, 0x00, 0x55);
        let found = defects(&doc);
        let d = found
            .iter()
            .find(|d| d.code == "LOOK-DRIFT")
            .expect("LOOK-DRIFT");
        assert_eq!(d.evidence["field"], "shape.color");
        assert!(d.evidence["distance_deg"].as_f64().unwrap() > 100.0);
        let op: kinema_doc::Op = serde_json::from_value(d.fix_op.clone().unwrap()).unwrap();
        doc.apply(&op).unwrap();
        assert!(!defects(&doc).iter().any(|d| d.code == "LOOK-DRIFT"));
    }

    fn bg_gradient() -> kinema_doc::Gradient {
        kinema_doc::Gradient {
            kind: kinema_doc::GradientKind::Linear,
            angle: 0.0,
            stops: vec![
                kinema_doc::Stop {
                    at: 0.0,
                    color: kinema_doc::Rgba::opaque(0x10, 0x12, 0x18),
                },
                kinema_doc::Stop {
                    at: 1.0,
                    color: kinema_doc::Rgba::opaque(0x16, 0x18, 0x20),
                },
            ],
            dither: false,
        }
    }

    #[test]
    fn a_flat_ramp_is_caught_and_dithered() {
        let mut doc = Doc::starter();
        doc.comps.get_mut("main").unwrap().layers[0]
            .solid
            .as_mut()
            .unwrap()
            .gradient = Some(bg_gradient());
        let found = defects(&doc);
        let d = found.iter().find(|d| d.code == "BANDING").expect("BANDING");
        assert!(d.evidence["longest_run_px"].as_u64().unwrap() > 16);
        let op: kinema_doc::Op = serde_json::from_value(d.fix_op.clone().unwrap()).unwrap();
        doc.apply(&op).unwrap();
        assert!(
            doc.comps["main"].layers[0]
                .solid
                .as_ref()
                .unwrap()
                .gradient
                .as_ref()
                .unwrap()
                .dither,
            "the remedy turns dithering on"
        );
        assert!(!defects(&doc).iter().any(|d| d.code == "BANDING"));
    }

    #[test]
    fn a_dithered_ramp_is_left_alone() {
        let mut doc = Doc::starter();
        let mut g = bg_gradient();
        g.dither = true;
        doc.comps.get_mut("main").unwrap().layers[0]
            .solid
            .as_mut()
            .unwrap()
            .gradient = Some(g);
        assert!(!defects(&doc).iter().any(|d| d.code == "BANDING"));
    }

    #[test]
    fn a_document_without_a_look_is_not_policed() {
        let mut doc = Doc::starter();
        doc.look = None;
        doc.comps.get_mut("main").unwrap().layers[1]
            .shape
            .as_mut()
            .unwrap()
            .color = kinema_doc::Rgba::opaque(0xff, 0x00, 0x55);
        let found = defects(&doc);
        assert!(
            !found.iter().any(|d| d.code == "LOOK-DRIFT"),
            "without a stated look there is nothing to drift from"
        );
    }

    #[test]
    fn the_look_supplies_the_scale() {
        let mut doc = Doc::starter();
        if let Some(look) = doc.look.as_mut() {
            look.scale.base = 20.0;
            look.scale.ratio = 1.5;
        }
        // 96 px is on the house scale but not on 20 x 1.5 (20, 30, 45, 67.5, 101.25).
        let found = defects(&doc);
        let d = found
            .iter()
            .find(|d| d.code == "TYPE-SCALE")
            .expect("the document's own scale is the one that counts");
        assert_eq!(d.evidence["nearest"], 101.25);
    }

    #[test]
    fn too_many_sizes_is_advisory() {
        let mut doc = Doc::starter();
        let extra = doc.comps["main"].layers[2].clone();
        for (i, size) in [25.0, 31.25, 39.06, 61.04].iter().enumerate() {
            let mut layer = extra.clone();
            layer.id = format!("t{i}");
            let text = layer.text.as_mut().unwrap();
            text.size = *size;
            text.string = format!("LINE {i}");
            doc.comps.get_mut("main").unwrap().layers.push(layer);
        }
        let found = defects(&doc);
        let d = found
            .iter()
            .find(|d| d.code == "TYPE-SCALE" && d.evidence["reason"] == "too many sizes")
            .expect("count defect");
        assert!(
            d.fix_op.is_none(),
            "dropping a size is a directing call, so the hint names the budget instead"
        );
        assert!(d.evidence["distinct"].as_u64().unwrap() > 4);
    }

    #[test]
    fn unbundled_font_is_caught() {
        let mut doc = Doc::starter();
        doc.comps.get_mut("main").unwrap().layers[2]
            .text
            .as_mut()
            .unwrap()
            .font = "Helvetica".into();
        let found = defects(&doc);
        assert!(found
            .iter()
            .any(|d| d.code == "TYPE-FAKE" && d.evidence["reason"] == "font not bundled"));
    }

    #[test]
    fn two_accents_are_caught() {
        let mut doc = Doc::starter();
        doc.comps.get_mut("main").unwrap().layers[1]
            .shape
            .as_mut()
            .unwrap()
            .color = Rgba::opaque(0xff, 0x00, 0x44);
        let mut second = doc.comps["main"].layers[1].clone();
        second.id = "accent2".into();
        second.shape.as_mut().unwrap().color = Rgba::opaque(0x22, 0xcc, 0x55);
        doc.comps.get_mut("main").unwrap().layers.push(second);
        let found = defects(&doc);
        assert!(
            found.iter().any(|d| d.code == "PALETTE"),
            "expected a palette defect"
        );
    }

    #[test]
    fn opacity_change_does_not_break_checks() {
        let mut doc = Doc::starter();
        doc.comps.get_mut("main").unwrap().layers[2].opacity = Prop::number(0.4);
        let _ = defects(&doc);
    }
}
