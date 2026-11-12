//! Typed ops, their inverses, and path addressing.
//!
//! Every mutation is an `Op` that can be inverted, so the journal gives undo, diff and
//! provenance for free. A path looks like `comp:main/layer:title/transform.position`.

use crate::model::{Asset, Gradient, Layer, Look, Matte, Modifier};
use crate::{Doc, DocError, Prop};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum Op {
    SetProp {
        path: String,
        value: Prop,
        #[serde(default)]
        reason: String,
    },
    InsertLayer {
        comp: String,
        #[serde(default)]
        index: Option<usize>,
        /// Boxed: a layer is much larger than any other operand, and ops travel in batches.
        layer: Box<Layer>,
        #[serde(default)]
        reason: String,
    },
    DeleteLayer {
        comp: String,
        layer: String,
        #[serde(default)]
        reason: String,
    },
    /// Create a comp, with no layers unless `layers` says otherwise. Its inverse deletes it.
    CreateComp {
        comp: String,
        #[serde(default)]
        size: Option<[u32; 2]>,
        #[serde(default)]
        duration: Option<f64>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        layers: Vec<Layer>,
        #[serde(default)]
        reason: String,
    },
    /// Delete a comp. Its inverse puts it back, layers and all.
    DeleteComp {
        comp: String,
        #[serde(default)]
        reason: String,
    },
    SetComp {
        comp: String,
        #[serde(default)]
        size: Option<[u32; 2]>,
        #[serde(default)]
        duration: Option<f64>,
        #[serde(default)]
        reason: String,
    },
    SetAsset {
        asset: Asset,
        #[serde(default)]
        reason: String,
    },
    RemoveAsset {
        asset: String,
        #[serde(default)]
        reason: String,
    },
    /// Give a layer's solid or shape a ramp, replacing any it had.
    SetGradient {
        comp: String,
        layer: String,
        /// `solid` or `shape`.
        on: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        gradient: Option<Gradient>,
        #[serde(default)]
        reason: String,
    },
    /// Replace a layer's modifiers, or clear them with `"modifiers": []`.
    SetModifier {
        comp: String,
        layer: String,
        #[serde(default)]
        modifiers: Vec<Modifier>,
        #[serde(default)]
        reason: String,
    },
    /// Point a layer at another as its alpha or luma matte, or clear it with `"mattes": null`.
    SetMattes {
        comp: String,
        layer: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mattes: Option<Matte>,
        #[serde(default)]
        reason: String,
    },
    /// Replace the document's look, or clear it with `"look": null`.
    SetLook {
        #[serde(default)]
        look: Option<Look>,
        #[serde(default)]
        reason: String,
    },
}

impl Op {
    pub fn reason(&self) -> &str {
        match self {
            Op::SetProp { reason, .. }
            | Op::InsertLayer { reason, .. }
            | Op::DeleteLayer { reason, .. }
            | Op::SetComp { reason, .. }
            | Op::CreateComp { reason, .. }
            | Op::DeleteComp { reason, .. }
            | Op::SetAsset { reason, .. }
            | Op::RemoveAsset { reason, .. }
            | Op::SetLook { reason, .. }
            | Op::SetMattes { reason, .. }
            | Op::SetModifier { reason, .. }
            | Op::SetGradient { reason, .. } => reason,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Op::SetProp { .. } => "set-prop",
            Op::InsertLayer { .. } => "insert-layer",
            Op::DeleteLayer { .. } => "delete-layer",
            Op::SetComp { .. } => "set-comp",
            Op::CreateComp { .. } => "create-comp",
            Op::DeleteComp { .. } => "delete-comp",
            Op::SetAsset { .. } => "set-asset",
            Op::RemoveAsset { .. } => "remove-asset",
            Op::SetLook { .. } => "set-look",
            Op::SetGradient { .. } => "set-gradient",
            Op::SetMattes { .. } => "set-mattes",
            Op::SetModifier { .. } => "set-modifier",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AppliedResult {
    pub op: String,
    pub version: u64,
    pub inverse: Option<Op>,
}

/// Translate a kinema path into a JSON pointer against the serialised document.
pub fn pointer_for(doc: &Doc, path: &str) -> Result<String, DocError> {
    let mut ptr = String::new();
    let mut comp: Option<String> = None;
    for seg in path.split('/').filter(|s| !s.is_empty()) {
        if let Some(id) = seg.strip_prefix("comp:") {
            comp = Some(id.to_string());
            push(&mut ptr, "comps");
            push(&mut ptr, id);
        } else if let Some(id) = seg.strip_prefix("layer:") {
            let comp_id = comp.clone().ok_or_else(|| DocError::NotFound {
                path: path.to_string(),
                hint: "a `layer:` segment must follow `comp:<id>`".to_string(),
            })?;
            let c = doc.comp(&comp_id)?;
            let idx =
                c.layers
                    .iter()
                    .position(|l| l.id == id)
                    .ok_or_else(|| DocError::NotFound {
                        path: path.to_string(),
                        hint: format!(
                            "layers in `{comp_id}`: {}",
                            c.layers
                                .iter()
                                .map(|l| l.id.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    })?;
            push(&mut ptr, "layers");
            push(&mut ptr, &idx.to_string());
        } else if let Some(id) = seg.strip_prefix("asset:") {
            push(&mut ptr, "assets");
            push(&mut ptr, id);
        } else {
            for part in seg.split('.') {
                if !part.is_empty() {
                    push(&mut ptr, part);
                }
            }
        }
    }
    if ptr.is_empty() {
        return Err(DocError::NotFound {
            path: path.to_string(),
            hint: "expected comp:main/layer:title/transform.position".to_string(),
        });
    }
    Ok(ptr)
}

fn push(ptr: &mut String, seg: &str) {
    ptr.push('/');
    for ch in seg.chars() {
        match ch {
            '~' => ptr.push_str("~0"),
            '/' => ptr.push_str("~1"),
            other => ptr.push(other),
        }
    }
}

/// Inverse of an op, computed against the document as it was.
fn inverse_of(before: &Doc, op: &Op) -> Result<Option<Op>, DocError> {
    match op {
        Op::SetProp { path, reason, .. } => {
            let ptr = pointer_for(before, path)?;
            let value = serde_json::to_value(before)?;
            match value.pointer(&ptr) {
                Some(old) => {
                    let old_prop: Prop = serde_json::from_value(old.clone())?;
                    Ok(Some(Op::SetProp {
                        path: path.clone(),
                        value: old_prop,
                        reason: format!("undo of: {reason}"),
                    }))
                }
                None => Ok(None),
            }
        }
        Op::InsertLayer {
            comp,
            layer,
            reason,
            ..
        } => Ok(Some(Op::DeleteLayer {
            comp: comp.clone(),
            layer: layer.id.clone(),
            reason: format!("undo of: {reason}"),
        })),
        Op::DeleteLayer {
            comp,
            layer,
            reason,
        } => {
            let c = before.comp(comp)?;
            let index = c.layers.iter().position(|l| &l.id == layer);
            match index {
                Some(index) => {
                    let removed = c.layers[index].clone();
                    Ok(Some(Op::InsertLayer {
                        comp: comp.clone(),
                        index: Some(index),
                        layer: Box::new(removed),
                        reason: format!("undo of: {reason}"),
                    }))
                }
                None => Ok(None),
            }
        }
        Op::CreateComp { comp, reason, .. } => Ok(Some(Op::DeleteComp {
            comp: comp.clone(),
            reason: format!("undo of: {reason}"),
        })),
        Op::DeleteComp { comp, reason } => {
            let c = before.comp(comp)?;
            Ok(Some(Op::CreateComp {
                comp: comp.clone(),
                size: Some(c.size),
                duration: Some(c.duration),
                layers: c.layers.clone(),
                reason: format!("undo of: {reason}"),
            }))
        }
        Op::SetComp { comp, reason, .. } => {
            let c = before.comp(comp)?;
            Ok(Some(Op::SetComp {
                comp: comp.clone(),
                size: Some(c.size),
                duration: Some(c.duration),
                reason: format!("undo of: {reason}"),
            }))
        }
        Op::SetAsset { asset, reason } => match before.assets.get(&asset.id) {
            Some(old) => Ok(Some(Op::SetAsset {
                asset: old.clone(),
                reason: format!("undo of: {reason}"),
            })),
            // A brand new asset is undone by removing it, not by restoring nothing.
            None => Ok(Some(Op::RemoveAsset {
                asset: asset.id.clone(),
                reason: format!("undo of: {reason}"),
            })),
        },
        Op::RemoveAsset { asset, reason } => match before.assets.get(asset) {
            Some(old) => Ok(Some(Op::SetAsset {
                asset: old.clone(),
                reason: format!("undo of: {reason}"),
            })),
            None => Ok(None),
        },
        Op::SetModifier {
            comp,
            layer,
            reason,
            ..
        } => {
            let c = before.comp(comp)?;
            let l = c
                .layers
                .iter()
                .find(|l| &l.id == layer)
                .ok_or_else(|| DocError::NotFound {
                    path: format!("comp:{comp}/layer:{layer}"),
                    hint: "check the layer id".to_string(),
                })?;
            Ok(Some(Op::SetModifier {
                comp: comp.clone(),
                layer: layer.clone(),
                modifiers: l.modifiers.clone(),
                reason: format!("undo of: {reason}"),
            }))
        }
        Op::SetMattes {
            comp,
            layer,
            reason,
            ..
        } => {
            let c = before.comp(comp)?;
            let l = c
                .layers
                .iter()
                .find(|l| &l.id == layer)
                .ok_or_else(|| DocError::NotFound {
                    path: format!("comp:{comp}/layer:{layer}"),
                    hint: "check the layer id".to_string(),
                })?;
            Ok(Some(Op::SetMattes {
                comp: comp.clone(),
                layer: layer.clone(),
                mattes: l.mattes.clone(),
                reason: format!("undo of: {reason}"),
            }))
        }
        Op::SetLook { reason, .. } => Ok(Some(Op::SetLook {
            look: before.look.clone(),
            reason: format!("undo of: {reason}"),
        })),
        Op::SetGradient {
            comp,
            layer,
            on,
            reason,
            ..
        } => {
            let c = before.comp(comp)?;
            let l = c
                .layers
                .iter()
                .find(|l| &l.id == layer)
                .ok_or_else(|| DocError::NotFound {
                    path: format!("comp:{comp}/layer:{layer}"),
                    hint: "check the layer id".to_string(),
                })?;
            let previous = match on.as_str() {
                "solid" => l.solid.as_ref().and_then(|s| s.gradient.clone()),
                "shape" => l.shape.as_ref().and_then(|s| s.gradient.clone()),
                _ => None,
            };
            Ok(Some(Op::SetGradient {
                comp: comp.clone(),
                layer: layer.clone(),
                on: on.clone(),
                gradient: previous,
                reason: format!("undo of: {reason}"),
            }))
        }
    }
}

impl Doc {
    /// Apply one op, returning its inverse. The version is bumped exactly once.
    pub fn apply(&mut self, op: &Op) -> Result<AppliedResult, DocError> {
        let inverse = inverse_of(self, op)?;
        match op {
            Op::SetProp { path, value, .. } => {
                let ptr = pointer_for(self, path)?;
                let mut v = serde_json::to_value(&*self)?;
                let slot = v.pointer_mut(&ptr).ok_or_else(|| DocError::NotFound {
                    path: path.to_string(),
                    hint: "the leaf exists but the container may not; set the parent first"
                        .to_string(),
                })?;
                *slot = serde_json::to_value(value)?;
                *self = serde_json::from_value(v)?;
            }
            Op::InsertLayer {
                comp, index, layer, ..
            } => {
                let c = self.comps.get_mut(comp).ok_or_else(|| DocError::NotFound {
                    path: format!("comp:{comp}"),
                    hint: "create the comp first".to_string(),
                })?;
                if c.layers.iter().any(|l| l.id == layer.id) {
                    return Err(DocError::Invalid {
                        code: "duplicate-layer",
                        message: format!("comp `{comp}` already has a layer `{}`", layer.id),
                        hint: "pick another id, or use set-prop to edit the existing layer"
                            .to_string(),
                    });
                }
                let layer = (**layer).clone();
                match index {
                    Some(i) if *i < c.layers.len() => c.layers.insert(*i, layer),
                    _ => c.layers.push(layer),
                }
            }
            Op::DeleteLayer { comp, layer, .. } => {
                let c = self.comps.get_mut(comp).ok_or_else(|| DocError::NotFound {
                    path: format!("comp:{comp}"),
                    hint: "check the comp id".to_string(),
                })?;
                let before_len = c.layers.len();
                c.layers.retain(|l| &l.id != layer);
                if c.layers.len() == before_len {
                    return Err(DocError::NotFound {
                        path: format!("comp:{comp}/layer:{layer}"),
                        hint: "run `kin probe` to list layer ids".to_string(),
                    });
                }
            }
            Op::SetComp {
                comp,
                size,
                duration,
                ..
            } => {
                let c = self.comps.get_mut(comp).ok_or_else(|| DocError::NotFound {
                    path: format!("comp:{comp}"),
                    hint: "check the comp id".to_string(),
                })?;
                if let Some(s) = size {
                    c.size = *s;
                }
                if let Some(d) = duration {
                    c.duration = *d;
                }
            }
            Op::SetAsset { asset, .. } => {
                self.assets.insert(asset.id.clone(), asset.clone());
            }
            Op::RemoveAsset { asset, .. } => {
                if self.assets.remove(asset).is_none() {
                    return Err(DocError::NotFound {
                        path: format!("asset:{asset}"),
                        hint: "run `kin credits` to list asset ids".to_string(),
                    });
                }
            }
            Op::SetLook { look, .. } => {
                self.look = look.clone();
            }
            Op::CreateComp {
                comp,
                size,
                duration,
                layers,
                ..
            } => {
                if self.comps.contains_key(comp) {
                    return Err(DocError::Invalid {
                        code: "exists",
                        message: format!("comp `{comp}` is already there"),
                        hint: "use set-comp to change a comp that exists".to_string(),
                    });
                }
                if comp.trim().is_empty() {
                    return Err(DocError::Invalid {
                        code: "bad-id",
                        message: "a comp needs an id".to_string(),
                        hint: "a comp layer finds its source by this id".to_string(),
                    });
                }
                self.comps.insert(
                    comp.clone(),
                    crate::model::Comp {
                        size: size.unwrap_or([1920, 1080]),
                        duration: duration.unwrap_or(5.0),
                        shutter: 0.0,
                        samples: 8,
                        layers: layers.clone(),
                    },
                );
            }
            Op::DeleteComp { comp, .. } => {
                if self.comps.remove(comp).is_none() {
                    return Err(DocError::NotFound {
                        path: format!("comp:{comp}"),
                        hint: "check the comp id".to_string(),
                    });
                }
            }
            Op::SetModifier {
                comp,
                layer,
                modifiers,
                ..
            } => {
                let c = self.comps.get_mut(comp).ok_or_else(|| DocError::NotFound {
                    path: format!("comp:{comp}"),
                    hint: "check the comp id".to_string(),
                })?;
                let l = c
                    .layers
                    .iter_mut()
                    .find(|l| &l.id == layer)
                    .ok_or_else(|| DocError::NotFound {
                        path: format!("comp:{comp}/layer:{layer}"),
                        hint: "check the layer id".to_string(),
                    })?;
                l.modifiers = modifiers.clone();
            }
            Op::SetMattes {
                comp,
                layer,
                mattes,
                ..
            } => {
                let c = self.comps.get_mut(comp).ok_or_else(|| DocError::NotFound {
                    path: format!("comp:{comp}"),
                    hint: "check the comp id".to_string(),
                })?;
                let l = c
                    .layers
                    .iter_mut()
                    .find(|l| &l.id == layer)
                    .ok_or_else(|| DocError::NotFound {
                        path: format!("comp:{comp}/layer:{layer}"),
                        hint: "check the layer id".to_string(),
                    })?;
                l.mattes = mattes.clone();
            }
            Op::SetGradient {
                comp,
                layer,
                on,
                gradient,
                ..
            } => {
                let c = self.comps.get_mut(comp).ok_or_else(|| DocError::NotFound {
                    path: format!("comp:{comp}"),
                    hint: "check the comp id".to_string(),
                })?;
                let l = c
                    .layers
                    .iter_mut()
                    .find(|l| &l.id == layer)
                    .ok_or_else(|| DocError::NotFound {
                        path: format!("comp:{comp}/layer:{layer}"),
                        hint: "check the layer id".to_string(),
                    })?;
                let slot = match on.as_str() {
                    "solid" => l.solid.as_mut().map(|s| &mut s.gradient),
                    "shape" => l.shape.as_mut().map(|s| &mut s.gradient),
                    other => {
                        return Err(DocError::Invalid {
                            code: "bad-target",
                            message: format!("`{other}` is not `solid` or `shape`"),
                            hint: "target `solid` or `shape`".to_string(),
                        })
                    }
                };
                let slot = slot.ok_or_else(|| DocError::Invalid {
                    code: "no-block",
                    message: format!("layer `{layer}` has no {on} block"),
                    hint: format!("give the layer a {on} block first"),
                })?;
                *slot = gradient.clone();
            }
        }
        self.version += 1;
        Ok(AppliedResult {
            op: op.name().to_string(),
            version: self.version,
            inverse,
        })
    }

    /// Apply a batch, stopping at the first failure.
    pub fn apply_all(&mut self, ops: &[Op]) -> Result<Vec<AppliedResult>, DocError> {
        let mut out = Vec::with_capacity(ops.len());
        for op in ops {
            out.push(self.apply(op)?);
        }
        Ok(out)
    }
}

/// The ops that `kin apply` accepts, for `capabilities`.
pub fn op_schema() -> Value {
    serde_json::json!({
        "oneOf": [
            {"type": "object", "required": ["op", "path", "value"],
             "properties": {"op": {"const": "set-prop"}, "path": {"type": "string"}, "value": {}, "reason": {"type": "string"}},
             "examples": [{"op": "set-prop", "path": "comp:main/layer:title/text.color", "value": "#ffffff", "reason": "raise contrast on the ground"}]},
            {"type": "object", "required": ["op", "comp", "layer"],
             "properties": {"op": {"const": "insert-layer"}, "comp": {"type": "string"}, "index": {"type": "integer"}, "layer": {"type": "object"}, "reason": {"type": "string"}}},
            {"type": "object", "required": ["op", "comp", "layer"],
             "properties": {"op": {"const": "delete-layer"}, "comp": {"type": "string"}, "layer": {"type": "string"}, "reason": {"type": "string"}}},
            {"type": "object", "required": ["op", "comp"],
             "properties": {"op": {"const": "set-comp"}, "comp": {"type": "string"}, "size": {"type": "array", "items": {"type": "integer"}}, "duration": {"type": "number"}, "reason": {"type": "string"}}},
            {"type": "object", "required": ["op", "asset"],
             "properties": {"op": {"const": "set-asset"}, "asset": {"type": "object"}, "reason": {"type": "string"}}},
            {"type": "object", "required": ["op", "asset"],
             "properties": {"op": {"const": "remove-asset"}, "asset": {"type": "string"}, "reason": {"type": "string"}}},
            {"type": "object", "required": ["op"],
             "properties": {"op": {"const": "set-look"}, "look": {"type": ["object", "null"]}, "reason": {"type": "string"}},
             "examples": [{"op": "set-look", "look": {"accent": "#4c8fff", "scheme": "dark", "scale": {"base": 16, "ratio": 1.25}}, "reason": "one accent for the whole project"}]},
            {"type": "object", "required": ["op", "comp", "layer", "on"],
             "properties": {"op": {"const": "set-gradient"}, "comp": {"type": "string"}, "layer": {"type": "string"}, "on": {"enum": ["solid", "shape"]}, "gradient": {"type": ["object", "null"]}, "reason": {"type": "string"}},
             "examples": [{"op": "set-gradient", "comp": "main", "layer": "bg", "on": "solid", "gradient": {"kind": "linear", "angle": 90, "dither": true, "stops": [{"at": 0, "color": "#0a0e18"}, {"at": 1, "color": "#1b2a4a"}]}, "reason": "a ground with depth"}]},
            {"type": "object", "required": ["op", "comp", "layer"],
             "properties": {"op": {"const": "set-mattes"}, "comp": {"type": "string"}, "layer": {"type": "string"}, "mattes": {"type": ["object", "null"]}, "reason": {"type": "string"}},
             "examples": [{"op": "set-mattes", "comp": "main", "layer": "title", "mattes": {"alpha": "accent"}, "reason": "cut the title to the rule"}]},
            {"type": "object", "required": ["op", "comp", "layer"],
             "properties": {"op": {"const": "set-modifier"}, "comp": {"type": "string"}, "layer": {"type": "string"}, "modifiers": {"type": "array"}, "reason": {"type": "string"}},
             "examples": [{"op": "set-modifier", "comp": "main", "layer": "accent", "modifiers": [{"type": "glow", "radius": 40, "strength": 0.9, "color": "#4c8fff"}], "reason": "let the rule cast light"}]},
            {"type": "object", "required": ["op", "comp"],
             "properties": {"op": {"const": "create-comp"}, "comp": {"type": "string"}, "size": {"type": "array"}, "duration": {"type": "number"}, "layers": {"type": "array"}, "reason": {"type": "string"}},
             "examples": [{"op": "create-comp", "comp": "lower-third", "size": [1920, 400], "duration": 4, "reason": "a reusable strap"}]},
            {"type": "object", "required": ["op", "comp"],
             "properties": {"op": {"const": "delete-comp"}, "comp": {"type": "string"}, "reason": {"type": "string"}},
             "examples": [{"op": "delete-comp", "comp": "lower-third", "reason": "the strap is gone from the cut"}]}
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn set(key: &str, value: serde_json::Value) -> Op {
        serde_json::from_value(json!({
            "op": "set-prop",
            "path": format!("comp:main/layer:title/{key}"),
            "value": value,
            "reason": "test"
        }))
        .unwrap()
    }

    #[test]
    fn paths_translate_to_pointers() {
        let doc = Doc::starter();
        assert_eq!(
            pointer_for(&doc, "comp:main/layer:title/transform.position").unwrap(),
            "/comps/main/layers/2/transform/position"
        );
        assert!(pointer_for(&doc, "comp:main/layer:nope/opacity").is_err());
        assert!(pointer_for(&doc, "layer:title/opacity").is_err());
    }

    #[test]
    fn set_prop_round_trips_and_inverts() {
        let mut doc = Doc::starter();
        let before = doc.version;
        let res = doc.apply(&set("text.color", json!("#ff0055"))).unwrap();
        assert_eq!(res.version, before + 1);
        assert_eq!(
            doc.comps["main"].layers[2]
                .text
                .as_ref()
                .unwrap()
                .color
                .to_hex(),
            "#ff0055"
        );

        let inverse = res.inverse.expect("inverse");
        doc.apply(&inverse).unwrap();
        assert_eq!(
            doc.comps["main"].layers[2]
                .text
                .as_ref()
                .unwrap()
                .color
                .to_hex(),
            "#f4f6fa"
        );
    }

    #[test]
    fn insert_then_delete_inverts() {
        let mut doc = Doc::starter();
        let mut layer = doc.comps["main"].layers[1].clone();
        layer.id = "extra".into();
        let op: Op = serde_json::from_value(json!({
            "op": "insert-layer", "comp": "main", "layer": layer, "reason": "test"
        }))
        .unwrap();
        let res = doc.apply(&op).unwrap();
        assert_eq!(doc.comps["main"].layers.len(), 4);
        doc.apply(&res.inverse.unwrap()).unwrap();
        assert_eq!(doc.comps["main"].layers.len(), 3);
    }

    #[test]
    fn delete_restores_position() {
        let mut doc = Doc::starter();
        let op: Op = serde_json::from_value(json!({
            "op": "delete-layer", "comp": "main", "layer": "accent", "reason": "test"
        }))
        .unwrap();
        let res = doc.apply(&op).unwrap();
        assert_eq!(doc.comps["main"].layers.len(), 2);
        doc.apply(&res.inverse.unwrap()).unwrap();
        assert_eq!(doc.comps["main"].layers[1].id, "accent");
    }

    #[test]
    fn duplicate_ids_are_refused() {
        let mut doc = Doc::starter();
        let layer = doc.comps["main"].layers[2].clone();
        let op: Op = serde_json::from_value(json!({
            "op": "insert-layer", "comp": "main", "layer": layer, "reason": "test"
        }))
        .unwrap();
        let err = doc.apply(&op).unwrap_err();
        assert_eq!(err.to_json()["code"], "duplicate-layer");
    }

    #[test]
    fn missing_leaf_reports_not_found() {
        let mut doc = Doc::starter();
        let op = set("nope.nope", json!(1));
        let err = doc.apply(&op).unwrap_err();
        assert_eq!(err.to_json()["code"], "not-found");
    }

    #[test]
    fn assets_round_trip_through_the_journal() {
        let mut doc = Doc::starter();
        let op: Op = serde_json::from_value(json!({
            "op": "set-asset",
            "asset": {"id": "logo", "kind": "image", "path": "assets/logo.png", "digest": "ab"},
            "reason": "test"
        }))
        .unwrap();
        let res = doc.apply(&op).unwrap();
        assert!(doc.assets.contains_key("logo"));
        let inverse = res.inverse.expect("a new asset is undone by removing it");
        assert_eq!(inverse.name(), "remove-asset");
        doc.apply(&inverse).unwrap();
        assert!(doc.assets.is_empty());
    }

    #[test]
    fn removing_a_missing_asset_is_not_found() {
        let mut doc = Doc::starter();
        let op: Op = serde_json::from_value(json!({
            "op": "remove-asset", "asset": "nope", "reason": "test"
        }))
        .unwrap();
        assert_eq!(doc.apply(&op).unwrap_err().to_json()["code"], "not-found");
    }

    #[test]
    fn a_look_can_be_set_and_undone() {
        let mut doc = Doc::starter();
        let before = doc.look.clone();
        let op: Op = serde_json::from_value(json!({
            "op": "set-look",
            "look": {"accent": "#f67a1f", "scheme": "light", "scale": {"base": 14, "ratio": 1.333}},
            "reason": "test"
        }))
        .unwrap();
        let res = doc.apply(&op).unwrap();
        let look = doc.look.clone().expect("look set");
        assert_eq!(look.accent, crate::Rgba::opaque(0xf6, 0x7a, 0x1f));
        assert_eq!(look.scale.base, 14.0);
        doc.apply(&res.inverse.unwrap()).unwrap();
        assert_eq!(doc.look, before, "undo restores the previous look");
    }

    #[test]
    fn modifiers_can_be_set_and_undone() {
        let mut doc = Doc::starter();
        let op: Op = serde_json::from_value(json!({
            "op": "set-modifier", "comp": "main", "layer": "accent",
            "modifiers": [{"type": "glow", "radius": 40.0, "strength": 0.9}],
            "reason": "test"
        }))
        .unwrap();
        let res = doc.apply(&op).unwrap();
        let accent = doc.comps["main"]
            .layers
            .iter()
            .find(|l| l.id == "accent")
            .unwrap();
        assert_eq!(accent.modifiers.len(), 1);
        assert_eq!(accent.modifiers[0].kind, "glow");
        doc.apply(&res.inverse.unwrap()).unwrap();
        let accent = doc.comps["main"]
            .layers
            .iter()
            .find(|l| l.id == "accent")
            .unwrap();
        assert!(accent.modifiers.is_empty(), "undo clears the list");
    }

    #[test]
    fn a_comp_can_be_created_and_deleted_with_the_layers_intact() {
        let mut doc = Doc::starter();
        let create: Op = serde_json::from_value(json!({
            "op": "create-comp", "comp": "strap", "size": [1920, 400], "duration": 4.0,
            "layers": doc.comps["main"].layers.clone(), "reason": "test"
        }))
        .unwrap();
        let made = doc.apply(&create).unwrap();
        assert_eq!(doc.comps["strap"].layers.len(), 3);
        let removed = doc.apply(&made.inverse.clone().unwrap()).unwrap();
        assert!(!doc.comps.contains_key("strap"), "delete-comp removes it");
        doc.apply(&removed.inverse.unwrap()).unwrap();
        assert_eq!(
            doc.comps["strap"].layers.len(),
            3,
            "the inverse of delete-comp restores the layers, not an empty comp"
        );
    }

    #[test]
    fn creating_a_comp_that_exists_is_refused() {
        let mut doc = Doc::starter();
        let op: Op = serde_json::from_value(json!({
            "op": "create-comp", "comp": "main", "reason": "test"
        }))
        .unwrap();
        assert_eq!(doc.apply(&op).unwrap_err().to_json()["code"], "exists");
    }

    #[test]
    fn a_matte_can_be_set_and_undone() {
        let mut doc = Doc::starter();
        let op: Op = serde_json::from_value(json!({
            "op": "set-mattes", "comp": "main", "layer": "title",
            "mattes": {"alpha": "accent"}, "reason": "test"
        }))
        .unwrap();
        let res = doc.apply(&op).unwrap();
        let title = doc.comps["main"]
            .layers
            .iter()
            .find(|l| l.id == "title")
            .unwrap();
        assert_eq!(
            title.mattes.as_ref().unwrap().alpha.as_deref(),
            Some("accent")
        );
        assert!(doc.validate().is_empty(), "{:?}", doc.validate());
        doc.apply(&res.inverse.unwrap()).unwrap();
        let title = doc.comps["main"]
            .layers
            .iter()
            .find(|l| l.id == "title")
            .unwrap();
        assert!(title.mattes.is_none(), "undo removes the matte");
    }

    #[test]
    fn a_matte_on_a_missing_layer_is_reported() {
        let mut doc = Doc::starter();
        let op: Op = serde_json::from_value(json!({
            "op": "set-mattes", "comp": "main", "layer": "title",
            "mattes": {"luma": "nope"}, "reason": "test"
        }))
        .unwrap();
        doc.apply(&op).unwrap();
        let problems = doc.validate();
        assert!(
            problems.iter().any(|p| p.contains("nope")),
            "a matte on a layer that is not there must be reported: {problems:?}"
        );
    }

    #[test]
    fn a_gradient_can_be_set_and_undone() {
        let mut doc = Doc::starter();
        let op: Op = serde_json::from_value(json!({
            "op": "set-gradient", "comp": "main", "layer": "bg", "on": "solid",
            "gradient": {"kind": "linear", "angle": 90, "dither": true,
                         "stops": [{"at": 0.0, "color": "#000000"}, {"at": 1.0, "color": "#ffffff"}]},
            "reason": "test"
        }))
        .unwrap();
        let res = doc.apply(&op).unwrap();
        let g = doc.comps["main"].layers[0]
            .solid
            .as_ref()
            .unwrap()
            .gradient
            .clone()
            .expect("gradient set");
        assert!(g.dither);
        assert_eq!(g.stops.len(), 2);
        doc.apply(&res.inverse.unwrap()).unwrap();
        assert!(
            doc.comps["main"].layers[0]
                .solid
                .as_ref()
                .unwrap()
                .gradient
                .is_none(),
            "undo removes a gradient that was not there before"
        );
    }

    #[test]
    fn a_gradient_on_the_wrong_block_is_refused() {
        let mut doc = Doc::starter();
        let op: Op = serde_json::from_value(json!({
            "op": "set-gradient", "comp": "main", "layer": "title", "on": "solid",
            "gradient": {"stops": [{"at": 0.0, "color": "#000000"}, {"at": 1.0, "color": "#ffffff"}]},
            "reason": "test"
        }))
        .unwrap();
        assert_eq!(doc.apply(&op).unwrap_err().to_json()["code"], "no-block");
    }

    #[test]
    fn a_look_can_be_cleared() {
        let mut doc = Doc::starter();
        let op: Op = serde_json::from_value(json!({
            "op": "set-look", "look": null, "reason": "test"
        }))
        .unwrap();
        doc.apply(&op).unwrap();
        assert!(doc.look.is_none());
    }

    #[test]
    fn batch_apply_is_ordered() {
        let mut doc = Doc::starter();
        let ops = vec![set("opacity", json!(0.5)), set("opacity", json!(0.25))];
        doc.apply_all(&ops).unwrap();
        assert_eq!(doc.comps["main"].layers[2].opacity.value, Some(json!(0.25)));
    }
}
