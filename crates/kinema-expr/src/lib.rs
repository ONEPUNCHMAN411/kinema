//! The expression language.
//!
//! A property may be `{"expr": "…"}`. Expressions are pure functions of the evaluation
//! context: `t` seconds, `frame`, `fps`, `width`, `height`, `duration`, and any other
//! property via `link("comp:main/layer:bg/opacity")`.
//!
//! No loops, no assignment, no I/O, bounded source, bounded depth, bounded steps. A
//! document can be hostile; an expression cannot hang the renderer.

pub mod syntax;

use std::fmt;

pub use syntax::{Expr, ExprError};

#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Num(f64),
    Bool(bool),
    Str(String),
    Arr(Vec<f64>),
}

impl Val {
    pub fn num(n: f64) -> Val {
        Val::Num(n)
    }

    pub fn to_num(&self) -> Result<f64, String> {
        match self {
            Val::Num(n) => Ok(*n),
            Val::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
            Val::Str(s) => Err(format!("`{s}` is text, not a number")),
            Val::Arr(_) => Err("a list is not a number; index it with x() or y()".to_string()),
        }
    }

    pub fn truthy(&self) -> bool {
        match self {
            Val::Num(n) => *n != 0.0,
            Val::Bool(b) => *b,
            Val::Str(s) => !s.is_empty(),
            Val::Arr(a) => !a.is_empty(),
        }
    }

    pub fn to_json(&self) -> serde_like::Value {
        serde_like::from_val(self)
    }
}

impl fmt::Display for Val {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Val::Num(n) => write!(f, "{n}"),
            Val::Bool(b) => write!(f, "{b}"),
            Val::Str(s) => write!(f, "\"{s}\""),
            Val::Arr(a) => {
                write!(f, "[")?;
                for (i, v) in a.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, "]")
            }
        }
    }
}

/// A tiny local stand-in for JSON so this crate stays dependency-free.
/// The document crate converts through its own serde types.
pub mod serde_like {
    use super::Val;

    #[derive(Debug, Clone, PartialEq)]
    pub enum Value {
        Num(f64),
        Bool(bool),
        Str(String),
        Arr(Vec<Value>),
    }

    pub fn from_val(v: &Val) -> Value {
        match v {
            Val::Num(n) => Value::Num(*n),
            Val::Bool(b) => Value::Bool(*b),
            Val::Str(s) => Value::Str(s.clone()),
            Val::Arr(a) => Value::Arr(a.iter().map(|n| Value::Num(*n)).collect()),
        }
    }
}

/// Everything an expression may read. `resolve` is how `link()` reaches the document.
pub struct Ctx<'a> {
    pub t: f64,
    pub frame: u32,
    pub fps: f64,
    pub width: f64,
    pub height: f64,
    pub duration: f64,
    pub resolve: &'a dyn Fn(&str) -> Result<Val, String>,
}

const MAX_STEPS: usize = 20_000;

struct Eval {
    steps: usize,
    depth: usize,
}

pub fn eval(src: &str, ctx: &Ctx) -> Result<Val, ExprError> {
    let ast = syntax::parse(src)?;
    let mut e = Eval { steps: 0, depth: 0 };
    // Runtime failures carry no byte offset: the evaluator has no spans. Parse errors do,
    // which is where an offset actually helps.
    e.node(&ast, ctx).map_err(|m| ExprError {
        kind: "eval",
        message: m,
        at: 0,
        hint: String::new(),
    })
}

impl Eval {
    fn node(&mut self, e: &Expr, ctx: &Ctx) -> Result<Val, String> {
        self.steps += 1;
        self.depth += 1;
        if self.steps > MAX_STEPS {
            return Err(format!("expression exceeded {MAX_STEPS} steps"));
        }
        if self.depth > syntax::MAX_DEPTH {
            return Err(format!(
                "evaluation nested deeper than {}",
                syntax::MAX_DEPTH
            ));
        }
        let out = self.node_inner(e, ctx);
        self.depth -= 1;
        out
    }

    fn node_inner(&mut self, e: &Expr, ctx: &Ctx) -> Result<Val, String> {
        match e {
            Expr::Num(n) => Ok(Val::Num(*n)),
            Expr::Str(s) => Ok(Val::Str(s.clone())),
            Expr::Bool(b) => Ok(Val::Bool(*b)),
            Expr::Var(name) => match name.as_str() {
                "t" | "time" => Ok(Val::Num(ctx.t)),
                "frame" => Ok(Val::Num(ctx.frame as f64)),
                "fps" => Ok(Val::Num(ctx.fps)),
                "width" | "w" => Ok(Val::Num(ctx.width)),
                "height" | "h" => Ok(Val::Num(ctx.height)),
                "duration" => Ok(Val::Num(ctx.duration)),
                "PI" => Ok(Val::Num(std::f64::consts::PI)),
                "TAU" => Ok(Val::Num(std::f64::consts::TAU)),
                "E" => Ok(Val::Num(std::f64::consts::E)),
                other => Err(format!(
                    "`{other}` is not a name here; try t, frame, fps, width, height, duration, PI, TAU, E, or a function"
                )),
            },
            Expr::Array(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    out.push(self.node(item, ctx)?.to_num()?);
                }
                Ok(Val::Arr(out))
            }
            Expr::Unary { op, rhs } => {
                let v = self.node(rhs, ctx)?;
                match op {
                    syntax::UnOp::Neg => Ok(Val::Num(-v.to_num()?)),
                    syntax::UnOp::Pos => Ok(Val::Num(v.to_num()?)),
                    syntax::UnOp::Not => Ok(Val::Bool(!v.truthy())),
                }
            }
            Expr::Ternary { cond, yes, no } => {
                if self.node(cond, ctx)?.truthy() {
                    self.node(yes, ctx)
                } else {
                    self.node(no, ctx)
                }
            }
            Expr::Binary { op, lhs, rhs } => {
                use syntax::BinOp::*;
                // Short circuit before evaluating the right side.
                if *op == And {
                    return Ok(Val::Bool(self.node(lhs, ctx)?.truthy() && self.node(rhs, ctx)?.truthy()));
                }
                if *op == Or {
                    return Ok(Val::Bool(self.node(lhs, ctx)?.truthy() || self.node(rhs, ctx)?.truthy()));
                }
                let a = self.node(lhs, ctx)?;
                let b = self.node(rhs, ctx)?;
                let out = binary(*op, &a, &b)?;
                // Overflow to infinity is a bug in the document, not a value: refuse it
                // here so it cannot silently reach the rasteriser.
                match &out {
                    Val::Num(n) if !n.is_finite() => {
                        Err("result is not a finite number".to_string())
                    }
                    Val::Arr(v) if v.iter().any(|x| !x.is_finite()) => {
                        Err("a list component is not a finite number".to_string())
                    }
                    _ => Ok(out),
                }
            }
            Expr::Call { name, args, .. } => {
                let mut vals = Vec::with_capacity(args.len());
                for a in args {
                    vals.push(self.node(a, ctx)?);
                }
                call(name, &vals, ctx)
            }
        }
    }
}

fn finite(n: f64) -> Result<Val, String> {
    if n.is_finite() {
        Ok(Val::Num(n))
    } else {
        Err("result is not a finite number".to_string())
    }
}

fn zip_maps(a: &Val, b: &Val, f: impl Fn(f64, f64) -> Result<f64, String>) -> Result<Val, String> {
    match (a, b) {
        (Val::Arr(xs), Val::Arr(ys)) => {
            if xs.len() != ys.len() {
                return Err(format!(
                    "lists must be the same length: {} and {}",
                    xs.len(),
                    ys.len()
                ));
            }
            let mut out = Vec::with_capacity(xs.len());
            for (x, y) in xs.iter().zip(ys.iter()) {
                out.push(f(*x, *y)?);
            }
            Ok(Val::Arr(out))
        }
        (Val::Arr(xs), other) => {
            let k = other.to_num()?;
            let mut out = Vec::with_capacity(xs.len());
            for x in xs {
                out.push(f(*x, k)?);
            }
            Ok(Val::Arr(out))
        }
        (other, Val::Arr(ys)) => {
            let k = other.to_num()?;
            let mut out = Vec::with_capacity(ys.len());
            for y in ys {
                out.push(f(k, *y)?);
            }
            Ok(Val::Arr(out))
        }
        _ => Ok(Val::Num(f(a.to_num()?, b.to_num()?)?)),
    }
}

fn binary(op: syntax::BinOp, a: &Val, b: &Val) -> Result<Val, String> {
    use syntax::BinOp::*;
    match op {
        Add => zip_maps(a, b, |x, y| Ok(x + y)),
        Sub => zip_maps(a, b, |x, y| Ok(x - y)),
        Mul => zip_maps(a, b, |x, y| Ok(x * y)),
        Div => zip_maps(a, b, |x, y| {
            if y == 0.0 {
                Err("division by zero".to_string())
            } else {
                Ok(x / y)
            }
        }),
        Mod => zip_maps(a, b, |x, y| {
            if y == 0.0 {
                Err("modulo by zero".to_string())
            } else {
                Ok(x % y)
            }
        }),
        Pow => zip_maps(a, b, |x, y| Ok(x.powf(y))).and_then(|v| match v {
            Val::Num(n) => finite(n),
            other => Ok(other),
        }),
        Eq => Ok(Val::Bool(
            a == b || matches!((a, b), (Val::Num(x), Val::Num(y)) if x == y),
        )),
        Ne => Ok(Val::Bool(
            !(a == b || matches!((a, b), (Val::Num(x), Val::Num(y)) if x == y)),
        )),
        Lt | Le | Gt | Ge => {
            // Lists compare by their first component, which is what a position move needs.
            let x = match a {
                Val::Arr(v) if !v.is_empty() => v[0],
                other => other.to_num()?,
            };
            let y = match b {
                Val::Arr(v) if !v.is_empty() => v[0],
                other => other.to_num()?,
            };
            Ok(Val::Bool(match op {
                Lt => x < y,
                Le => x <= y,
                Gt => x > y,
                _ => x >= y,
            }))
        }
        And | Or => unreachable!("short-circuited above"),
    }
}

fn need(name: &str, args: &[Val], n: usize) -> Result<(), String> {
    if args.len() == n {
        Ok(())
    } else {
        Err(format!(
            "`{name}` takes {n} argument{}, got {}",
            if n == 1 { "" } else { "s" },
            args.len()
        ))
    }
}

fn need_range(name: &str, args: &[Val], lo: usize, hi: usize) -> Result<(), String> {
    if args.len() >= lo && args.len() <= hi {
        Ok(())
    } else {
        Err(format!(
            "`{name}` takes {lo} to {hi} arguments, got {}",
            args.len()
        ))
    }
}

fn num_arg(args: &[Val], i: usize) -> Result<f64, String> {
    args[i].to_num()
}

fn hash_unit(mut x: u32) -> f64 {
    // splitmix32, so noise is deterministic across machines without any RNG state.
    x = x.wrapping_add(0x9e37_79b9);
    let mut z = x;
    z = (z ^ (z >> 16)).wrapping_mul(0x21f0_aaad);
    z = (z ^ (z >> 15)).wrapping_mul(0x735a_2d97);
    z ^= z >> 15;
    (z as f64) / (u32::MAX as f64 + 1.0)
}

fn value_noise(x: f64, seed: u32) -> f64 {
    let i = x.floor();
    let f = x - i;
    let a = hash_unit((i as i64 as u32) ^ seed);
    let b = hash_unit(((i as i64 + 1) as u32) ^ seed);
    let u = f * f * (3.0 - 2.0 * f);
    a + (b - a) * u
}

fn smooth(u: f64) -> f64 {
    let u = u.clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

fn bezier_y(c: [f64; 4], x: f64) -> f64 {
    let [x1, y1, x2, y2] = c;
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

/// Every function, for `kin capabilities` and for error hints.
pub const FUNCTIONS: &[(&str, &str)] = &[
    ("sin", "sin(x) radians"),
    ("cos", "cos(x) radians"),
    ("tan", "tan(x)"),
    ("asin", "asin(x)"),
    ("acos", "acos(x)"),
    ("atan", "atan(x)"),
    ("atan2", "atan2(y, x)"),
    ("sqrt", "sqrt(x)"),
    ("abs", "abs(x)"),
    ("floor", "floor(x)"),
    ("ceil", "ceil(x)"),
    ("round", "round(x)"),
    ("sign", "sign(x)"),
    ("exp", "exp(x)"),
    ("log", "log(x) natural"),
    ("pow", "pow(a, b)"),
    ("min", "min(a, ...)"),
    ("max", "max(a, ...)"),
    ("clamp", "clamp(x, lo, hi)"),
    ("mod", "mod(a, b)"),
    ("lerp", "lerp(a, b, u)"),
    ("smoothstep", "smoothstep(a, b, x)"),
    ("step", "step(edge, x)"),
    ("ease_in", "ease_in(u) accelerates"),
    ("ease_out", "ease_out(u) decelerates"),
    ("ease_in_out", "ease_in_out(u) both ends"),
    ("bezier", "bezier(x1, y1, x2, y2, u)"),
    ("spring", "spring(t, freq, damping, amp)"),
    ("noise", "noise(x[, seed]) deterministic, 0..1"),
    ("random", "random(seed) deterministic, 0..1"),
    ("wiggle", "wiggle(t, freq, amp[, seed])"),
    ("x", "x([a, b]) first component"),
    ("y", "y([a, b]) second component"),
    ("len", "len([a, b]) magnitude"),
    ("deg", "deg(radians)"),
    ("rad", "rad(degrees)"),
    ("link", "link(\"comp:main/layer:bg/opacity\")"),
];

fn call(name: &str, args: &[Val], ctx: &Ctx) -> Result<Val, String> {
    let f = |i: usize| num_arg(args, i);
    match name {
        "sin" => (need(name, args, 1)?, finite(f(0)?.sin())).1,
        "cos" => (need(name, args, 1)?, finite(f(0)?.cos())).1,
        "tan" => (need(name, args, 1)?, finite(f(0)?.tan())).1,
        "asin" => (need(name, args, 1)?, finite(f(0)?.asin())).1,
        "acos" => (need(name, args, 1)?, finite(f(0)?.acos())).1,
        "atan" => (need(name, args, 1)?, finite(f(0)?.atan())).1,
        "atan2" => (need(name, args, 2)?, finite(f(0)?.atan2(f(1)?))).1,
        "sqrt" => {
            need(name, args, 1)?;
            if f(0)? < 0.0 {
                return Err("sqrt of a negative number".to_string());
            }
            finite(f(0)?.sqrt())
        }
        "abs" => (need(name, args, 1)?, finite(f(0)?.abs())).1,
        "floor" => (need(name, args, 1)?, finite(f(0)?.floor())).1,
        "ceil" => (need(name, args, 1)?, finite(f(0)?.ceil())).1,
        "round" => (need(name, args, 1)?, finite(f(0)?.round())).1,
        "sign" => (need(name, args, 1)?, finite(f(0)?.signum())).1,
        "exp" => (need(name, args, 1)?, finite(f(0)?.exp())).1,
        "log" => {
            need(name, args, 1)?;
            if f(0)? <= 0.0 {
                return Err("log of zero or a negative number".to_string());
            }
            finite(f(0)?.ln())
        }
        "pow" => (need(name, args, 2)?, finite(f(0)?.powf(f(1)?))).1,
        "min" | "max" => {
            need_range(name, args, 1, 16)?;
            let mut acc = f(0)?;
            for i in 1..args.len() {
                let v = f(i)?;
                acc = if name == "min" {
                    acc.min(v)
                } else {
                    acc.max(v)
                };
            }
            finite(acc)
        }
        "clamp" => {
            need(name, args, 3)?;
            let (x, lo, hi) = (f(0)?, f(1)?, f(2)?);
            if lo > hi {
                return Err(format!("clamp bounds are inverted: {lo} > {hi}"));
            }
            finite(x.clamp(lo, hi))
        }
        "mod" => {
            need(name, args, 2)?;
            let (a, b) = (f(0)?, f(1)?);
            if b == 0.0 {
                return Err("modulo by zero".to_string());
            }
            finite(a % b)
        }
        "lerp" => {
            need(name, args, 3)?;
            let (a, b, u) = (f(0)?, f(1)?, f(2)?);
            finite(a + (b - a) * u)
        }
        "smoothstep" => {
            need(name, args, 3)?;
            let (a, b, x) = (f(0)?, f(1)?, f(2)?);
            if a == b {
                return Err("smoothstep edges are equal".to_string());
            }
            finite(smooth((x - a) / (b - a)))
        }
        "step" => {
            need(name, args, 2)?;
            Ok(Val::Num(if f(1)? >= f(0)? { 1.0 } else { 0.0 }))
        }
        "ease_in" => {
            need(name, args, 1)?;
            let u = f(0)?.clamp(0.0, 1.0);
            finite(u * u)
        }
        "ease_out" => {
            need(name, args, 1)?;
            let u = f(0)?.clamp(0.0, 1.0);
            finite(1.0 - (1.0 - u) * (1.0 - u))
        }
        "ease_in_out" => {
            need(name, args, 1)?;
            let u = f(0)?.clamp(0.0, 1.0);
            finite(if u < 0.5 {
                2.0 * u * u
            } else {
                1.0 - 2.0 * (1.0 - u) * (1.0 - u)
            })
        }
        "bezier" => {
            need(name, args, 5)?;
            finite(bezier_y([f(0)?, f(1)?, f(2)?, f(3)?], f(4)?))
        }
        "spring" => {
            // A damped oscillator: the settle time is visible in the numbers.
            need(name, args, 4)?;
            let (t, freq, damping, amp) = (f(0)?, f(1)?, f(2)?, f(3)?);
            finite(amp * (-damping * t).exp() * (std::f64::consts::TAU * freq * t).cos())
        }
        "noise" => {
            need_range(name, args, 1, 2)?;
            let seed = if args.len() == 2 {
                f(1)? as i64 as u32
            } else {
                0
            };
            finite(value_noise(f(0)?, seed))
        }
        "random" => {
            need(name, args, 1)?;
            finite(hash_unit(f(0)? as i64 as u32))
        }
        "wiggle" => {
            need_range(name, args, 3, 4)?;
            let (t, freq, amp) = (f(0)?, f(1)?, f(2)?);
            let seed = if args.len() == 4 {
                f(3)? as i64 as u32
            } else {
                0
            };
            let v = value_noise(t * freq, seed);
            finite(amp * (v * 2.0 - 1.0))
        }
        "x" => {
            need(name, args, 1)?;
            match &args[0] {
                Val::Arr(a) if a.len() == 2 => Ok(Val::Num(a[0])),
                _ => Err("x() takes a two-number list".to_string()),
            }
        }
        "y" => {
            need(name, args, 1)?;
            match &args[0] {
                Val::Arr(a) if a.len() == 2 => Ok(Val::Num(a[1])),
                _ => Err("y() takes a two-number list".to_string()),
            }
        }
        "len" => {
            need(name, args, 1)?;
            match &args[0] {
                Val::Arr(a) => finite(a.iter().map(|v| v * v).sum::<f64>().sqrt()),
                _ => Err("len() takes a list".to_string()),
            }
        }
        "deg" => (need(name, args, 1)?, finite(f(0)?.to_degrees())).1,
        "rad" => (need(name, args, 1)?, finite(f(0)?.to_radians())).1,
        "link" => {
            need(name, args, 1)?;
            let path = match &args[0] {
                Val::Str(s) => s.clone(),
                _ => return Err("link() takes a path string".to_string()),
            };
            (ctx.resolve)(&path)
        }
        other => Err(format!(
            "`{other}` is not a function; run `kin capabilities` for the list"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_at(t: f64) -> Ctx<'static> {
        Ctx {
            t,
            frame: (t * 30.0).round() as u32,
            fps: 30.0,
            width: 1920.0,
            height: 1080.0,
            duration: 5.0,
            resolve: &|path: &str| {
                if path == "comp:main/layer:bg/opacity" {
                    Ok(Val::Num(0.5))
                } else {
                    Err(format!("no `{path}`"))
                }
            },
        }
    }

    /// A number, with booleans read as 0/1 so condition tests can use the same helper.
    fn n(src: &str, t: f64) -> f64 {
        match eval(src, &ctx_at(t)) {
            Ok(Val::Num(v)) => v,
            Ok(Val::Bool(b)) => {
                if b {
                    1.0
                } else {
                    0.0
                }
            }
            other => panic!("{src} at {t} gave {other:?}"),
        }
    }

    #[test]
    fn arithmetic_and_precedence() {
        assert_eq!(n("1 + 2 * 3", 0.0), 7.0);
        assert_eq!(n("(1 + 2) * 3", 0.0), 9.0);
        assert_eq!(n("2 ^ 3 ^ 2", 0.0), 512.0);
        assert_eq!(n("-2 ^ 2", 0.0), -4.0);
        assert_eq!(n("10 % 4", 0.0), 2.0);
    }

    #[test]
    fn context_values_are_exact() {
        assert_eq!(n("t", 1.25), 1.25);
        assert_eq!(n("frame", 1.0), 30.0);
        assert_eq!(n("width / 2", 0.0), 960.0);
        assert_eq!(n("duration", 0.0), 5.0);
    }

    #[test]
    fn comparisons_and_ternary() {
        assert_eq!(n("t > 1 ? 10 : 20", 0.5), 20.0);
        assert_eq!(n("t > 1 ? 10 : 20", 2.0), 10.0);
        assert_eq!(n("t >= 1 && t < 2", 1.5), 1.0);
        assert_eq!(n("!(t > 1)", 0.5), 1.0);
    }

    #[test]
    fn easing_curves_match_their_definitions() {
        assert!((n("ease_out(0.5)", 0.0) - 0.75).abs() < 1e-12);
        assert!((n("ease_in(0.5)", 0.0) - 0.25).abs() < 1e-12);
        assert!((n("smoothstep(0, 1, 0.5)", 0.0) - 0.5).abs() < 1e-12);
        assert!((n("bezier(.2,.9,.1,1,0.5)", 0.0)) > 0.5);
    }

    #[test]
    fn spring_settles() {
        let early = n("abs(spring(t, 3, 6, 100))", 0.35);
        let late = n("abs(spring(t, 3, 6, 100))", 3.0);
        assert!(
            late < early * 0.02,
            "{late} should be near zero after {early}"
        );
    }

    #[test]
    fn noise_and_wiggle_are_deterministic() {
        let a = n("noise(t * 4, 7)", 1.234);
        let b = n("noise(t * 4, 7)", 1.234);
        assert_eq!(a, b);
        assert!((0.0..=1.0).contains(&a));
        let w = n("wiggle(t, 2, 50, 3)", 0.7);
        assert!(w.abs() <= 50.0);
    }

    #[test]
    fn lists_do_vector_maths() {
        let v = eval("[960, 600] + [0, wiggle(t, 2, 20)]", &ctx_at(0.5)).unwrap();
        match v {
            Val::Arr(a) => {
                assert_eq!(a[0], 960.0);
                assert!(a[1] != 600.0);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(n("y([3, 4])", 0.0), 4.0);
        assert_eq!(n("len([3, 4])", 0.0), 5.0);
    }

    #[test]
    fn links_reach_other_properties() {
        assert_eq!(n("link(\"comp:main/layer:bg/opacity\") * 2", 0.0), 1.0);
        let err = eval("link(\"comp:main/layer:nope\")", &ctx_at(0.0)).unwrap_err();
        assert!(
            err.message.contains("no `comp:main/layer:nope`"),
            "{}",
            err.message
        );
    }

    #[test]
    fn failures_are_explained() {
        assert!(eval("1 / 0", &ctx_at(0.0))
            .unwrap_err()
            .message
            .contains("division by zero"));
        assert!(eval("sqrt(-1)", &ctx_at(0.0))
            .unwrap_err()
            .message
            .contains("negative"));
        assert!(eval("clamp(1, 5, 0)", &ctx_at(0.0))
            .unwrap_err()
            .message
            .contains("inverted"));
        assert!(eval("nope", &ctx_at(0.0))
            .unwrap_err()
            .message
            .contains("not a name"));
        assert!(eval("sin(1, 2)", &ctx_at(0.0))
            .unwrap_err()
            .message
            .contains("takes 1"));
    }

    #[test]
    fn exotic_numbers_are_refused() {
        assert!(eval("1e308 * 1e308", &ctx_at(0.0)).is_err());
    }

    #[test]
    fn every_documented_function_resolves() {
        for (name, _) in FUNCTIONS {
            let err = eval(name, &ctx_at(0.0)).unwrap_err();
            assert!(
                !err.message.contains("is not a function"),
                "{name} is listed but not implemented"
            );
        }
    }
}
