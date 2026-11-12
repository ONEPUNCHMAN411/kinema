//! Lexer and parser for the expression language.
//!
//! The grammar is deliberately small and total: no loops, no assignment, no user
//! functions, no I/O. An expression is a pure function of its context, which is what
//! lets a property be evaluated at any tick without order mattering.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Num(f64),
    Str(String),
    Ident(String),
    /// Operators and punctuation, kept as their source spelling.
    Op(&'static str),
    Eof,
}

impl fmt::Display for Tok {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Tok::Num(n) => write!(f, "{n}"),
            Tok::Str(s) => write!(f, "\"{s}\""),
            Tok::Ident(s) => write!(f, "{s}"),
            Tok::Op(s) => write!(f, "{s}"),
            Tok::Eof => write!(f, "end of expression"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
    Pos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Num(f64),
    Str(String),
    Bool(bool),
    Var(String),
    Array(Vec<Expr>),
    Call {
        name: String,
        args: Vec<Expr>,
        at: usize,
    },
    Unary {
        op: UnOp,
        rhs: Box<Expr>,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Ternary {
        cond: Box<Expr>,
        yes: Box<Expr>,
        no: Box<Expr>,
    },
}

#[derive(Debug, Clone)]
pub struct ExprError {
    pub kind: &'static str,
    pub message: String,
    pub at: usize,
    pub hint: String,
}

impl ExprError {
    fn new(kind: &'static str, message: impl Into<String>, at: usize) -> Self {
        ExprError {
            kind,
            message: message.into(),
            at,
            hint: String::new(),
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = hint.into();
        self
    }
}

impl fmt::Display for ExprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.message, self.at)
    }
}

impl std::error::Error for ExprError {}

/// Longest expression accepted, so a hostile document cannot wedge the evaluator.
pub const MAX_SOURCE: usize = 4096;
/// Deepest nesting accepted.
pub const MAX_DEPTH: usize = 32;

pub fn lex(src: &str) -> Result<Vec<(Tok, usize)>, ExprError> {
    if src.len() > MAX_SOURCE {
        return Err(ExprError::new(
            "too-long",
            format!(
                "expression is {} bytes, the limit is {MAX_SOURCE}",
                src.len()
            ),
            0,
        )
        .with_hint("split the property into several linked properties"));
    }
    let bytes: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        if c.is_ascii_digit() || (c == '.' && bytes.get(i + 1).is_some_and(|d| d.is_ascii_digit()))
        {
            let mut j = i;
            while j < bytes.len() && (bytes[j].is_ascii_digit() || bytes[j] == '.') {
                j += 1;
            }
            // Exponent, but only when it is actually followed by digits.
            if j < bytes.len() && (bytes[j] == 'e' || bytes[j] == 'E') {
                let mut k = j + 1;
                if k < bytes.len() && (bytes[k] == '+' || bytes[k] == '-') {
                    k += 1;
                }
                if k < bytes.len() && bytes[k].is_ascii_digit() {
                    while k < bytes.len() && bytes[k].is_ascii_digit() {
                        k += 1;
                    }
                    j = k;
                }
            }
            let text: String = bytes[i..j].iter().collect();
            let n: f64 = text.parse().map_err(|_| {
                ExprError::new("bad-number", format!("`{text}` is not a number"), start)
            })?;
            out.push((Tok::Num(n), start));
            i = j;
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let mut j = i;
            while j < bytes.len() && (bytes[j].is_alphanumeric() || bytes[j] == '_') {
                j += 1;
            }
            let text: String = bytes[i..j].iter().collect();
            out.push((Tok::Ident(text), start));
            i = j;
            continue;
        }
        if c == '"' {
            let mut j = i + 1;
            let mut s = String::new();
            loop {
                if j >= bytes.len() {
                    return Err(ExprError::new("unterminated", "unterminated string", start)
                        .with_hint("close the string with a double quote"));
                }
                match bytes[j] {
                    '"' => break,
                    '\\' if j + 1 < bytes.len() => {
                        s.push(match bytes[j + 1] {
                            'n' => '\n',
                            't' => '\t',
                            other => other,
                        });
                        j += 2;
                    }
                    ch => {
                        s.push(ch);
                        j += 1;
                    }
                }
            }
            out.push((Tok::Str(s), start));
            i = j + 1;
            continue;
        }
        let two: String = bytes[i..(i + 2).min(bytes.len())].iter().collect();
        let op: Option<&'static str> = match two.as_str() {
            "==" => Some("=="),
            "!=" => Some("!="),
            "<=" => Some("<="),
            ">=" => Some(">="),
            "&&" => Some("&&"),
            "||" => Some("||"),
            _ => match c {
                '+' => Some("+"),
                '-' => Some("-"),
                '*' => Some("*"),
                '/' => Some("/"),
                '%' => Some("%"),
                '^' => Some("^"),
                '(' => Some("("),
                ')' => Some(")"),
                '[' => Some("["),
                ']' => Some("]"),
                ',' => Some(","),
                '?' => Some("?"),
                ':' => Some(":"),
                '!' => Some("!"),
                '<' => Some("<"),
                '>' => Some(">"),
                _ => None,
            },
        };
        match op {
            Some(o) => {
                out.push((Tok::Op(o), start));
                i += o.len();
            }
            None => {
                return Err(ExprError::new(
                    "bad-character",
                    format!("`{c}` has no meaning here"),
                    start,
                )
                .with_hint("operators are + - * / % ^, comparisons, && and ||"))
            }
        }
    }
    out.push((Tok::Eof, bytes.len()));
    Ok(out)
}

struct Parser {
    toks: Vec<(Tok, usize)>,
    i: usize,
    depth: usize,
}

pub fn parse(src: &str) -> Result<Expr, ExprError> {
    let toks = lex(src)?;
    let mut p = Parser {
        toks,
        i: 0,
        depth: 0,
    };
    let e = p.expr()?;
    if p.peek() != &Tok::Eof {
        let at = p.at();
        return Err(ExprError::new(
            "trailing",
            format!("unexpected `{}` after a complete expression", p.peek()),
            at,
        )
        .with_hint("remove the extra tokens"));
    }
    Ok(e)
}

impl Parser {
    fn peek(&self) -> &Tok {
        &self.toks[self.i].0
    }

    fn at(&self) -> usize {
        self.toks[self.i].1
    }

    fn bump(&mut self) -> Tok {
        let t = self.toks[self.i].0.clone();
        if self.i + 1 < self.toks.len() {
            self.i += 1;
        }
        t
    }

    fn eat(&mut self, op: &str) -> bool {
        if matches!(self.peek(), Tok::Op(o) if *o == op) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, op: &str) -> Result<(), ExprError> {
        if self.eat(op) {
            Ok(())
        } else {
            Err(ExprError::new(
                "syntax",
                format!("expected `{op}`, found `{}`", self.peek()),
                self.at(),
            )
            .with_hint("check the brackets and commas"))
        }
    }

    fn nest(&mut self) -> Result<(), ExprError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(ExprError::new(
                "too-deep",
                format!("nesting deeper than {MAX_DEPTH}"),
                self.at(),
            )
            .with_hint("pull part of the expression into a linked property"));
        }
        Ok(())
    }

    fn expr(&mut self) -> Result<Expr, ExprError> {
        self.nest()?;
        let cond = self.or()?;
        let out = if self.eat("?") {
            let yes = self.expr()?;
            self.expect(":")?;
            let no = self.expr()?;
            Expr::Ternary {
                cond: Box::new(cond),
                yes: Box::new(yes),
                no: Box::new(no),
            }
        } else {
            cond
        };
        self.depth -= 1;
        Ok(out)
    }

    fn or(&mut self) -> Result<Expr, ExprError> {
        let mut lhs = self.and()?;
        while self.eat("||") {
            let rhs = self.and()?;
            lhs = Expr::Binary {
                op: BinOp::Or,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn and(&mut self) -> Result<Expr, ExprError> {
        let mut lhs = self.equality()?;
        while self.eat("&&") {
            let rhs = self.equality()?;
            lhs = Expr::Binary {
                op: BinOp::And,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn equality(&mut self) -> Result<Expr, ExprError> {
        let mut lhs = self.comparison()?;
        loop {
            let op = if self.eat("==") {
                BinOp::Eq
            } else if self.eat("!=") {
                BinOp::Ne
            } else {
                return Ok(lhs);
            };
            let rhs = self.comparison()?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
    }

    fn comparison(&mut self) -> Result<Expr, ExprError> {
        let mut lhs = self.additive()?;
        loop {
            let op = if self.eat("<=") {
                BinOp::Le
            } else if self.eat(">=") {
                BinOp::Ge
            } else if self.eat("<") {
                BinOp::Lt
            } else if self.eat(">") {
                BinOp::Gt
            } else {
                return Ok(lhs);
            };
            let rhs = self.additive()?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
    }

    fn additive(&mut self) -> Result<Expr, ExprError> {
        let mut lhs = self.multiplicative()?;
        loop {
            let op = if self.eat("+") {
                BinOp::Add
            } else if self.eat("-") {
                BinOp::Sub
            } else {
                return Ok(lhs);
            };
            let rhs = self.multiplicative()?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
    }

    fn multiplicative(&mut self) -> Result<Expr, ExprError> {
        let mut lhs = self.unary()?;
        loop {
            let op = if self.eat("*") {
                BinOp::Mul
            } else if self.eat("/") {
                BinOp::Div
            } else if self.eat("%") {
                BinOp::Mod
            } else {
                return Ok(lhs);
            };
            let rhs = self.unary()?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
    }

    fn unary(&mut self) -> Result<Expr, ExprError> {
        self.nest()?;
        let out = if self.eat("-") {
            Expr::Unary {
                op: UnOp::Neg,
                rhs: Box::new(self.unary()?),
            }
        } else if self.eat("!") {
            Expr::Unary {
                op: UnOp::Not,
                rhs: Box::new(self.unary()?),
            }
        } else if self.eat("+") {
            Expr::Unary {
                op: UnOp::Pos,
                rhs: Box::new(self.unary()?),
            }
        } else {
            self.power()?
        };
        self.depth -= 1;
        Ok(out)
    }

    fn power(&mut self) -> Result<Expr, ExprError> {
        let base = self.primary()?;
        if self.eat("^") {
            // Right associative, and the exponent may be signed: 2^-1.
            let exp = self.unary()?;
            return Ok(Expr::Binary {
                op: BinOp::Pow,
                lhs: Box::new(base),
                rhs: Box::new(exp),
            });
        }
        Ok(base)
    }

    fn primary(&mut self) -> Result<Expr, ExprError> {
        let at = self.at();
        match self.peek().clone() {
            Tok::Num(n) => {
                self.bump();
                Ok(Expr::Num(n))
            }
            Tok::Str(s) => {
                self.bump();
                Ok(Expr::Str(s))
            }
            Tok::Ident(name) => {
                self.bump();
                if self.eat("(") {
                    let mut args = Vec::new();
                    if !self.eat(")") {
                        loop {
                            args.push(self.expr()?);
                            if self.eat(",") {
                                continue;
                            }
                            self.expect(")")?;
                            break;
                        }
                    }
                    if name == "true" || name == "false" {
                        return Err(ExprError::new(
                            "syntax",
                            format!("`{name}` is not callable"),
                            at,
                        ));
                    }
                    Ok(Expr::Call { name, args, at })
                } else {
                    match name.as_str() {
                        "true" => Ok(Expr::Bool(true)),
                        "false" => Ok(Expr::Bool(false)),
                        _ => Ok(Expr::Var(name)),
                    }
                }
            }
            Tok::Op("[") => {
                self.bump();
                let mut items = Vec::new();
                if !self.eat("]") {
                    loop {
                        items.push(self.expr()?);
                        if self.eat(",") {
                            continue;
                        }
                        self.expect("]")?;
                        break;
                    }
                }
                Ok(Expr::Array(items))
            }
            Tok::Op("(") => {
                self.bump();
                let inner = self.expr()?;
                self.expect(")")?;
                Ok(inner)
            }
            other => Err(ExprError::new(
                "syntax",
                format!("expected a value, found `{other}`"),
                at,
            )
            .with_hint("values are numbers, strings, true/false, [a, b], t, frame or a call")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precedence_follows_arithmetic() {
        let e = parse("1 + 2 * 3").unwrap();
        assert_eq!(
            e,
            Expr::Binary {
                op: BinOp::Add,
                lhs: Box::new(Expr::Num(1.0)),
                rhs: Box::new(Expr::Binary {
                    op: BinOp::Mul,
                    lhs: Box::new(Expr::Num(2.0)),
                    rhs: Box::new(Expr::Num(3.0)),
                }),
            }
        );
    }

    #[test]
    fn power_binds_tighter_than_unary_minus() {
        // -2^2 is -(2^2), as in every language that has both.
        let e = parse("-2 ^ 2").unwrap();
        assert!(matches!(e, Expr::Unary { op: UnOp::Neg, .. }));
    }

    #[test]
    fn ternary_and_calls_parse() {
        assert!(matches!(
            parse("t > 1 ? 1 : t").unwrap(),
            Expr::Ternary { .. }
        ));
        assert!(matches!(
            parse("clamp(t, 0, 1)").unwrap(),
            Expr::Call { .. }
        ));
        assert!(matches!(parse("[960, 600]").unwrap(), Expr::Array(a) if a.len() == 2));
    }

    #[test]
    fn numbers_include_exponents_and_decimals() {
        assert_eq!(parse(".5").unwrap(), Expr::Num(0.5));
        assert_eq!(parse("1e-3").unwrap(), Expr::Num(0.001));
    }

    #[test]
    fn errors_point_at_the_problem() {
        let e = parse("1 + ").unwrap_err();
        assert_eq!(e.kind, "syntax");
        let e = parse("(1").unwrap_err();
        assert_eq!(e.kind, "syntax");
        let e = parse("1 2").unwrap_err();
        assert_eq!(e.kind, "trailing");
        let e = parse("1 $ 2").unwrap_err();
        assert_eq!(e.kind, "bad-character");
    }

    #[test]
    fn nesting_is_bounded() {
        let src = "(".repeat(MAX_DEPTH + 2);
        let e = parse(&src).unwrap_err();
        assert_eq!(e.kind, "too-deep");
    }

    #[test]
    fn strings_lex_with_escapes() {
        let toks = lex(r#""a\nb""#).unwrap();
        assert_eq!(toks[0].0, Tok::Str("a\nb".into()));
    }
}
