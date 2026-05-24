//! `calc()` + `min()` / `max()` / `clamp()` resolver (numeric).
//!
//! Spec: https://www.w3.org/TR/css-values-4/#calc-notation
//! Eval: bottom-up with operator precedence (*  /  +  -).
//! For lengths a length->px context is passed in.

#[derive(Debug, Clone, PartialEq)]
pub enum CalcUnit {
    Number,
    Pixel,
    Em,
    Rem,
    Percent,
    Vw,
    Vh,
}

#[derive(Debug, Clone)]
pub struct LengthCtx {
    pub em: f32,
    pub rem: f32,
    pub vw: f32,
    pub vh: f32,
    pub percent_basis: f32,
}

impl Default for LengthCtx {
    fn default() -> Self {
        Self { em: 16.0, rem: 16.0, vw: 1280.0, vh: 800.0, percent_basis: 0.0 }
    }
}

#[derive(Debug, Clone)]
pub struct CalcValue {
    pub value: f32,
    pub unit: CalcUnit,
}

impl CalcValue {
    pub fn to_px(&self, ctx: &LengthCtx) -> f32 {
        match self.unit {
            CalcUnit::Number | CalcUnit::Pixel => self.value,
            CalcUnit::Em => self.value * ctx.em,
            CalcUnit::Rem => self.value * ctx.rem,
            CalcUnit::Percent => self.value * ctx.percent_basis / 100.0,
            CalcUnit::Vw => self.value * ctx.vw / 100.0,
            CalcUnit::Vh => self.value * ctx.vh / 100.0,
        }
    }
}

#[derive(Debug, Clone)]
pub enum CalcNode {
    Leaf(CalcValue),
    Add(Box<CalcNode>, Box<CalcNode>),
    Sub(Box<CalcNode>, Box<CalcNode>),
    Mul(Box<CalcNode>, Box<CalcNode>),
    Div(Box<CalcNode>, Box<CalcNode>),
    Neg(Box<CalcNode>),
    Min(Vec<CalcNode>),
    Max(Vec<CalcNode>),
    Clamp(Box<CalcNode>, Box<CalcNode>, Box<CalcNode>),
}

pub fn evaluate(node: &CalcNode, ctx: &LengthCtx) -> f32 {
    match node {
        CalcNode::Leaf(v) => v.to_px(ctx),
        CalcNode::Add(a, b) => evaluate(a, ctx) + evaluate(b, ctx),
        CalcNode::Sub(a, b) => evaluate(a, ctx) - evaluate(b, ctx),
        CalcNode::Mul(a, b) => evaluate(a, ctx) * evaluate(b, ctx),
        CalcNode::Div(a, b) => {
            let denom = evaluate(b, ctx);
            if denom == 0.0 { 0.0 } else { evaluate(a, ctx) / denom }
        }
        CalcNode::Neg(a) => -evaluate(a, ctx),
        CalcNode::Min(items) => items.iter().map(|i| evaluate(i, ctx)).fold(f32::INFINITY, f32::min),
        CalcNode::Max(items) => items.iter().map(|i| evaluate(i, ctx)).fold(f32::NEG_INFINITY, f32::max),
        CalcNode::Clamp(min, val, max) => {
            evaluate(val, ctx).max(evaluate(min, ctx)).min(evaluate(max, ctx))
        }
    }
}

/// Parse a leaf token like "10px" / "1.5em" / "50%".
pub fn parse_leaf(token: &str) -> Option<CalcValue> {
    let t = token.trim();
    // Find suffix
    let (num_part, unit) = if let Some(pos) = t.find(|c: char| c.is_alphabetic() || c == '%') {
        (&t[..pos], &t[pos..])
    } else {
        (t, "")
    };
    let value: f32 = num_part.parse().ok()?;
    let u = match unit {
        "" => CalcUnit::Number,
        "px" => CalcUnit::Pixel,
        "em" => CalcUnit::Em,
        "rem" => CalcUnit::Rem,
        "%" => CalcUnit::Percent,
        "vw" => CalcUnit::Vw,
        "vh" => CalcUnit::Vh,
        _ => return None,
    };
    Some(CalcValue { value, unit: u })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(s: &str) -> CalcNode {
        CalcNode::Leaf(parse_leaf(s).unwrap())
    }

    #[test]
    fn parse_px() {
        let v = parse_leaf("10px").unwrap();
        assert_eq!(v.value, 10.0);
        assert_eq!(v.unit, CalcUnit::Pixel);
    }

    #[test]
    fn parse_percent() {
        let v = parse_leaf("50%").unwrap();
        assert_eq!(v.unit, CalcUnit::Percent);
    }

    #[test]
    fn add_evaluates() {
        let n = CalcNode::Add(Box::new(leaf("10px")), Box::new(leaf("5px")));
        assert_eq!(evaluate(&n, &LengthCtx::default()), 15.0);
    }

    #[test]
    fn min_picks_smallest() {
        let n = CalcNode::Min(vec![leaf("10px"), leaf("5px"), leaf("20px")]);
        assert_eq!(evaluate(&n, &LengthCtx::default()), 5.0);
    }

    #[test]
    fn clamp_constrains() {
        let n = CalcNode::Clamp(Box::new(leaf("0px")), Box::new(leaf("50px")), Box::new(leaf("30px")));
        assert_eq!(evaluate(&n, &LengthCtx::default()), 30.0);
    }

    #[test]
    fn em_resolves_via_ctx() {
        let n = leaf("2em");
        let mut ctx = LengthCtx::default();
        ctx.em = 20.0;
        assert_eq!(evaluate(&n, &ctx), 40.0);
    }

    #[test]
    fn percent_uses_basis() {
        let n = leaf("50%");
        let mut ctx = LengthCtx::default();
        ctx.percent_basis = 200.0;
        assert_eq!(evaluate(&n, &ctx), 100.0);
    }

    #[test]
    fn div_by_zero_returns_zero() {
        let n = CalcNode::Div(Box::new(leaf("10")), Box::new(leaf("0")));
        assert_eq!(evaluate(&n, &LengthCtx::default()), 0.0);
    }
}
