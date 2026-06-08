//! CSS <length> parsing - px/em/rem/vw/vh/% support.

/// Parse delku v px/em/rem/vw/vh/% (bez kontextu - vraci 0 pro %).
/// Delsi suffixy musi byt kontrolovany drive.
pub fn parse_length(s: &str) -> f32 {
    parse_length_ctx(s, 1024.0, 768.0, 16.0)
}

/// Resolve length proti parent containeru. Pri "%" suffix vraci parent * p/100.
/// Bez % deleguje na parse_length (pixel/em/rem/vw/vh/...).
/// Empty string / "none" -> 0.
pub fn parse_length_or_pct(s: &str, parent: f32) -> f32 {
    let v = s.trim();
    if v.is_empty() || v == "none" { return 0.0; }
    if let Some(pct_str) = v.strip_suffix('%') {
        if let Ok(p) = pct_str.parse::<f32>() {
            return parent * p / 100.0;
        }
    }
    parse_length(v)
}

/// Parse delky s viewport kontextem (pro vw/vh/% support).
pub fn parse_length_ctx(s: &str, vw: f32, vh: f32, parent_size: f32) -> f32 {
    let s = s.trim();
    // calc/clamp/min/max s `%` - cascade je nepocita (% = parent width znamy az
    // tady). Kazdy operand resolvujem rekurzivne pres parse_length_ctx.
    if let Some(inner) = s.strip_prefix("calc(").and_then(|x| x.strip_suffix(')')) {
        return eval_calc_ctx(inner, vw, vh, parent_size);
    }
    if let Some(inner) = s.strip_prefix("clamp(").and_then(|x| x.strip_suffix(')')) {
        let a = super::split_top_level_commas(inner);
        if a.len() == 3 {
            let lo = parse_length_ctx(a[0].trim(), vw, vh, parent_size);
            let val = parse_length_ctx(a[1].trim(), vw, vh, parent_size);
            let hi = parse_length_ctx(a[2].trim(), vw, vh, parent_size);
            return val.clamp(lo.min(hi), lo.max(hi));
        }
    }
    if let Some(inner) = s.strip_prefix("min(").and_then(|x| x.strip_suffix(')')) {
        return super::split_top_level_commas(inner).iter()
            .map(|p| parse_length_ctx(p.trim(), vw, vh, parent_size))
            .fold(f32::INFINITY, f32::min);
    }
    if let Some(inner) = s.strip_prefix("max(").and_then(|x| x.strip_suffix(')')) {
        return super::split_top_level_commas(inner).iter()
            .map(|p| parse_length_ctx(p.trim(), vw, vh, parent_size))
            .fold(f32::NEG_INFINITY, f32::max);
    }
    if let Some(num) = s.strip_suffix("rem") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * 16.0;
    }
    if let Some(num) = s.strip_suffix("vmin") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * vw.min(vh) / 100.0;
    }
    if let Some(num) = s.strip_suffix("vmax") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * vw.max(vh) / 100.0;
    }
    if let Some(num) = s.strip_suffix("px") {
        return num.trim().parse().unwrap_or(0.0);
    }
    if let Some(num) = s.strip_suffix("em") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * 16.0;
    }
    // Dynamic / small / large viewport units - musi byt PRED vw/vh aby suffix match
    if let Some(num) = s.strip_suffix("dvw").or_else(|| s.strip_suffix("svw")).or_else(|| s.strip_suffix("lvw")) {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * vw / 100.0;
    }
    if let Some(num) = s.strip_suffix("dvh").or_else(|| s.strip_suffix("svh")).or_else(|| s.strip_suffix("lvh")) {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * vh / 100.0;
    }
    if let Some(num) = s.strip_suffix("dvi").or_else(|| s.strip_suffix("svi")).or_else(|| s.strip_suffix("lvi")) {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * vw / 100.0;
    }
    if let Some(num) = s.strip_suffix("dvb").or_else(|| s.strip_suffix("svb")).or_else(|| s.strip_suffix("lvb")) {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * vh / 100.0;
    }
    if let Some(num) = s.strip_suffix("vi") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * vw / 100.0;
    }
    if let Some(num) = s.strip_suffix("vb") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * vh / 100.0;
    }
    if let Some(num) = s.strip_suffix("vw") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * vw / 100.0;
    }
    if let Some(num) = s.strip_suffix("vh") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * vh / 100.0;
    }
    // CSS Container Queries L1 jednotky - aktualne aproximace = viewport
    // (presna implementace by potrebovala lookup nejblizsiho ancestor s container-type).
    if let Some(num) = s.strip_suffix("cqw") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * vw / 100.0;
    }
    if let Some(num) = s.strip_suffix("cqh") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * vh / 100.0;
    }
    if let Some(num) = s.strip_suffix("cqi") {
        // inline = horizontal v default writing-mode
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * vw / 100.0;
    }
    if let Some(num) = s.strip_suffix("cqb") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * vh / 100.0;
    }
    if let Some(num) = s.strip_suffix("cqmin") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * vw.min(vh) / 100.0;
    }
    if let Some(num) = s.strip_suffix("cqmax") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * vw.max(vh) / 100.0;
    }
    if let Some(num) = s.strip_suffix("pt") {
        // 1pt = 1.333px
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * 1.333;
    }
    // Character / line-height units
    // ch = sirka 0 (zero glyph) - aproximace 0.5 * font-size (default 16px)
    // lh = line-height current element - aproximace 1.2 * font-size
    // rlh = root lh
    if let Some(num) = s.strip_suffix("rlh") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * 16.0 * 1.2;
    }
    if let Some(num) = s.strip_suffix("lh") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * 16.0 * 1.2;
    }
    if let Some(num) = s.strip_suffix("ch") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * 16.0 * 0.5;
    }
    if let Some(num) = s.strip_suffix("ex") {
        // ex = vyska x-height - aproximace 0.5 * font-size
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * 16.0 * 0.5;
    }
    // Absolutni jednotky
    if let Some(num) = s.strip_suffix("cm") {
        // 1cm = 96/2.54 px ~ 37.795
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * 37.795;
    }
    if let Some(num) = s.strip_suffix("mm") {
        // 1mm = 1cm/10
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * 3.7795;
    }
    if let Some(num) = s.strip_suffix("Q") {
        // 1Q = 0.25mm
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * 0.9449;
    }
    if let Some(num) = s.strip_suffix("in") {
        // 1in = 96px
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * 96.0;
    }
    if let Some(num) = s.strip_suffix("pc") {
        // 1pc = 12pt = 16px
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * 16.0;
    }
    if let Some(num) = s.strip_suffix('%') {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * parent_size / 100.0;
    }
    s.parse().unwrap_or(0.0)
}

/// Vyhodnoti calc() arithmetiku s kontextem. Operandy resolvuje pres
/// parse_length_ctx (zna %/em/vw/vh). Precedence: * / pred + -.
fn eval_calc_ctx(expr: &str, vw: f32, vh: f32, parent_size: f32) -> f32 {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.is_empty() { return 0.0; }
    let resolve = |p: &str| -> f32 {
        // Cisty bezrozmerny faktor (pro * /) NEBO delka.
        if let Ok(n) = p.parse::<f32>() { n }
        else { parse_length_ctx(p, vw, vh, parent_size) }
    };
    // Pass 1: * / left-to-right.
    let mut vals: Vec<f32> = vec![resolve(parts[0])];
    let mut ops: Vec<&str> = Vec::new();
    let mut i = 1;
    while i + 1 < parts.len() {
        let op = parts[i];
        let val = resolve(parts[i + 1]);
        match op {
            "*" => { if let Some(l) = vals.last_mut() { *l *= val; } }
            "/" => { if let Some(l) = vals.last_mut() { if val != 0.0 { *l /= val; } } }
            "+" | "-" => { ops.push(op); vals.push(val); }
            _ => break,
        }
        i += 2;
    }
    // Pass 2: + - left-to-right.
    let mut acc = vals[0];
    for (k, op) in ops.iter().enumerate() {
        match *op { "+" => acc += vals[k + 1], "-" => acc -= vals[k + 1], _ => {} }
    }
    acc
}
