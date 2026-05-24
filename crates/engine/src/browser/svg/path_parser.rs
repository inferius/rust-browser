//! SVG `<path d>` mini-language parser.
//!
//! Spec: https://www.w3.org/TR/SVG11/paths.html#PathData
//! Commands: M/L/H/V/C/S/Q/T/A/Z (absolute) and m/l/h/v/c/s/q/t/a/z (relative).

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathCommand {
    Move,
    Line,
    HLine,
    VLine,
    Cubic,
    SmoothCubic,
    Quad,
    SmoothQuad,
    Arc,
    Close,
}

#[derive(Debug, Clone)]
pub struct PathSegment {
    pub cmd: PathCommand,
    pub absolute: bool,
    pub params: Vec<f32>,
}

impl PathSegment {
    pub fn required_params(cmd: PathCommand) -> usize {
        match cmd {
            PathCommand::Move | PathCommand::Line => 2,
            PathCommand::HLine | PathCommand::VLine => 1,
            PathCommand::Cubic => 6,
            PathCommand::SmoothCubic | PathCommand::Quad => 4,
            PathCommand::SmoothQuad => 2,
            PathCommand::Arc => 7,
            PathCommand::Close => 0,
        }
    }
}

/// Parse a path data string into a sequence of segments.
pub fn parse(input: &str) -> Result<Vec<PathSegment>, String> {
    let mut out = Vec::new();
    let mut iter = input.chars().peekable();
    let mut last_cmd: Option<(PathCommand, bool)> = None;

    loop {
        skip_ws(&mut iter);
        let Some(c) = iter.peek().copied() else { break; };
        if c.is_ascii_alphabetic() {
            iter.next();
            let absolute = c.is_ascii_uppercase();
            let mut cmd = match c.to_ascii_lowercase() {
                'm' => PathCommand::Move,
                'l' => PathCommand::Line,
                'h' => PathCommand::HLine,
                'v' => PathCommand::VLine,
                'c' => PathCommand::Cubic,
                's' => PathCommand::SmoothCubic,
                'q' => PathCommand::Quad,
                't' => PathCommand::SmoothQuad,
                'a' => PathCommand::Arc,
                'z' => PathCommand::Close,
                _ => return Err(format!("unknown command '{}'", c)),
            };
            if cmd == PathCommand::Close {
                out.push(PathSegment { cmd, absolute, params: Vec::new() });
                last_cmd = None;
                continue;
            }
            // First segment: required params for the original command.
            let req = PathSegment::required_params(cmd);
            let params = read_numbers(&mut iter, req)?;
            if params.len() != req { return Err("partial command params".into()); }
            out.push(PathSegment { cmd, absolute, params });
            // After first Move, subsequent implicit params become Line.
            if cmd == PathCommand::Move { cmd = PathCommand::Line; }
            last_cmd = Some((cmd, absolute));
            continue;
        }
        // Implicit repeat using last command.
        if let Some((cmd, absolute)) = last_cmd {
            let req = PathSegment::required_params(cmd);
            let n = read_numbers(&mut iter, req)?;
            if n.len() == req {
                out.push(PathSegment { cmd, absolute, params: n });
                continue;
            }
        }
        return Err(format!("unexpected char '{}'", c));
    }
    Ok(out)
}

fn skip_ws(iter: &mut std::iter::Peekable<std::str::Chars>) {
    while iter.peek().map(|c| c.is_whitespace() || *c == ',').unwrap_or(false) {
        iter.next();
    }
}

fn read_numbers(iter: &mut std::iter::Peekable<std::str::Chars>, count: usize) -> Result<Vec<f32>, String> {
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        skip_ws(iter);
        let mut buf = String::new();
        let mut seen_dot = false;
        if let Some(c) = iter.peek().copied() {
            if c == '+' || c == '-' { buf.push(c); iter.next(); }
        }
        while let Some(c) = iter.peek().copied() {
            if c.is_ascii_digit() { buf.push(c); iter.next(); }
            else if c == '.' && !seen_dot { buf.push(c); seen_dot = true; iter.next(); }
            else if c == 'e' || c == 'E' {
                buf.push(c); iter.next();
                if let Some(s) = iter.peek().copied() { if s == '+' || s == '-' { buf.push(s); iter.next(); } }
            }
            else { break; }
        }
        if buf.is_empty() { return Ok(out); }
        let v: f32 = buf.parse().map_err(|e: std::num::ParseFloatError| e.to_string())?;
        out.push(v);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_move_line() {
        let p = parse("M 10 20 L 30 40").unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].cmd, PathCommand::Move);
        assert_eq!(p[0].params, vec![10.0, 20.0]);
        assert_eq!(p[1].cmd, PathCommand::Line);
        assert_eq!(p[1].params, vec![30.0, 40.0]);
    }

    #[test]
    fn parse_close_command() {
        let p = parse("M 0 0 L 10 10 z").unwrap();
        assert_eq!(p.last().unwrap().cmd, PathCommand::Close);
    }

    #[test]
    fn parse_relative() {
        let p = parse("m 5 5 l 10 0").unwrap();
        assert!(!p[0].absolute);
        assert!(!p[1].absolute);
    }

    #[test]
    fn parse_implicit_repeat_m_becomes_l() {
        let p = parse("M 0 0 10 10 20 20").unwrap();
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].cmd, PathCommand::Move);
        assert_eq!(p[1].cmd, PathCommand::Line);
        assert_eq!(p[2].cmd, PathCommand::Line);
    }

    #[test]
    fn parse_cubic() {
        let p = parse("M 0 0 C 10 0 20 10 30 10").unwrap();
        assert_eq!(p[1].cmd, PathCommand::Cubic);
        assert_eq!(p[1].params.len(), 6);
    }

    #[test]
    fn parse_arc_seven_params() {
        let p = parse("M 0 0 A 50 50 0 0 1 100 100").unwrap();
        assert_eq!(p[1].cmd, PathCommand::Arc);
        assert_eq!(p[1].params.len(), 7);
    }

    #[test]
    fn parse_negative_numbers() {
        let p = parse("M -10 -20 L 5.5 -3.14").unwrap();
        assert_eq!(p[0].params, vec![-10.0, -20.0]);
    }

    #[test]
    fn parse_unknown_command_errors() {
        assert!(parse("X 10 10").is_err());
    }
}
