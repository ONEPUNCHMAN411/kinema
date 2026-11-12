//! Content addressing.
//!
//! A frame is a pure function of the document, so it can be named by a hash of
//! everything it depends on. That is what makes "re-render only what changed" correct
//! rather than a guess: the cache key is derived, never declared.

use kinema_doc::Doc;
use serde_json::Value;

/// Deterministic JSON: object keys sorted, no whitespace.
pub fn canonical(v: &Value) -> String {
    let mut out = String::new();
    write_canonical(v, &mut out);
    out
}

fn write_canonical(v: &Value, out: &mut String) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&n.to_string()),
        Value::String(s) => {
            out.push('"');
            for ch in s.chars() {
                match ch {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                    c => out.push(c),
                }
            }
            out.push('"');
        }
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push('"');
                out.push_str(k);
                out.push_str("\":");
                write_canonical(&map[k], out);
            }
            out.push('}');
        }
    }
}

pub fn hash_bytes(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// The identity of a whole document.
pub fn hash_doc(doc: &Doc) -> String {
    match serde_json::to_value(doc) {
        Ok(v) => {
            // The op counter is bookkeeping, not content.
            let mut v = v;
            if let Some(obj) = v.as_object_mut() {
                obj.remove("version");
            }
            hash_bytes(canonical(&v).as_bytes())
        }
        Err(_) => hash_bytes(b"<unserialisable>"),
    }
}

/// The identity of one layer inside one comp.
pub fn hash_layer(doc: &Doc, comp: &str, layer: &str) -> String {
    let mut v = serde_json::to_value(doc).unwrap_or(Value::Null);
    if let Some(obj) = v.as_object_mut() {
        obj.remove("version");
    }
    if let Some(c) = v.get_mut("comps").and_then(|c| c.get_mut(comp)) {
        if let Some(layers) = c.get_mut("layers").and_then(|l| l.as_array_mut()) {
            layers.retain(|l| l.get("id").and_then(|i| i.as_str()) == Some(layer));
        }
    }
    hash_bytes(canonical(&v).as_bytes())
}

/// The identity of a rendered frame: the document plus the comp, frame and tier.
pub fn hash_frame(doc: &Doc, comp: &str, frame: u32, tier: &str) -> String {
    let mut v = serde_json::to_value(doc).unwrap_or(Value::Null);
    if let Some(obj) = v.as_object_mut() {
        obj.remove("version");
    }
    let payload = serde_json::json!({
        "doc": v,
        "comp": comp,
        "frame": frame,
        "tier": tier,
        "engine": env!("CARGO_PKG_VERSION"),
    });
    hash_bytes(canonical(&payload).as_bytes())
}

/// A short prefix, for logs and manifests.
pub fn short(hash: &str) -> &str {
    &hash[..hash.len().min(12)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canonical_json_sorts_keys() {
        let a = canonical(&json!({"b": 1, "a": 2}));
        let b = canonical(&json!({"a": 2, "b": 1}));
        assert_eq!(a, b);
        assert_eq!(a, r#"{"a":2,"b":1}"#);
    }

    #[test]
    fn hashes_are_stable_and_distinct() {
        let doc = Doc::starter();
        let h1 = hash_doc(&doc);
        let h2 = hash_doc(&doc);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);

        let mut other = Doc::starter();
        other.comps.get_mut("main").unwrap().layers[2].opacity = kinema_doc::Prop::number(0.5);
        assert_ne!(hash_doc(&doc), hash_doc(&other));
    }

    #[test]
    fn version_does_not_change_the_hash() {
        let mut doc = Doc::starter();
        let before = hash_doc(&doc);
        doc.version = 99;
        assert_eq!(before, hash_doc(&doc));
    }

    #[test]
    fn layer_and_frame_hashes_differ() {
        let doc = Doc::starter();
        let l1 = hash_layer(&doc, "main", "title");
        let l2 = hash_layer(&doc, "main", "bg");
        assert_ne!(l1, l2);
        let f0 = hash_frame(&doc, "main", 0, "gpu");
        let f1 = hash_frame(&doc, "main", 1, "gpu");
        assert_ne!(f0, f1);
        assert_ne!(f0, hash_frame(&doc, "main", 0, "strict"));
    }
}
