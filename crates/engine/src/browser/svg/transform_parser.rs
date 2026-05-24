//! SVG transform="..." attribute parser.
//!
//! Spec: https://www.w3.org/TR/SVG11/coords.html#TransformAttribute
//! translate(x [y]) | scale(x [y]) | rotate(angle [cx cy]) | skewX(angle) | skewY(angle) | matrix(a b c d e f)

#[derive(Debug, Clone, PartialEq)]
pub enum SvgTransform {
    Translate(f32, f32),
    Scale(f32, f32),
    Rotate(f32, Option<(f32, f32)>),
    SkewX(f32),
    SkewY(f32),
    Matrix([f32; 6]),
}

pub fn parse(input: &str) -> Result<Vec<SvgTransform>, String> {
    let mut out = Vec::new();
    let mut chars = input.chars().peekable();
    loop {
        skip_ws(&mut chars);
        if chars.peek().is_none() { break; }
        let mut name = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_ascii_alphabetic() { name.push(c); chars.next(); }
            else { break; }
        }
        if name.is_empty() { return Err("expected transform name".into()); }
        skip_ws(&mut chars);
        if chars.next() != Some('(') { return Err("expected '('".into()); }
        let mut args_buf = String::new();
        for c in chars.by_ref() {
            if c == ')' { break; }
            args_buf.push(c);
        }
        let args: Vec<f32> = args_buf.split(|c: char| c == ',' || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .map(|s| s.parse::<f32>().map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        let t = build_transform(&name, &args)?;
        out.push(t);
        skip_ws(&mut chars);
        // optional comma
        if chars.peek().copied() == Some(',') { chars.next(); }
    }
    Ok(out)
}

fn skip_ws(chars: &mut std::iter::Peekable<std::str::Chars>) {
    while chars.peek().map(|c| c.is_whitespace()).unwrap_or(false) { chars.next(); }
}

fn build_transform(name: &str, args: &[f32]) -> Result<SvgTransform, String> {
    Ok(match name {
        "translate" => match args.len() {
            1 => SvgTransform::Translate(args[0], 0.0),
            2 => SvgTransform::Translate(args[0], args[1]),
            _ => return Err("translate requires 1 or 2 args".into()),
        },
        "scale" => match args.len() {
            1 => SvgTransform::Scale(args[0], args[0]),
            2 => SvgTransform::Scale(args[0], args[1]),
            _ => return Err("scale requires 1 or 2 args".into()),
        },
        "rotate" => match args.len() {
            1 => SvgTransform::Rotate(args[0], None),
            3 => SvgTransform::Rotate(args[0], Some((args[1], args[2]))),
            _ => return Err("rotate requires 1 or 3 args".into()),
        },
        "skewX" => {
            if args.len() != 1 { return Err("skewX requires 1 arg".into()); }
            SvgTransform::SkewX(args[0])
        }
        "skewY" => {
            if args.len() != 1 { return Err("skewY requires 1 arg".into()); }
            SvgTransform::SkewY(args[0])
        }
        "matrix" => {
            if args.len() != 6 { return Err("matrix requires 6 args".into()); }
            SvgTransform::Matrix([args[0], args[1], args[2], args[3], args[4], args[5]])
        }
        _ => return Err(format!("unknown transform '{}'", name)),
    })
}

/// Convert single transform to 2D affine matrix (a b c d e f).
pub fn to_matrix(t: &SvgTransform) -> [f32; 6] {
    match t {
        SvgTransform::Translate(x, y) => [1.0, 0.0, 0.0, 1.0, *x, *y],
        SvgTransform::Scale(sx, sy) => [*sx, 0.0, 0.0, *sy, 0.0, 0.0],
        SvgTransform::Rotate(a, c) => {
            let rad = a.to_radians();
            let cs = rad.cos();
            let sn = rad.sin();
            let mat = [cs, sn, -sn, cs, 0.0, 0.0];
            if let Some((cx, cy)) = c {
                let t1 = [1.0, 0.0, 0.0, 1.0, *cx, *cy];
                let t2 = [1.0, 0.0, 0.0, 1.0, -*cx, -*cy];
                let m = multiply(t1, mat);
                multiply(m, t2)
            } else { mat }
        }
        SvgTransform::SkewX(a) => [1.0, 0.0, a.to_radians().tan(), 1.0, 0.0, 0.0],
        SvgTransform::SkewY(a) => [1.0, a.to_radians().tan(), 0.0, 1.0, 0.0, 0.0],
        SvgTransform::Matrix(m) => *m,
    }
}

pub fn multiply(a: [f32; 6], b: [f32; 6]) -> [f32; 6] {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_translate() {
        let t = parse("translate(10 20)").unwrap();
        assert_eq!(t, vec![SvgTransform::Translate(10.0, 20.0)]);
    }

    #[test]
    fn parse_single_arg_scale() {
        let t = parse("scale(2)").unwrap();
        assert_eq!(t, vec![SvgTransform::Scale(2.0, 2.0)]);
    }

    #[test]
    fn parse_rotate_around_center() {
        let t = parse("rotate(45 100 100)").unwrap();
        assert_eq!(t, vec![SvgTransform::Rotate(45.0, Some((100.0, 100.0)))]);
    }

    #[test]
    fn parse_matrix_six() {
        let t = parse("matrix(1 0 0 1 50 50)").unwrap();
        if let SvgTransform::Matrix(m) = &t[0] {
            assert_eq!(*m, [1.0, 0.0, 0.0, 1.0, 50.0, 50.0]);
        } else { panic!("expected matrix"); }
    }

    #[test]
    fn parse_chained() {
        let t = parse("translate(10 0) scale(2)").unwrap();
        assert_eq!(t.len(), 2);
    }

    #[test]
    fn to_matrix_translate() {
        let m = to_matrix(&SvgTransform::Translate(5.0, 10.0));
        assert_eq!(m, [1.0, 0.0, 0.0, 1.0, 5.0, 10.0]);
    }

    #[test]
    fn to_matrix_scale() {
        let m = to_matrix(&SvgTransform::Scale(2.0, 3.0));
        assert_eq!(m, [2.0, 0.0, 0.0, 3.0, 0.0, 0.0]);
    }
}
