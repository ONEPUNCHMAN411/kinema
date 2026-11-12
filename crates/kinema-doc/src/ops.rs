//! Typed ops, their inverses, and path addressing.
//!
//! Every mutation is an `Op` that can be inverted, so the journal gives undo, diff and
//! provenance for free. A path looks like `comp:main/layer:title/transform.position`.

use crate::model::{Asset, Layer};
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
}

impl Op {
    pub fn reason(&self) -> &str {
        match self {
            Op::SetProp { reason, .. }
            | Op::InsertLayer { reason, .. }
            | Op::DeleteLayer { reason, .. }
            | Op::SetComp { reason, .. }
            | Op::SetAsset { reason, .. } => reason,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Op::SetProp { .. } => "set-prop",
            Op::InsertLayer { .. } => "insert-layer",
            Op::DeleteLayer { .. } => "delete-layer",
            Op::SetComp { .. } => "set-comp",
            Op::SetAsset { .. } => "set-asset",
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
            None => Ok(None),
        },
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
             "properties": {"op": {"const": "set-asset"}, "asset": {"type": "object"}, "reason": {"type": "string"}}}
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
    fn batch_apply_is_ordered() {
        let mut doc = Doc::starter();
        let ops = vec![set("opacity", json!(0.5)), set("opacity", json!(0.25))];
        doc.apply_all(&ops).unwrap();
        assert_eq!(doc.comps["main"].layers[2].opacity.value, Some(json!(0.25)));
    }
}
