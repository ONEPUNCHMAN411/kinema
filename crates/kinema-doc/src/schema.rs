//! A hand-written JSON Schema for the document.
//!
//! Agents need a schema they can trust, not prose. This is kept in step with the serde
//! model by hand; the `schema_is_internally_consistent` test catches the obvious drift.

use serde_json::{json, Value};

pub fn doc_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://github.com/ONEPUNCHMAN411/kinema/schema/doc-0.1.json",
        "title": "Kinema document",
        "type": "object",
        "required": ["fps", "comps"],
        "properties": {
            "schema": {"const": "kinema/1"},
            "version": {"type": "integer", "minimum": 0, "description": "bumped by every applied op"},
            "fps": {"$ref": "#/$defs/rational"},
            "comps": {
                "type": "object",
                "description": "composition id to comp",
                "additionalProperties": {"$ref": "#/$defs/comp"}
            },
            "assets": {
                "type": "object",
                "additionalProperties": {"$ref": "#/$defs/asset"}
            },
            "look": {"description": "palette, type scale and motion curves"}
        },
        "$defs": {
            "rational": {
                "type": "array", "minItems": 2, "maxItems": 2,
                "items": {"type": "integer"},
                "description": "frame rate as [num, den], e.g. [30000, 1001]"
            },
            "prop": {
                "oneOf": [
                    {"description": "constant"},
                    {"type": "object", "required": ["value"], "properties": {"value": {}}},
                    {"type": "object", "required": ["kf"], "properties": {
                        "kf": {"type": "array", "items": {"type": "array", "minItems": 2, "maxItems": 2,
                               "items": {"description": "[seconds, value]"}}},
                        "ease": {"type": "string", "examples": ["linear", "hold", "bezier(.2,.9,.1,1)"]}
                    }},
                    {"type": "object", "required": ["expr"], "properties": {"expr": {"type": "string"}}},
                    {"type": "object", "required": ["link"], "properties": {"link": {"type": "string",
                        "examples": ["comp:main/layer:src/opacity"]}}},
                    {"type": "object", "required": ["graph"], "properties": {"graph": {"type": "string"}}}
                ]
            },
            "comp": {
                "type": "object",
                "required": ["size", "duration", "layers"],
                "properties": {
                    "size": {"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": "integer", "minimum": 1}},
                    "duration": {"type": "number", "exclusiveMinimum": 0, "description": "seconds"},
                    "shutter": {"type": "number", "minimum": 0, "maximum": 360, "description": "shutter angle in degrees; 180 is film default, 0 disables motion blur"},
                    "samples": {"type": "integer", "minimum": 2, "maximum": 64, "description": "subframe samples when shutter > 0"},
                    "layers": {"type": "array", "items": {"$ref": "#/$defs/layer"}}
                }
            },
            "layer": {
                "type": "object",
                "required": ["id", "kind"],
                "properties": {
                    "id": {"type": "string", "pattern": "^[A-Za-z0-9_-]+$"},
                    "kind": {"enum": ["solid", "text", "shape", "image", "comp", "adjustment"]},
                    "start": {"type": "number", "description": "seconds"},
                    "duration": {"type": "number", "description": "seconds"},
                    "transform": {
                        "type": "object",
                        "properties": {
                            "position": {"$ref": "#/$defs/prop"},
                            "scale": {"$ref": "#/$defs/prop"},
                            "rotation": {"$ref": "#/$defs/prop"},
                            "anchor": {"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": "number"}}
                        }
                    },
                    "opacity": {"$ref": "#/$defs/prop"},
                    "blend": {"enum": ["normal", "screen", "multiply", "overlay", "add"]},
                    "solid": {"type": "object", "properties": {"color": {"$ref": "#/$defs/colour"}}},
                    "text": {
                        "type": "object",
                        "required": ["string", "size", "color"],
                        "properties": {
                            "string": {"type": "string"},
                            "size": {"type": "number", "exclusiveMinimum": 0},
                            "font": {"type": "string"},
                            "color": {"$ref": "#/$defs/colour"},
                            "weight": {"type": "integer"},
                            "italic": {"type": "boolean"},
                            "tracking": {"type": "number"}
                        }
                    },
                    "shape": {
                        "type": "object",
                        "required": ["size", "color"],
                        "properties": {
                            "kind": {"enum": ["rect", "ellipse"]},
                            "size": {"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": "number"}},
                            "color": {"$ref": "#/$defs/colour"},
                            "radius": {"type": "number"}
                        }
                    },
                    "mattes": {"type": "object", "properties": {
                        "alpha": {"type": "string"}, "luma": {"type": "string"}}},
                    "modifiers": {"type": "array", "items": {"type": "object", "required": ["type"],
                        "properties": {"type": {"type": "string"}}}},
                    "source": {"type": "string", "description": "asset id for image and comp layers"}
                }
            },
            "asset": {
                "type": "object",
                "required": ["id", "kind", "path"],
                "properties": {
                    "id": {"type": "string"},
                    "kind": {"enum": ["image", "video", "audio", "font", "model", "hdri"]},
                    "path": {"type": "string"},
                    "digest": {"type": "string", "description": "blake3 of the file"},
                    "licence": {"$ref": "#/$defs/licence"}
                }
            },
            "licence": {
                "type": "object",
                "required": ["spdx", "source_url", "author", "retrieved_at"],
                "description": "never guessed; `unknown` blocks a release render",
                "properties": {
                    "spdx": {"type": "string", "examples": ["CC0-1.0", "CC-BY-4.0", "OFL-1.1", "MIT", "unknown"]},
                    "source_url": {"type": "string"},
                    "author": {"type": "string"},
                    "retrieved_at": {"type": "string"},
                    "attribution_required": {"type": "boolean"},
                    "commercial_ok": {"type": "boolean"},
                    "derivatives_ok": {"type": "boolean"},
                    "share_alike": {"type": "boolean"},
                    "model_release": {"type": "boolean"}
                }
            },
            "colour": {
                "type": "string",
                "pattern": "^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$"
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_is_a_valid_object() {
        let s = doc_schema();
        assert_eq!(s["type"], "object");
        for def in [
            "rational", "prop", "comp", "layer", "asset", "licence", "colour",
        ] {
            assert!(s["$defs"].get(def).is_some(), "missing $defs/{def}");
        }
    }

    #[test]
    fn starter_document_satisfies_required_fields() {
        let doc = crate::Doc::starter();
        let v = serde_json::to_value(&doc).unwrap();
        for key in doc_schema()["required"].as_array().unwrap() {
            assert!(
                v.get(key.as_str().unwrap()).is_some(),
                "starter missing {key}"
            );
        }
        assert_eq!(v["schema"], "kinema/1");
    }
}
