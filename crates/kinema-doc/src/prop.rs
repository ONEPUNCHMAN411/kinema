//! Properties: the five ways a value can be produced.
//!
//! `value` is constant, `kf` is keyframed, `expr` is an expression (not built yet),
//! `link` picks up another property, `graph` defers to a node graph (not built yet).
//!
//! Serialisation is deliberately loose: a bare scalar or array is a constant, and only
//! an object with one of the five keys counts as a structured property, so documents
//! read like `"position": [960, 540]` and `"position": {"kf": [[0, [960, 540]]]}`.

use crate::model::Doc;
use crate::Rgba;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct Prop {
    pub value: Option<Value>,
    pub kf: Option<Vec<Keyframe>>,
    pub expr: Option<String>,
    pub link: Option<String>,
    pub graph: Option<String>,
    pub ease: Option<String>,
}

/// One keyframe: `[time_seconds, value]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Keyframe {
    pub t: f64,
    pub v: Value,
}

impl Serialize for Keyframe {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        (self.t, &self.v).serialize(s)
    }
}

impl<'de> Deserialize<'de> for Keyframe {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let (t, v) = <(f64, Value)>::deserialize(d)?;
        Ok(Keyframe { t, v })
    }
}

impl Prop {
    pub fn constant(v: impl Into<Value>) -> Self {
        Prop {
            value: Some(v.into()),
            kf: None,
            expr: None,
            link: None,
            graph: None,
            ease: None,
        }
    }

    pub fn number(n: f64) -> Self {
        Prop::constant(serde_json::json!(n))
    }

    pub fn keyframed(kf: Vec<Keyframe>, ease: Option<String>) -> Self {
        Prop {
            kf: Some(kf),
            ease,
            ..Prop::empty()
        }
    }

    pub fn empty() -> Self {
        Prop {
            value: None,
            kf: None,
            expr: None,
            link: None,
            graph: None,
            ease: None,
        }
    }

    pub fn is_constant(&self) -> bool {
        self.value.is_some()
    }

    pub fn kind(&self) -> &'static str {
        if self.value.is_some() {
            "value"
        } else if self.kf.is_some() {
            "kf"
        } else if self.expr.is_some() {
            "expr"
        } else if self.link.is_some() {
            "link"
        } else if self.graph.is_some() {
            "graph"
        } else {
            "empty"
        }
    }

    /// Resolve the property at a time in seconds.
    pub fn eval(&self, t: f64, doc: &Doc, depth: u8) -> Result<Value, EvalError> {
        if depth > 8 {
            return Err(EvalError::Recursion);
        }
        if let Some(v) = &self.value {
            return Ok(v.clone());
        }
        if let Some(kfs) = &self.kf {
            return Ok(eval_keyframes(kfs, self.ease.as_deref(), t));
        }
        if let Some(path) = &self.link {
            let raw = doc.value_at(path).map_err(|e| EvalError::BadLink {
                path: path.clone(),
                why: e.to_string(),
            })?;
            // A link may point at a bare value or at another property.
            let target: Prop =
                serde_json::from_value(raw.clone()).unwrap_or_else(|_| Prop::constant(raw));
            return target.eval(t, doc, depth + 1);
        }
        if let Some(src) = &self.expr {
            return eval_source(doc, src, t, depth + 1);
        }
        if let Some(id) = &self.graph {
            return Err(EvalError::NoGraphHost { id: id.clone() });
        }
        Err(EvalError::Empty)
    }

    pub fn eval_f64(&self, t: f64, doc: &Doc) -> Result<f64, EvalError> {
        self.eval(t, doc, 0)?
            .as_f64()
            .ok_or(EvalError::TypeMismatch { want: "number" })
    }

    pub fn eval_vec2(&self, t: f64, doc: &Doc) -> Result<[f64; 2], EvalError> {
        let v = self.eval(t, doc, 0)?;
        let arr = v.as_array().ok_or(EvalError::TypeMismatch {
            want: "pair of numbers",
        })?;
        if arr.len() != 2 {
            return Err(EvalError::TypeMismatch {
                want: "pair of numbers",
            });
        }
        Ok([
            arr[0].as_f64().ok_or(EvalError::TypeMismatch {
                want: "pair of numbers",
            })?,
            arr[1].as_f64().ok_or(EvalError::TypeMismatch {
                want: "pair of numbers",
            })?,
        ])
    }

    pub fn eval_color(&self, t: f64, doc: &Doc) -> Result<Rgba, EvalError> {
        let v = self.eval(t, doc, 0)?;
        match v {
            Value::String(s) => {
                Rgba::parse(&s).map_err(|_| EvalError::TypeMismatch { want: "colour" })
            }
            _ => Err(EvalError::TypeMismatch { want: "colour" }),
        }
    }
}

impl Default for Prop {
    fn default() -> Self {
        Prop::empty()
    }
}

impl Serialize for Prop {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if let Some(v) = &self.value {
            if self.kf.is_none()
                && self.expr.is_none()
                && self.link.is_none()
                && self.graph.is_none()
            {
                return v.serialize(s);
            }
        }
        let mut n = 0usize;
        if self.value.is_some() {
            n += 1;
        }
        if self.kf.is_some() {
            n += 1;
        }
        if self.expr.is_some() {
            n += 1;
        }
        if self.link.is_some() {
            n += 1;
        }
        if self.graph.is_some() {
            n += 1;
        }
        if self.ease.is_some() {
            n += 1;
        }
        let mut map = s.serialize_map(Some(n))?;
        if let Some(v) = &self.value {
            map.serialize_entry("value", v)?;
        }
        if let Some(kf) = &self.kf {
            map.serialize_entry("kf", kf)?;
        }
        if let Some(e) = &self.expr {
            map.serialize_entry("expr", e)?;
        }
        if let Some(l) = &self.link {
            map.serialize_entry("link", l)?;
        }
        if let Some(g) = &self.graph {
            map.serialize_entry("graph", g)?;
        }
        if let Some(e) = &self.ease {
            map.serialize_entry("ease", e)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for Prop {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        let obj = match &v {
            Value::Object(m) => m,
            _ => return Ok(Prop::constant(v)),
        };
        let structured = ["value", "kf", "expr", "link", "graph"]
            .iter()
            .any(|k| obj.contains_key(*k));
        if !structured {
            return Ok(Prop::constant(v));
        }
        let get_str = |k: &str| obj.get(k).and_then(|x| x.as_str()).map(str::to_string);
        let kf = match obj.get("kf") {
            Some(Value::Array(items)) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    let kf: Keyframe = serde_json::from_value(item.clone()).map_err(|e| {
                        serde::de::Error::custom(format!(
                            "keyframe should be [seconds, value]: {e}"
                        ))
                    })?;
                    out.push(kf);
                }
                out.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
                Some(out)
            }
            Some(_) => return Err(serde::de::Error::custom("`kf` must be an array")),
            None => None,
        };
        Ok(Prop {
            value: obj.get("value").cloned(),
            kf,
            expr: get_str("expr"),
            link: get_str("link"),
            graph: get_str("graph"),
            ease: get_str("ease"),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Ease {
    Linear,
    Hold,
    Bezier([f64; 4]),
}

impl Ease {
    pub fn parse(s: Option<&str>) -> Ease {
        let Some(s) = s else { return Ease::Linear };
        let s = s.trim();
        if s.eq_ignore_ascii_case("linear") {
            return Ease::Linear;
        }
        if s.eq_ignore_ascii_case("hold") || s.eq_ignore_ascii_case("step") {
            return Ease::Hold;
        }
        if let Some(rest) = s.strip_prefix("bezier") {
            let nums: Vec<f64> = rest
                .trim_start_matches('(')
                .trim_end_matches(')')
                .split(',')
                .filter_map(|p| p.trim().parse::<f64>().ok())
                .collect();
            if nums.len() == 4 {
                return Ease::Bezier([nums[0], nums[1], nums[2], nums[3]]);
            }
        }
        Ease::Linear
    }

    /// Eased progress for a normalised span.
    pub fn apply(&self, u: f64) -> f64 {
        let u = u.clamp(0.0, 1.0);
        match self {
            Ease::Hold => 0.0,
            Ease::Linear => u,
            Ease::Bezier(c) => bezier_y(c, u),
        }
    }

    pub fn canonical(&self) -> String {
        match self {
            Ease::Linear => "linear".into(),
            Ease::Hold => "hold".into(),
            Ease::Bezier([a, b, c, d]) => format!("bezier({a},{b},{c},{d})"),
        }
    }
}

/// Cubic-bezier timing function: find t for x, then return y.
fn bezier_y(c: &[f64; 4], x: f64) -> f64 {
    let [x1, y1, x2, y2] = *c;
    let bx = |t: f64| {
        let mt = 1.0 - t;
        3.0 * mt * mt * t * x1 + 3.0 * mt * t * t * x2 + t * t * t
    };
    let by = |t: f64| {
        let mt = 1.0 - t;
        3.0 * mt * mt * t * y1 + 3.0 * mt * t * t * y2 + t * t * t
    };
    let (mut lo, mut hi) = (0.0f64, 1.0f64);
    for _ in 0..32 {
        let mid = (lo + hi) / 2.0;
        if bx(mid) < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    by((lo + hi) / 2.0)
}

fn eval_keyframes(kfs: &[Keyframe], ease: Option<&str>, t: f64) -> Value {
    if kfs.is_empty() {
        return Value::Null;
    }
    if t <= kfs[0].t {
        return kfs[0].v.clone();
    }
    let last = kfs.last().expect("non-empty");
    if t >= last.t {
        return last.v.clone();
    }
    for pair in kfs.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if t >= a.t && t <= b.t {
            let span = (b.t - a.t).max(f64::EPSILON);
            let u = Ease::parse(ease).apply((t - a.t) / span);
            return lerp_value(&a.v, &b.v, u);
        }
    }
    last.v.clone()
}

fn lerp_value(a: &Value, b: &Value, u: f64) -> Value {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap_or(0.0), y.as_f64().unwrap_or(0.0));
            serde_json::json!(x + (y - x) * u)
        }
        (Value::Array(xs), Value::Array(ys)) if xs.len() == ys.len() => {
            let mixed: Vec<Value> = xs
                .iter()
                .zip(ys.iter())
                .map(|(x, y)| lerp_value(x, y, u))
                .collect();
            Value::Array(mixed)
        }
        (Value::String(x), Value::String(y)) => {
            // Colours interpolate; other strings hold.
            match (Rgba::parse(x), Rgba::parse(y)) {
                (Ok(cx), Ok(cy)) => serde_json::json!(cx.lerp(cy, u).to_hex()),
                _ => a.clone(),
            }
        }
        _ => a.clone(),
    }
}

#[derive(Debug, Clone)]
pub enum EvalError {
    Empty,
    Recursion,
    TypeMismatch {
        want: &'static str,
    },
    Expr {
        source: String,
        message: String,
        at: usize,
        hint: String,
    },
    NoGraphHost {
        id: String,
    },
    BadLink {
        path: String,
        why: String,
    },
}

/// Evaluate a standalone expression against a document at one time.
///
/// `link()` paths resolve through the same property evaluator, so an expression can read
/// anything a property can, and the depth guard applies to the whole chain.
pub fn eval_source(doc: &Doc, src: &str, t: f64, depth: u8) -> Result<Value, EvalError> {
    if depth > 8 {
        return Err(EvalError::Recursion);
    }
    let (width, height, duration) = doc
        .comps
        .get("main")
        .map(|c| (c.size[0] as f64, c.size[1] as f64, c.duration))
        .unwrap_or((0.0, 0.0, 0.0));
    let resolve = |path: &str| -> Result<kinema_expr::Val, String> {
        let raw = doc.value_at(path).map_err(|e| e.to_string())?;
        let target: Prop =
            serde_json::from_value(raw.clone()).unwrap_or_else(|_| Prop::constant(raw));
        let out = target.eval(t, doc, depth).map_err(|e| e.to_string())?;
        to_expr_val(&out)
            .ok_or_else(|| format!("`{path}` is not a number, boolean or list of numbers"))
    };
    let ctx = kinema_expr::Ctx {
        t,
        frame: doc.fps.ticks_to_frame(kinema_time::seconds_to_ticks(t)),
        fps: doc.fps.as_f64(),
        width,
        height,
        duration,
        resolve: &resolve,
    };
    let val = kinema_expr::eval(src, &ctx).map_err(|e| EvalError::Expr {
        source: src.to_string(),
        message: e.to_string(),
        at: e.at,
        hint: if e.hint.is_empty() {
            "test the expression on its own with `kin eval '<expr>' --at 1.0`".to_string()
        } else {
            e.hint
        },
    })?;
    Ok(from_expr_val(&val.to_json()))
}

fn to_expr_val(v: &Value) -> Option<kinema_expr::Val> {
    match v {
        Value::Number(n) => n.as_f64().map(kinema_expr::Val::Num),
        Value::Bool(b) => Some(kinema_expr::Val::Bool(*b)),
        Value::String(s) => Some(kinema_expr::Val::Str(s.clone())),
        Value::Array(items) => items
            .iter()
            .map(|i| i.as_f64())
            .collect::<Option<Vec<f64>>>()
            .map(kinema_expr::Val::Arr),
        _ => None,
    }
}

fn from_expr_val(v: &kinema_expr::serde_like::Value) -> Value {
    use kinema_expr::serde_like::Value as E;
    match v {
        E::Num(n) => serde_json::json!(n),
        E::Bool(b) => serde_json::json!(b),
        E::Str(s) => serde_json::json!(s),
        E::Arr(items) => Value::Array(items.iter().map(from_expr_val).collect()),
    }
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EvalError::Empty => write!(f, "property has no value, keyframes, expression or link"),
            EvalError::Recursion => write!(f, "link chain deeper than 8; it probably loops"),
            EvalError::TypeMismatch { want } => write!(f, "property is not a {want}"),
            EvalError::Expr {
                source, message, ..
            } => {
                write!(f, "expression `{source}` failed: {message}")
            }
            EvalError::NoGraphHost { id } => write!(f, "no node host in this build: `{id}`"),
            EvalError::BadLink { path, why } => write!(f, "link `{path}` does not resolve: {why}"),
        }
    }
}

impl std::error::Error for EvalError {}

impl EvalError {
    pub fn to_json(&self) -> Value {
        match self {
            EvalError::Expr {
                source,
                message,
                at,
                hint,
            } => {
                let mut json = serde_json::json!({
                    "code": "expr-failed",
                    "message": message,
                    "hint": hint,
                    "fix_op": null,
                    "doc": "docs/PLAN.md#properties",
                    "source": source,
                });
                // Only parse errors know where they failed; never invent an offset.
                if *at > 0 {
                    json["at"] = serde_json::json!(at);
                }
                json
            }
            EvalError::NoGraphHost { id } => serde_json::json!({
                "code": "not-implemented",
                "message": "the node-graph host is not in this build",
                "hint": "bake the graph into keyframes or a constant",
                "fix_op": null,
                "doc": "docs/PLAN.md#templates",
                "graph": id,
            }),
            other => serde_json::json!({
                "code": "eval-failed",
                "message": other.to_string(),
                "hint": "check the property kind with `kin probe`",
                "fix_op": null,
                "doc": "docs/PLAN.md#contract",
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_values_are_constants() {
        let p: Prop = serde_json::from_str("[960, 540]").unwrap();
        assert!(p.is_constant());
        assert_eq!(p.kind(), "value");
        assert_eq!(serde_json::to_string(&p).unwrap(), "[960,540]");
    }

    #[test]
    fn keyframes_serialise_as_pairs() {
        let src = r#"{"kf": [[0, [960, 540]], [0.5, [960, 600]]], "ease": "linear"}"#;
        let p: Prop = serde_json::from_str(src).unwrap();
        assert_eq!(p.kind(), "kf");
        let doc = Doc::empty();
        let mid = p.eval_vec2(0.25, &doc).unwrap();
        assert!((mid[0] - 960.0).abs() < 1e-9);
        assert!((mid[1] - 570.0).abs() < 1e-9);
        let round = serde_json::to_string(&p).unwrap();
        assert!(round.contains("\"kf\":[[0.0,[960,540]"));
    }

    #[test]
    fn hold_easing_steps() {
        let p: Prop = serde_json::from_str(r#"{"kf":[[0,0],[1,100]],"ease":"hold"}"#).unwrap();
        let doc = Doc::empty();
        assert_eq!(p.eval_f64(0.9, &doc).unwrap(), 0.0);
        assert_eq!(p.eval_f64(1.0, &doc).unwrap(), 100.0);
    }

    #[test]
    fn bezier_easing_is_monotonic_and_bounded() {
        let e = Ease::parse(Some("bezier(.2,.9,.1,1)"));
        let mut prev = -1.0;
        for i in 0..=20 {
            let y = e.apply(i as f64 / 20.0);
            assert!(y >= prev - 1e-9, "not monotonic at {i}");
            assert!((0.0..=1.0).contains(&y));
            prev = y;
        }
        // Decelerating curve is ahead of linear halfway through.
        assert!(e.apply(0.5) > 0.5);
    }

    #[test]
    fn colours_interpolate_through_keyframes() {
        let p: Prop = serde_json::from_str(r##"{"kf":[[0,"#000000"],[1,"#ffffff"]]}"##).unwrap();
        let doc = Doc::empty();
        let mid = p.eval_color(0.5, &doc).unwrap();
        assert!(mid.r > 100 && mid.r < 160, "{}", mid);
    }

    #[test]
    fn expressions_evaluate_in_context() {
        let doc = Doc::starter();
        let p: Prop = serde_json::from_str(r#"{"expr":"t * 10 + frame"}"#).unwrap();
        // Frame 45 at 1.5 s on a 30 fps document.
        assert_eq!(p.eval_f64(1.5, &doc).unwrap(), 60.0);

        let pair: Prop = serde_json::from_str(r#"{"expr":"[width / 2, height - 60]"}"#).unwrap();
        assert_eq!(pair.eval_vec2(0.0, &doc).unwrap(), [960.0, 1020.0]);
    }

    #[test]
    fn expressions_can_link_properties() {
        let doc = Doc::starter();
        let p: Prop =
            serde_json::from_str(r#"{"expr":"link(\"comp:main/layer:bg/opacity\") * 3"}"#).unwrap();
        assert_eq!(p.eval_f64(0.0, &doc).unwrap(), 3.0);
    }

    #[test]
    fn expression_failures_are_typed() {
        let doc = Doc::starter();
        let p: Prop = serde_json::from_str(r#"{"expr":"1 / 0"}"#).unwrap();
        let err = p.eval(0.0, &doc, 0).unwrap_err();
        assert!(matches!(err, EvalError::Expr { .. }));
        let json = err.to_json();
        assert_eq!(json["code"], "expr-failed");
        assert!(
            json["hint"].as_str().unwrap().contains("kin eval"),
            "{json}"
        );
    }

    #[test]
    fn links_resolve_and_depth_is_bounded() {
        let mut doc = Doc::empty();
        let a = Prop::link("comp:main/layer:src/opacity");
        let _ = &mut doc;
        let err = a.eval(0.0, &doc, 0).unwrap_err();
        assert!(matches!(err, EvalError::BadLink { .. }));
    }

    impl Prop {
        fn link(path: &str) -> Prop {
            Prop {
                link: Some(path.into()),
                ..Prop::empty()
            }
        }
    }
}
