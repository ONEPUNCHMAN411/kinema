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

/// Faces shipped in this build. Anything else is a system fallback, which is a tell.
pub const BUNDLED_FONTS: &[&str] = &["Inter"];
/// Weights actually available. There is no 300, and there is no synthetic 500.
pub const BUNDLED_WEIGHTS: &[u16] = &[400, 700];

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
    read(doc, comp, &mut out);
    palette(doc, comp, &mut out);
    stagger(doc, comp, frame, &mut out, frame_of);
    let _ = c;
    out
}

fn type_fake(doc: &Doc, comp: &str, out: &mut Vec<Defect>) {
    let Ok(c) = doc.comp(comp) else { return };
    for layer in &c.layers {
        let Some(text) = &layer.text else { continue };
        if text.italic {
            out.push(Defect {
                code: "TYPE-FAKE",
                severity: "error",
                comp: comp.into(),
                layer: Some(layer.id.clone()),
                frame: None,
                evidence: json!({ "reason": "synthetic oblique", "font": text.font }),
                hint: "no italic face is bundled, so italic renders as a slanted copy; use weight or size for emphasis, or add a real italic face with `kin font add`".into(),
                fix_op: Some(set_prop(
                    format!("comp:{comp}/layer:{}/text.italic", layer.id),
                    json!(false),
                    "remove synthesised italic; emphasise with weight instead",
                )),
            });
        }
        if !BUNDLED_WEIGHTS.contains(&text.weight) {
            out.push(Defect {
                code: "TYPE-FAKE",
                severity: "warn",
                comp: comp.into(),
                layer: Some(layer.id.clone()),
                frame: None,
                evidence: json!({ "reason": "weight not available", "weight": text.weight, "available": BUNDLED_WEIGHTS }),
                hint: format!("requested weight {} is synthesised from {}", text.weight, BUNDLED_WEIGHTS[0]),
                fix_op: Some(set_prop(
                    format!("comp:{comp}/layer:{}/text.weight", layer.id),
                    json!(700),
                    "snap to a real bundled weight",
                )),
            });
        }
        if !BUNDLED_FONTS
            .iter()
            .any(|f| f.eq_ignore_ascii_case(&text.font))
        {
            out.push(Defect {
                code: "TYPE-FAKE",
                severity: "warn",
                comp: comp.into(),
                layer: Some(layer.id.clone()),
                frame: None,
                evidence: json!({ "reason": "font not bundled", "font": text.font, "bundled": BUNDLED_FONTS }),
                hint: "unbundled families fall back to whatever the machine has, so the render stops being reproducible".into(),
                fix_op: Some(set_prop(
                    format!("comp:{comp}/layer:{}/text.font", layer.id),
                    json!(BUNDLED_FONTS[0]),
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
    fn fake_italic_is_caught() {
        let mut doc = Doc::starter();
        doc.comps.get_mut("main").unwrap().layers[2]
            .text
            .as_mut()
            .unwrap()
            .italic = true;
        let found = defects(&doc);
        assert!(found.iter().any(|d| d.code == "TYPE-FAKE"));
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
