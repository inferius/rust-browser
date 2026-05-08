//! CSS <length> parsing - px/em/rem/vw/vh/% support.

/// Parse delku v px/em/rem/vw/vh/% (bez kontextu - vraci 0 pro %).
/// Delsi suffixy musi byt kontrolovany drive.
pub fn parse_length(s: &str) -> f32 {
    parse_length_ctx(s, 1024.0, 768.0, 16.0)
}

/// Parse delky s viewport kontextem (pro vw/vh/% support).
pub fn parse_length_ctx(s: &str, vw: f32, vh: f32, parent_size: f32) -> f32 {
    let s = s.trim();
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
