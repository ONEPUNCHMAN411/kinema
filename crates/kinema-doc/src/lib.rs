//! The Kinema document: one typed value that everything else is a projection of.

// The JSON Schema is a deeply nested `json!` literal; the default macro recursion budget
// is not enough for the full document shape.
#![recursion_limit = "256"]

pub mod color;
pub mod model;
pub mod ops;
pub mod prop;
pub mod schema;

pub use color::Rgba;
pub use model::{
    Align, AnimKind, AnimUnit, Asset, Blend, Comp, Doc, EvaluatedLayer, Layer, LayerKind, Licence,
    Matte, Modifier, Shape, ShapeKind, Solid, Text, TextAnim, Transform,
};
pub use ops::{AppliedResult, Op};
pub use prop::{Ease, EvalError, Keyframe, Prop};

use std::fmt;

/// Everything that can go wrong while reading or editing a document.
#[derive(Debug)]
pub enum DocError {
    Json(serde_json::Error),
    NotFound {
        path: String,
        hint: String,
    },
    Invalid {
        code: &'static str,
        message: String,
        hint: String,
    },
    Eval(EvalError),
}

impl fmt::Display for DocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DocError::Json(e) => write!(f, "invalid JSON: {e}"),
            DocError::NotFound { path, hint } => write!(f, "no value at `{path}`: {hint}"),
            DocError::Invalid { code, message, .. } => write!(f, "{code}: {message}"),
            DocError::Eval(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for DocError {}

impl From<serde_json::Error> for DocError {
    fn from(e: serde_json::Error) -> Self {
        DocError::Json(e)
    }
}

impl From<EvalError> for DocError {
    fn from(e: EvalError) -> Self {
        DocError::Eval(e)
    }
}

impl DocError {
    /// The machine-readable shape every `kin` command returns on failure.
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            DocError::Json(e) => serde_json::json!({
                "code": "invalid-json",
                "message": e.to_string(),
                "hint": "check for a trailing comma or an unclosed bracket",
                "fix_op": null,
                "doc": "docs/PLAN.md#document",
            }),
            DocError::NotFound { path, hint } => serde_json::json!({
                "code": "not-found",
                "message": format!("no value at `{path}`"),
                "hint": hint,
                "fix_op": null,
                "doc": "docs/PLAN.md#document",
            }),
            DocError::Invalid {
                code,
                message,
                hint,
            } => serde_json::json!({
                "code": code,
                "message": message,
                "hint": hint,
                "fix_op": null,
                "doc": "docs/PLAN.md#document",
            }),
            DocError::Eval(e) => e.to_json(),
        }
    }
}
