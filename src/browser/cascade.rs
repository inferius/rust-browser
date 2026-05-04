/// CSS cascade - aplikace stylesheets na DOM strom.
///
/// Vrati StyleMap: pro kazdy element computed styles (HashMap<String, String>).
/// Specificita rozhoduje pri kolizi.

use std::collections::HashMap;
use std::rc::Rc;
use super::dom::{Node, NodeKind};
use super::css_parser::{Stylesheet, Selector, SimpleSelector, Combinator, specificity};

/// Expanduje CSS shorthand props (margin/padding/border) do longhand.
/// Napr. "margin: 10px 20px;" -> margin-top:10, margin-right:20, margin-bottom:10, margin-left:20.
/// "border: 1px solid red;" -> border-width:1, border-style:solid, border-color:red.
pub fn expand_shorthand(prop: &str, value: &str, out: &mut HashMap<String, String>) {
    // CSS Logical Properties L1 - mapping na fyzicke (predpokladam LTR + horizontal-tb)
    if let Some(physical) = logical_to_physical(prop) {
        out.insert(physical.into(), value.into());
        out.insert(prop.into(), value.into()); // zachovat puvodni jmeno
        return;
    }
    // Logical shorthand (margin-block, margin-inline, inset)
    if let Some((p1, p2)) = logical_shorthand_pair(prop) {
        let parts: Vec<&str> = value.split_whitespace().collect();
        let (a, b) = match parts.len() {
            1 => (parts[0], parts[0]),
            2 => (parts[0], parts[1]),
            _ => (parts[0], parts.get(1).copied().unwrap_or(parts[0])),
        };
        out.insert(p1.into(), a.into());
        out.insert(p2.into(), b.into());
        out.insert(prop.into(), value.into());
        return;
    }
    // place-content / place-items / place-self shorthandy: <align> <justify>
    if matches!(prop, "place-content" | "place-items" | "place-self") {
        let parts: Vec<&str> = value.split_whitespace().collect();
        let (align, justify) = match parts.len() {
            1 => (parts[0], parts[0]),
            _ => (parts[0], parts[1]),
        };
        let (align_prop, justify_prop) = match prop {
            "place-content" => ("align-content", "justify-content"),
            "place-items"   => ("align-items", "justify-items"),
            "place-self"    => ("align-self", "justify-self"),
            _ => unreachable!(),
        };
        out.insert(align_prop.into(), align.into());
        out.insert(justify_prop.into(), justify.into());
        out.insert(prop.into(), value.into());
        return;
    }
    // gap shorthand: <row-gap> <column-gap>
    if prop == "gap" {
        let parts: Vec<&str> = value.split_whitespace().collect();
        let (row, col) = match parts.len() {
            1 => (parts[0], parts[0]),
            _ => (parts[0], parts[1]),
        };
        out.insert("row-gap".into(), row.into());
        out.insert("column-gap".into(), col.into());
        out.insert("gap".into(), value.into());
        return;
    }
    if prop == "inset" {
        // inset = top right bottom left (analog margin)
        let parts: Vec<&str> = value.split_whitespace().collect();
        let (t, r, b, l) = match parts.len() {
            1 => (parts[0], parts[0], parts[0], parts[0]),
            2 => (parts[0], parts[1], parts[0], parts[1]),
            3 => (parts[0], parts[1], parts[2], parts[1]),
            4 => (parts[0], parts[1], parts[2], parts[3]),
            _ => return,
        };
        out.insert("top".into(), t.into());
        out.insert("right".into(), r.into());
        out.insert("bottom".into(), b.into());
        out.insert("left".into(), l.into());
        out.insert("inset".into(), value.into());
        return;
    }
    match prop {
        "margin" | "padding" => {
            let parts: Vec<&str> = value.split_whitespace().collect();
            let (t, r, b, l) = match parts.len() {
                1 => (parts[0], parts[0], parts[0], parts[0]),
                2 => (parts[0], parts[1], parts[0], parts[1]),
                3 => (parts[0], parts[1], parts[2], parts[1]),
                4 => (parts[0], parts[1], parts[2], parts[3]),
                _ => return,
            };
            out.insert(format!("{prop}-top"),    t.into());
            out.insert(format!("{prop}-right"),  r.into());
            out.insert(format!("{prop}-bottom"), b.into());
            out.insert(format!("{prop}-left"),   l.into());
            out.insert(prop.into(), value.into()); // shorthand zachovan pro existing read
        }
        "border" | "outline" => {
            // "1px solid red" - parse postupne
            let parts: Vec<&str> = value.split_whitespace().collect();
            let prefix = prop;
            for p in &parts {
                if p.ends_with("px") || p.ends_with("em") || p.ends_with("rem") {
                    out.insert(format!("{prefix}-width"), p.to_string());
                } else if matches!(*p, "solid" | "dashed" | "dotted" | "double" | "none" | "groove" | "ridge" | "inset" | "outset") {
                    out.insert(format!("{prefix}-style"), p.to_string());
                } else if super::layout::parse_color(p).is_some() {
                    out.insert(format!("{prefix}-color"), p.to_string());
                }
            }
            out.insert(prop.into(), value.into());
        }
        "background" => {
            // Zjednoduseno: pokud je color, ulozit jako background-color
            if super::layout::parse_color(value).is_some() {
                out.insert("background-color".into(), value.into());
            }
            out.insert("background".into(), value.into());
        }
        "font" => {
            // "16px Arial" / "bold 14px Verdana" - parse size + family
            for p in value.split_whitespace() {
                if p.ends_with("px") || p.ends_with("em") || p.ends_with("rem") {
                    out.insert("font-size".into(), p.into());
                } else if p == "bold" {
                    out.insert("font-weight".into(), "bold".into());
                } else if p == "italic" {
                    out.insert("font-style".into(), "italic".into());
                }
            }
            out.insert("font".into(), value.into());
        }
        _ => {
            out.insert(prop.into(), value.into());
        }
    }
}

/// Mapuje CSS Logical Property na fyzickou (LTR + horizontal-tb).
/// Vrati None kdyz prop neni logicka.
pub fn logical_to_physical(prop: &str) -> Option<&'static str> {
    Some(match prop {
        // Margin
        "margin-block-start"  => "margin-top",
        "margin-block-end"    => "margin-bottom",
        "margin-inline-start" => "margin-left",
        "margin-inline-end"   => "margin-right",
        // Padding
        "padding-block-start"  => "padding-top",
        "padding-block-end"    => "padding-bottom",
        "padding-inline-start" => "padding-left",
        "padding-inline-end"   => "padding-right",
        // Border width
        "border-block-start-width"  => "border-top-width",
        "border-block-end-width"    => "border-bottom-width",
        "border-inline-start-width" => "border-left-width",
        "border-inline-end-width"   => "border-right-width",
        // Border style
        "border-block-start-style"  => "border-top-style",
        "border-block-end-style"    => "border-bottom-style",
        "border-inline-start-style" => "border-left-style",
        "border-inline-end-style"   => "border-right-style",
        // Border color
        "border-block-start-color"  => "border-top-color",
        "border-block-end-color"    => "border-bottom-color",
        "border-inline-start-color" => "border-left-color",
        "border-inline-end-color"   => "border-right-color",
        // Border radius (logicke rohy)
        "border-start-start-radius" => "border-top-left-radius",
        "border-start-end-radius"   => "border-top-right-radius",
        "border-end-start-radius"   => "border-bottom-left-radius",
        "border-end-end-radius"     => "border-bottom-right-radius",
        // Inset
        "inset-block-start"  => "top",
        "inset-block-end"    => "bottom",
        "inset-inline-start" => "left",
        "inset-inline-end"   => "right",
        // Size
        "block-size"      => "height",
        "inline-size"     => "width",
        "min-block-size"  => "min-height",
        "min-inline-size" => "min-width",
        "max-block-size"  => "max-height",
        "max-inline-size" => "max-width",
        _ => return None,
    })
}

/// Logicka shorthand -> par fyzickych properties.
fn logical_shorthand_pair(prop: &str) -> Option<(&'static str, &'static str)> {
    Some(match prop {
        "margin-block"   => ("margin-top", "margin-bottom"),
        "margin-inline"  => ("margin-left", "margin-right"),
        "padding-block"  => ("padding-top", "padding-bottom"),
        "padding-inline" => ("padding-left", "padding-right"),
        "inset-block"    => ("top", "bottom"),
        "inset-inline"   => ("left", "right"),
        _ => return None,
    })
}

/// Mapa: pointer na Node -> computed styles.
pub type StyleMap = HashMap<usize, HashMap<String, String>>;

/// Mapa: (node_id, pseudo-element-name) -> computed styles.
/// Napr. ((0xabcd, "before"), {"content": "\"->\"", "color": "red"})
pub type PseudoStyleMap = HashMap<(usize, String), HashMap<String, String>>;

/// Pomocnik: vrati pointer hodnotu Rc<Node> jako klic.
fn node_id(node: &Rc<Node>) -> usize {
    Rc::as_ptr(node) as usize
}

/// Resolvuje CSS var(--name), env(), calc(), min(), max(), clamp() expressions.
/// Pri var(--x, fallback): pokud --x v variables, pouzij ho, jinak fallback.
pub fn resolve_value(value: &str, variables: &HashMap<String, String>) -> String {
    let mut out = value.to_string();
    // Iterativne resolvujem do fixed pointu (max 10 prochodu).
    // var() muze obsahovat calc(), calc() muze obsahovat min(), atd.
    for _ in 0..10 {
        let before = out.clone();
        if out.contains("var(") {
            out = replace_var_once(&out, variables);
        }
        if out.contains("env(") {
            out = resolve_env(&out);
        }
        if out.contains("min(") || out.contains("max(") || out.contains("clamp(")
            || out.contains("abs(") || out.contains("sign(") || out.contains("sqrt(")
            || out.contains("round(") || out.contains("floor(") || out.contains("ceil(")
            || out.contains("exp(") || out.contains("log(") || out.contains("pow(")
            || out.contains("hypot(") || out.contains("mod(") || out.contains("rem(")
            || out.contains("sin(") || out.contains("cos(") || out.contains("tan(")
            || out.contains("asin(") || out.contains("acos(") || out.contains("atan(")
        {
            out = resolve_math_func(&out);
        }
        if out.contains("calc(") {
            out = resolve_calc(&out);
        }
        if out == before { break; }
    }
    out
}

/// env(safe-area-inset-top, fallback) - bez safe-area kontextu vrati fallback nebo 0px.
fn resolve_env(s: &str) -> String {
    let mut out = String::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 4 <= bytes.len() && &bytes[i..i+4] == b"env(" {
            let mut depth = 1;
            let mut j = i + 4;
            while j < bytes.len() && depth > 0 {
                match bytes[j] {
                    b'(' => depth += 1,
                    b')' => depth -= 1,
                    _ => {}
                }
                if depth == 0 { break; }
                j += 1;
            }
            let inner = &s[i+4..j];
            // Format: "name" nebo "name, fallback"
            let fallback = inner.find(',').map(|idx| inner[idx+1..].trim().to_string());
            let val = fallback.unwrap_or_else(|| "0px".to_string());
            out.push_str(&val);
            i = j + 1;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// Resolvuje min(a, b, ...), max(a, b, ...), clamp(min, val, max).
/// Najde nejvnitrnejsi vyskyt (zaden child neni mezi argumenty), pak iterativne.
fn resolve_math_func(s: &str) -> String {
    let names = [
        "min(", "max(", "clamp(",
        "abs(", "sign(", "sqrt(", "round(", "floor(", "ceil(",
        "exp(", "log(", "pow(", "hypot(", "mod(", "rem(",
        "sin(", "cos(", "tan(", "asin(", "acos(", "atan(", "atan2(",
    ];
    let mut out = s.to_string();
    loop {
        let bytes: Vec<u8> = out.as_bytes().to_vec();
        let mut found: Option<(usize, usize, &str)> = None;
        // Najdi nejvnitrnejsi (nejlevejsi po procesu, kde uvnitr neni dalsi math func)
        'outer: for (idx, _) in bytes.iter().enumerate() {
            for &name in &names {
                let nb = name.as_bytes();
                if idx + nb.len() <= bytes.len() && &bytes[idx..idx + nb.len()] == nb {
                    // Najdi matching )
                    let mut depth = 1;
                    let mut j = idx + nb.len();
                    while j < bytes.len() && depth > 0 {
                        match bytes[j] {
                            b'(' => depth += 1,
                            b')' => depth -= 1,
                            _ => {}
                        }
                        if depth == 0 { break; }
                        j += 1;
                    }
                    if j >= bytes.len() { break 'outer; }
                    // Zkontroluj ze argumenty NEobsahuji dalsi math func (kromě calc)
                    let inner = &out[idx + nb.len()..j];
                    let has_inner = names.iter().any(|n| inner.contains(*n));
                    if !has_inner {
                        found = Some((idx, j, name.trim_end_matches('(')));
                        break 'outer;
                    }
                }
            }
        }
        let (start, end, fname) = match found { Some(t) => t, None => break };
        let nb_len = fname.len() + 1; // +1 pro '('
        let inner = out[start + nb_len..end].to_string();
        let result = eval_math_func(fname, &inner);
        out.replace_range(start..end + 1, &result);
    }
    out
}

fn eval_math_func(name: &str, args: &str) -> String {
    let parts: Vec<&str> = args.split(',').map(|s| s.trim()).collect();
    if parts.is_empty() { return args.to_string(); }

    // Parsuj kazdy argument: vrati (number, unit_string).
    let parsed: Vec<(f32, String)> = parts.iter().map(|p| parse_value_with_unit(p)).collect();
    if parsed.is_empty() { return args.to_string(); }

    // Pouzij jednotku z prvniho argumentu jako vystupni
    let unit = parsed[0].1.clone();
    let nums: Vec<f32> = parsed.iter().map(|(n, _)| *n).collect();

    let result = match name {
        "min" => nums.iter().cloned().fold(f32::INFINITY, f32::min),
        "max" => nums.iter().cloned().fold(f32::NEG_INFINITY, f32::max),
        "clamp" if nums.len() >= 3 => {
            let lo = nums[0]; let val = nums[1]; let hi = nums[2];
            val.max(lo).min(hi)
        }
        // Math funkce L4 - vsechny pracuji v jednotkach prvniho argumentu
        "abs"   => nums[0].abs(),
        "sign"  => nums[0].signum(),
        "sqrt"  => nums[0].sqrt(),
        "round" => nums[0].round(),
        "floor" => nums[0].floor(),
        "ceil"  => nums[0].ceil(),
        "exp"   => nums[0].exp(),
        "log"   if nums.len() == 1 => nums[0].ln(),
        "log"   if nums.len() == 2 => nums[0].log(nums[1]),
        "pow"   if nums.len() == 2 => nums[0].powf(nums[1]),
        "hypot" => nums.iter().map(|x| x * x).sum::<f32>().sqrt(),
        "mod"   if nums.len() == 2 => nums[0].rem_euclid(nums[1]),
        "rem"   if nums.len() == 2 => nums[0] % nums[1],
        "sin"   => nums[0].to_radians().sin(),
        "cos"   => nums[0].to_radians().cos(),
        "tan"   => nums[0].to_radians().tan(),
        "asin"  => nums[0].asin().to_degrees(),
        "acos"  => nums[0].acos().to_degrees(),
        "atan"  => nums[0].atan().to_degrees(),
        "atan2" if nums.len() == 2 => nums[0].atan2(nums[1]).to_degrees(),
        _ => return args.to_string(),
    };
    // Trigonometrie sin/cos/tan + sqrt + exp + log + sign: vraci ciste cislo.
    // asin/acos/atan/atan2: vraci stupne (deg).
    let unitless = matches!(name,
        "sqrt" | "exp" | "log" | "sign" | "pow" | "hypot"
        | "sin" | "cos" | "tan");
    let angle = matches!(name, "asin" | "acos" | "atan" | "atan2");

    if unitless {
        format!("{result}")
    } else if angle {
        format!("{result}deg")
    } else if unit.is_empty() {
        format!("{result}")
    } else {
        format!("{result}{unit}")
    }
}

/// Parsuje hodnotu typu "12.5px", "100%", "2em", "42" -> (number, "px").
fn parse_value_with_unit(s: &str) -> (f32, String) {
    let s = s.trim();
    let units = ["px", "em", "rem", "vw", "vh", "vmin", "vmax", "pt", "%",
                 "ch", "ex", "lh", "rlh", "cqw", "cqh", "cqi", "cqb",
                 "deg", "rad", "turn", "ms", "s"];
    for u in &units {
        if let Some(num_part) = s.strip_suffix(u) {
            if let Ok(n) = num_part.trim().parse::<f32>() {
                return (n, u.to_string());
            }
        }
    }
    if let Ok(n) = s.parse::<f32>() {
        return (n, String::new());
    }
    (0.0, String::new())
}

fn replace_var_once(s: &str, variables: &HashMap<String, String>) -> String {
    let mut out = String::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 4 < bytes.len() && &bytes[i..i+4] == b"var(" {
            // Najdi matching )
            let mut depth = 1;
            let mut j = i + 4;
            while j < bytes.len() && depth > 0 {
                match bytes[j] {
                    b'(' => depth += 1,
                    b')' => depth -= 1,
                    _ => {}
                }
                if depth == 0 { break; }
                j += 1;
            }
            let inner = &s[i+4..j];
            // Split na name + fallback
            let (name, fallback) = match inner.find(',') {
                Some(idx) => (inner[..idx].trim(), Some(inner[idx+1..].trim())),
                None      => (inner.trim(), None),
            };
            let resolved = variables.get(name).cloned()
                .or_else(|| fallback.map(|f| f.to_string()))
                .unwrap_or_default();
            out.push_str(&resolved);
            i = j + 1;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn resolve_calc(s: &str) -> String {
    let mut out = String::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 5 < bytes.len() && &bytes[i..i+5] == b"calc(" {
            let mut depth = 1;
            let mut j = i + 5;
            while j < bytes.len() && depth > 0 {
                match bytes[j] {
                    b'(' => depth += 1,
                    b')' => depth -= 1,
                    _ => {}
                }
                if depth == 0 { break; }
                j += 1;
            }
            let expr = &s[i+5..j];
            let result = eval_calc_expr(expr);
            out.push_str(&result);
            i = j + 1;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// Velmi zjednoduseny calc evaluator - vstupy "Npx + Npx", "Nem * 2".
fn eval_calc_expr(expr: &str) -> String {
    // Najdi unit - pouzij prvni numerickou hodnotu
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() < 3 {
        return expr.trim().to_string();
    }

    let mut acc = super::layout::parse_length(parts[0]);
    let mut unit = "px".to_string();
    if let Some(u) = ["px", "em", "rem", "%"].iter().find(|u| parts[0].ends_with(*u)) {
        unit = u.to_string();
    }

    let mut i = 1;
    while i + 1 < parts.len() {
        let op = parts[i];
        let val = super::layout::parse_length(parts[i+1]);
        match op {
            "+" => acc += val,
            "-" => acc -= val,
            "*" => acc *= val,
            "/" => if val != 0.0 { acc /= val; },
            _ => break,
        }
        i += 2;
    }
    format!("{}{}", acc, unit)
}

/// Cascade s viewport pro @media queries + @container queries.
/// Pro @container: zatim aproximace - container size je root viewport. Pro
/// presnou implementaci by se musel evaluovat per-element po layout pass
/// (kruhova zavislost s layoutem).
pub fn cascade_with_viewport(root: &Rc<Node>, stylesheets: &[Stylesheet],
                              viewport_w: f32, viewport_h: f32) -> StyleMap {
    // Sjednotit rules + matching media query + matching container query rules
    let mut effective: Vec<Stylesheet> = Vec::new();
    for sheet in stylesheets {
        let mut combined = sheet.clone();
        // Aplikuj jen vyhovujici media queries
        for mq in &sheet.media_queries {
            if super::css_parser::evaluate_media_query(&mq.query, viewport_w, viewport_h) {
                combined.rules.extend(mq.rules.clone());
            }
        }
        // Aplikuj container queries - pro start s viewport jako approximation
        // container size. TODO: per-element ancestor lookup.
        for cq in &sheet.container_queries {
            if super::css_parser::evaluate_container_query(&cq.condition, viewport_w, viewport_h) {
                combined.rules.extend(cq.rules.clone());
            }
        }
        combined.media_queries.clear();
        combined.container_queries.clear();
        effective.push(combined);
    }
    cascade(root, &effective)
}

/// Aplikuje stylesheet na DOM strom, vrati StyleMap.
pub fn cascade(root: &Rc<Node>, stylesheets: &[Stylesheet]) -> StyleMap {
    let mut style_map: StyleMap = HashMap::new();
    // Globalni :root variables - resolved jednou
    let mut variables: HashMap<String, String> = HashMap::new();
    for sheet in stylesheets {
        for rule in &sheet.rules {
            for sel in &rule.selectors {
                let is_root = sel.parts.iter().any(|p|
                    p.tag.as_deref() == Some("html") ||
                    p.pseudo_classes.iter().any(|pc| pc == "root")
                ) || sel.parts.is_empty();
                if !is_root && !sel.parts.iter().any(|p| p.tag.as_deref() == Some(":root")) {
                    // Selektor :root nebo html
                    continue;
                }
                for decl in &rule.declarations {
                    if decl.property.starts_with("--") {
                        variables.insert(decl.property.clone(), decl.value.clone());
                    }
                }
            }
        }
    }

    // Prochazime DOM, pro kazdy element zkontrolujeme vsechny rules
    root.walk(&mut |node| {
        if !matches!(node.kind, NodeKind::Element { .. }) { return; }

        let mut matched_decls: Vec<((u32, u32, u32, usize), &super::css_parser::Declaration)> = Vec::new();
        let mut order = 0;

        for sheet in stylesheets {
            // Layered rules nejprve (nizsi prio) - per CSS Cascade Layers L5.
            // Layer order: pozdejsi v `layer_order` ma vyssi prio v ramci layered.
            // Vsechny layered jsou pod unlayered.
            for (layer_name, rules) in &sheet.layered_rules {
                let layer_priority = sheet.layer_order.iter().position(|n| n == layer_name)
                    .unwrap_or(0) as u32;
                for rule in rules {
                    for sel in &rule.selectors {
                        if sel.parts.last().map(|p| p.pseudo_element.is_some()).unwrap_or(false) {
                            continue;
                        }
                        if matches_selector(node, sel) {
                            let spec = specificity(sel);
                            for decl in &rule.declarations {
                                // Layer priority je nizsi nez unlayered (kterym dame "important_offset" 0)
                                // Layered: priority bit = 0, dale layer_priority pro razeni mezi layery
                                let key = (
                                    if decl.important { 1 } else { 0 },
                                    layer_priority, // nizsi 1. komponent => layered jdou nahoru
                                    spec.0 * 1000 + spec.1 + spec.2,
                                    order,
                                );
                                matched_decls.push(((key.0, key.1, key.2, key.3), decl));
                                order += 1;
                            }
                        }
                    }
                }
            }
            // Unlayered (default) - nejvyssi prio (po !important).
            // Pouzivame layer_priority = u32::MAX aby unlayered prepsalo layered.
            for rule in &sheet.rules {
                for sel in &rule.selectors {
                    // Pseudo-element selektory aplikujem v cascade_pseudo, ne tady
                    if sel.parts.last().map(|p| p.pseudo_element.is_some()).unwrap_or(false) {
                        continue;
                    }
                    if matches_selector(node, sel) {
                        let spec = specificity(sel);
                        for decl in &rule.declarations {
                            let key = (
                                if decl.important { 1 } else { 0 },
                                u32::MAX, // unlayered = nejvyssi
                                spec.0 * 1000 + spec.1 + spec.2,
                                order,
                            );
                            matched_decls.push(((key.0, key.1, key.2, key.3), decl));
                            order += 1;
                        }
                    }
                }
            }
        }

        // Sort podle (important, id_count, class+type, order) - vyssi kombinace vyhrava
        matched_decls.sort_by(|a, b| a.0.cmp(&b.0));

        let mut styles = HashMap::new();
        for (_, decl) in matched_decls {
            let resolved = resolve_value(&decl.value, &variables);
            expand_shorthand(&decl.property, &resolved, &mut styles);
        }

        // Inline styly z attributu "style" maji nejvyssi prioritu (mimo !important rules)
        if let Some(inline) = node.attr("style") {
            for pair in inline.split(';') {
                if let Some(colon) = pair.find(':') {
                    let prop = pair[..colon].trim().to_string();
                    let val = pair[colon+1..].trim().to_string();
                    if !prop.is_empty() && !val.is_empty() {
                        let resolved = resolve_value(&val, &variables);
                        expand_shorthand(&prop, &resolved, &mut styles);
                    }
                }
            }
        }

        style_map.insert(node_id(node), styles);
    });

    style_map
}

/// Cascade jen pro pseudo-elements (::before / ::after / ...).
/// Vraci mapu (node_id, pseudo_name) -> computed styles, pro elementy co matchuji
/// selektor s pseudo_element.
pub fn cascade_pseudo(root: &Rc<Node>, stylesheets: &[Stylesheet]) -> PseudoStyleMap {
    let mut out: PseudoStyleMap = HashMap::new();

    // Recyclujeme variables z hlavniho cascade (jen :root)
    let mut variables: HashMap<String, String> = HashMap::new();
    for sheet in stylesheets {
        for rule in &sheet.rules {
            for sel in &rule.selectors {
                let is_root = sel.parts.iter().any(|p|
                    p.tag.as_deref() == Some("html") ||
                    p.pseudo_classes.iter().any(|pc| pc == "root")
                ) || sel.parts.is_empty();
                if !is_root { continue; }
                for decl in &rule.declarations {
                    if decl.property.starts_with("--") {
                        variables.insert(decl.property.clone(), decl.value.clone());
                    }
                }
            }
        }
    }

    root.walk(&mut |node| {
        if !matches!(node.kind, NodeKind::Element { .. }) { return; }

        // Pro kazdy pseudo-element name shromazdime matched declarations
        let mut by_pseudo: HashMap<String, Vec<((u32, u32, u32, usize), &super::css_parser::Declaration)>>
            = HashMap::new();
        let mut order = 0;

        for sheet in stylesheets {
            for rule in &sheet.rules {
                for sel in &rule.selectors {
                    // Najdi pseudo_element v poslední casti selectoru
                    let pe = sel.parts.last().and_then(|p| p.pseudo_element.clone());
                    let pe = match pe { Some(p) => p, None => continue };
                    if !matches_selector(node, sel) { continue; }
                    let spec = specificity(sel);
                    for decl in &rule.declarations {
                        let key = (
                            if decl.important { 1 } else { 0 },
                            spec.0,
                            spec.1 + spec.2,
                            order,
                        );
                        by_pseudo.entry(pe.clone()).or_default().push((key, decl));
                        order += 1;
                    }
                }
            }
        }

        for (pe_name, mut list) in by_pseudo {
            list.sort_by(|a, b| a.0.cmp(&b.0));
            let mut styles = HashMap::new();
            for (_, decl) in list {
                let resolved = resolve_value(&decl.value, &variables);
                expand_shorthand(&decl.property, &resolved, &mut styles);
            }
            out.insert((node_id(node), pe_name), styles);
        }
    });

    out
}

/// Vrati pseudo-element styles pro dany node + name (pomocnik).
pub fn get_pseudo_styles<'a>(map: &'a PseudoStyleMap, node: &Rc<Node>, pseudo: &str)
    -> Option<&'a HashMap<String, String>>
{
    map.get(&(node_id(node), pseudo.to_string()))
}

/// Kontrola jestli selektor matchuje uzel.
/// Pro multi-part selektory chodime parents.
pub fn matches_selector(node: &Rc<Node>, sel: &Selector) -> bool {
    if sel.parts.is_empty() { return false; }
    // Posledni cast musi matchovat node
    let last = &sel.parts[sel.parts.len() - 1];
    if !matches_simple(node, last) { return false; }

    // Pokud jen jedna cast, hotovo
    if sel.parts.len() == 1 { return true; }

    // Vice casti - chodime po parents
    let mut current_part = sel.parts.len() - 2;
    let mut current_node = node.parent.borrow().upgrade();

    // Pro sibling combinatory drzime aktualni "scope node" - pri prvni iteraci
    // je to puvodni `node`, jeho parent je current_node uz nastavene.
    let mut scope_node = Rc::clone(node);

    loop {
        let part = &sel.parts[current_part];
        let combinator = sel.parts[current_part + 1].combinator.clone()
            .unwrap_or(Combinator::Descendant);

        match combinator {
            Combinator::Child => {
                let p_clone = current_node.clone();
                if let Some(p) = p_clone {
                    if !matches_simple(&p, part) { return false; }
                    if current_part == 0 { return true; }
                    current_part -= 1;
                    let next = p.parent.borrow().upgrade();
                    scope_node = Rc::clone(&p);
                    current_node = next;
                } else { return false; }
            }
            Combinator::Descendant => {
                let mut found = false;
                loop {
                    let p_clone = current_node.clone();
                    let p = match p_clone { Some(p) => p, None => break };
                    if matches_simple(&p, part) {
                        if current_part == 0 { return true; }
                        current_part -= 1;
                        let next = p.parent.borrow().upgrade();
                        scope_node = Rc::clone(&p);
                        current_node = next;
                        found = true;
                        break;
                    }
                    let next = p.parent.borrow().upgrade();
                    current_node = next;
                }
                if !found { return false; }
            }
            Combinator::AdjacentSibling => {
                // Predchazejici sourozenec scope_node musi matchovat part
                let parent = scope_node.parent.borrow().upgrade();
                let parent = match parent { Some(p) => p, None => return false };
                let children = parent.children.borrow();
                let idx = children.iter().position(|c| Rc::ptr_eq(c, &scope_node));
                let idx = match idx { Some(i) => i, None => return false };
                // Najdi predchazejici element (skip text/comment)
                let mut prev: Option<Rc<Node>> = None;
                for j in (0..idx).rev() {
                    if matches!(children[j].kind, NodeKind::Element(_)) {
                        prev = Some(Rc::clone(&children[j]));
                        break;
                    }
                }
                let prev = match prev { Some(p) => p, None => return false };
                if !matches_simple(&prev, part) { return false; }
                if current_part == 0 { return true; }
                current_part -= 1;
                scope_node = Rc::clone(&prev);
                current_node = prev.parent.borrow().upgrade();
            }
            Combinator::GeneralSibling => {
                // Nektery predchazejici sourozenec musi matchovat part
                let parent = scope_node.parent.borrow().upgrade();
                let parent = match parent { Some(p) => p, None => return false };
                let children = parent.children.borrow();
                let idx = children.iter().position(|c| Rc::ptr_eq(c, &scope_node));
                let idx = match idx { Some(i) => i, None => return false };
                let mut found: Option<Rc<Node>> = None;
                for j in (0..idx).rev() {
                    if matches!(children[j].kind, NodeKind::Element(_))
                        && matches_simple(&children[j], part)
                    {
                        found = Some(Rc::clone(&children[j]));
                        break;
                    }
                }
                let prev = match found { Some(p) => p, None => return false };
                if current_part == 0 { return true; }
                current_part -= 1;
                scope_node = Rc::clone(&prev);
                current_node = prev.parent.borrow().upgrade();
            }
        }
    }
}

/// Kontroluje simple selector proti uzlu.
pub fn matches_simple(node: &Rc<Node>, sel: &SimpleSelector) -> bool {
    use super::css_parser::AttrOp;

    let tag = match node.tag_name() {
        Some(t) => t,
        None => return false,
    };

    if let Some(want_tag) = &sel.tag {
        if want_tag != "*" && want_tag.to_lowercase() != tag {
            return false;
        }
    }

    if let Some(want_id) = &sel.id {
        if node.attr("id").as_deref() != Some(want_id.as_str()) {
            return false;
        }
    }

    if !sel.classes.is_empty() {
        let class_attr = node.attr("class").unwrap_or_default();
        let classes: Vec<&str> = class_attr.split_whitespace().collect();
        for required in &sel.classes {
            if !classes.contains(&required.as_str()) {
                return false;
            }
        }
    }

    // Atribute selektory
    for attr_sel in &sel.attributes {
        let actual = node.attr(&attr_sel.name);
        match (&attr_sel.op, &attr_sel.value, &actual) {
            (AttrOp::Exists, _, None) => return false,
            (AttrOp::Exists, _, Some(_)) => {}
            (_, _, None) => return false,
            (AttrOp::Equals, Some(want), Some(got)) => {
                if want != got { return false; }
            }
            (AttrOp::Contains, Some(want), Some(got)) => {
                if !got.contains(want.as_str()) { return false; }
            }
            (AttrOp::StartsWith, Some(want), Some(got)) => {
                if !got.starts_with(want.as_str()) { return false; }
            }
            (AttrOp::EndsWith, Some(want), Some(got)) => {
                if !got.ends_with(want.as_str()) { return false; }
            }
            (AttrOp::WordContains, Some(want), Some(got)) => {
                if !got.split_whitespace().any(|w| w == want) { return false; }
            }
            _ => {}
        }
    }

    // Pseudo-classes (bez argumentu)
    for pc in &sel.pseudo_classes {
        match pc.as_str() {
            "root" => {
                if tag != "html" { return false; }
            }
            "first-child" => {
                let parent = node.parent.borrow().upgrade();
                if let Some(p) = parent {
                    let children = p.children.borrow();
                    let first_el = children.iter().find(|c| matches!(c.kind, NodeKind::Element(_)));
                    if first_el.map(|f| !Rc::ptr_eq(f, node)).unwrap_or(true) {
                        return false;
                    }
                }
            }
            "last-child" => {
                let parent = node.parent.borrow().upgrade();
                if let Some(p) = parent {
                    let children = p.children.borrow();
                    let last_el = children.iter().rev().find(|c| matches!(c.kind, NodeKind::Element(_)));
                    if last_el.map(|f| !Rc::ptr_eq(f, node)).unwrap_or(true) {
                        return false;
                    }
                }
            }
            "only-child" => {
                let parent = node.parent.borrow().upgrade();
                if let Some(p) = parent {
                    let children = p.children.borrow();
                    let count = children.iter().filter(|c| matches!(c.kind, NodeKind::Element(_))).count();
                    if count != 1 { return false; }
                }
            }
            "first-of-type" | "last-of-type" | "only-of-type" => {
                let parent = node.parent.borrow().upgrade();
                if let Some(p) = parent {
                    let children = p.children.borrow();
                    let same_tag: Vec<_> = children.iter()
                        .filter(|c| matches!(c.kind, NodeKind::Element(_)))
                        .filter(|c| c.tag_name().as_deref() == Some(tag.as_str()))
                        .collect();
                    let pos = same_tag.iter().position(|c| Rc::ptr_eq(c, node));
                    let pos = match pos { Some(p) => p, None => return false };
                    match pc.as_str() {
                        "first-of-type" => if pos != 0 { return false; },
                        "last-of-type" => if pos != same_tag.len() - 1 { return false; },
                        "only-of-type" => if same_tag.len() != 1 { return false; },
                        _ => {}
                    }
                }
            }
            "empty" => {
                let children = node.children.borrow();
                let has_content = children.iter().any(|c| match &c.kind {
                    NodeKind::Element(_) => true,
                    NodeKind::Text(t) => !t.is_empty(),
                    _ => false,
                });
                if has_content { return false; }
            }
            "any-link" | "scope" => { /* OK */ }
            // Form attribute pseudo-classes - lze staticky overit z DOM attributes
            "required" => {
                if node.attr("required").is_none() { return false; }
            }
            "optional" => {
                // :optional - jen na form input/select/textarea co NEMA required
                let is_form = matches!(tag.as_str(), "input" | "select" | "textarea");
                if !is_form || node.attr("required").is_some() { return false; }
            }
            "disabled" => {
                if node.attr("disabled").is_none() { return false; }
            }
            "enabled" => {
                let is_form = matches!(tag.as_str(), "input" | "select" | "textarea" | "button");
                if !is_form || node.attr("disabled").is_some() { return false; }
            }
            "checked" => {
                // checkbox / radio s checked attributem
                if node.attr("checked").is_none() { return false; }
            }
            "read-only" => {
                let is_form = matches!(tag.as_str(), "input" | "textarea");
                if !is_form { return false; }
                // readonly attribut nebo not text-like input
                if node.attr("readonly").is_none() {
                    return false;
                }
            }
            "read-write" => {
                let is_form = matches!(tag.as_str(), "input" | "textarea");
                if !is_form || node.attr("readonly").is_some() || node.attr("disabled").is_some() {
                    return false;
                }
            }
            "placeholder-shown" => {
                // :placeholder-shown match pokud value je prazdne a element ma placeholder
                let has_placeholder = node.attr("placeholder").is_some();
                let value_empty = node.attr("value").map(|v| v.is_empty()).unwrap_or(true);
                if !has_placeholder || !value_empty { return false; }
            }
            "valid" => {
                // :valid match pokud form input s required ma neprazdnou hodnotu
                let is_form = matches!(tag.as_str(), "input" | "select" | "textarea" | "form");
                if !is_form { return false; }
                if node.attr("required").is_some() {
                    let val = node.attr("value").unwrap_or_default();
                    if val.is_empty() { return false; }
                }
                // type="email" - musi obsahovat @
                if let Some(ty) = node.attr("type") {
                    if ty == "email" {
                        let val = node.attr("value").unwrap_or_default();
                        if !val.is_empty() && !val.contains('@') { return false; }
                    }
                }
            }
            "invalid" => {
                let is_form = matches!(tag.as_str(), "input" | "select" | "textarea" | "form");
                if !is_form { return false; }
                let mut is_invalid = false;
                if node.attr("required").is_some() {
                    let val = node.attr("value").unwrap_or_default();
                    if val.is_empty() { is_invalid = true; }
                }
                if let Some(ty) = node.attr("type") {
                    if ty == "email" {
                        let val = node.attr("value").unwrap_or_default();
                        if !val.is_empty() && !val.contains('@') { is_invalid = true; }
                    }
                }
                if !is_invalid { return false; }
            }
            "default" => {
                // :default match pro default-checked input + button[type=submit]
                let is_default = match tag.as_str() {
                    "button" => node.attr("type").as_deref().unwrap_or("submit") == "submit",
                    "input" => node.attr("checked").is_some(),
                    _ => false,
                };
                if !is_default { return false; }
            }
            "indeterminate" | "in-range" | "out-of-range" => {
                // Vyzaduje runtime stav - skip
                return false;
            }
            // hover/active/focus - vyzaduje runtime stav - skip (rule se neaplikuje staticky)
            "hover" | "active" | "focus" | "focus-visible" | "focus-within"
            | "visited" | "link" => return false,
            _ => {}
        }
    }

    // Funkcni pseudo-classes
    for pf in &sel.pseudo_funcs {
        match pf {
            super::css_parser::PseudoFunc::Is(args)
            | super::css_parser::PseudoFunc::Where(args) => {
                if !args.iter().any(|s| matches_selector(node, s)) { return false; }
            }
            super::css_parser::PseudoFunc::Not(args) => {
                if args.iter().any(|s| matches_selector(node, s)) { return false; }
            }
            super::css_parser::PseudoFunc::Has(args) => {
                // :has(selector) - existuje descendant matchujici selector
                if !has_matching_descendant(node, args) { return false; }
            }
            super::css_parser::PseudoFunc::NthChild { a, b, of_type, last } => {
                if !nth_child_matches(node, *a, *b, *of_type, *last, &tag) { return false; }
            }
            super::css_parser::PseudoFunc::Unknown { .. } => {
                // Neznamy pseudo - nepouzit pravidlo (safe)
                return false;
            }
        }
    }

    true
}

/// :has(selector) - vrati true pokud nejaky descendant matchuje arg.
fn has_matching_descendant(node: &Rc<Node>, args: &[super::css_parser::Selector]) -> bool {
    let children = node.children.borrow();
    for child in children.iter() {
        if !matches!(child.kind, NodeKind::Element(_)) { continue; }
        if args.iter().any(|s| matches_selector(child, s)) { return true; }
        if has_matching_descendant(child, args) { return true; }
    }
    false
}

/// :nth-child / :nth-of-type / :nth-last-* matching.
/// an+b: vrati true pokud index splnuje (index = (n*a + b) pro n=0,1,2,...).
fn nth_child_matches(node: &Rc<Node>, a: i32, b: i32, of_type: bool, last: bool, tag: &str) -> bool {
    let parent = match node.parent.borrow().upgrade() { Some(p) => p, None => return false };
    let children = parent.children.borrow();
    let siblings: Vec<_> = children.iter()
        .filter(|c| matches!(c.kind, NodeKind::Element(_)))
        .filter(|c| !of_type || c.tag_name().as_deref() == Some(tag))
        .collect();
    let pos = siblings.iter().position(|c| Rc::ptr_eq(c, node));
    let pos = match pos { Some(p) => p, None => return false };
    let idx = if last { siblings.len() - 1 - pos + 1 } else { pos + 1 } as i32; // 1-based

    // Reseni an+b = idx -> (idx - b) % a == 0 a (idx - b) / a >= 0
    if a == 0 {
        return idx == b;
    }
    let diff = idx - b;
    if diff % a != 0 { return false; }
    diff / a >= 0
}

/// Vrati computed styles pro dany uzel (z StyleMap).
pub fn get_styles<'a>(map: &'a StyleMap, node: &Rc<Node>) -> Option<&'a HashMap<String, String>> {
    map.get(&node_id(node))
}

/// Parsovany shorthand `animation` property.
/// Spec je permisive co do poradi tokenu.
#[derive(Debug, Clone)]
pub struct AnimationSpec {
    pub name: String,
    pub duration_secs: f32,
    pub timing_function: String, // "linear" / "ease" / "ease-in" / "ease-out" / "ease-in-out" / "cubic-bezier(...)" / "steps(...)"
    pub iteration_count: f32,    // f32::INFINITY pro "infinite"
    pub direction: String,        // "normal" / "reverse" / "alternate" / "alternate-reverse"
    pub delay_secs: f32,
    pub fill_mode: String,        // "none" / "forwards" / "backwards" / "both"
    pub play_state: String,       // "running" / "paused"
}

impl AnimationSpec {
    pub fn from_styles(styles: &HashMap<String, String>) -> Option<AnimationSpec> {
        // Bud `animation` shorthand, nebo `animation-name` + dalsi longhand.
        let mut name: Option<String> = None;
        let mut duration: f32 = 0.0;
        let mut timing: String = "linear".into();
        let mut iter: f32 = 1.0;
        let mut direction: String = "normal".into();
        let mut delay: f32 = 0.0;

        let mut fill_mode: String = "none".into();
        let mut play_state: String = "running".into();

        // Shorthand parsing - tokenizace respektuje zavorky (cubic-bezier(...), steps(...))
        if let Some(short) = styles.get("animation") {
            for tok in tokenize_balanced(short) {
                let tok = tok.as_str();
                if let Some(s) = parse_time(tok) {
                    if duration == 0.0 { duration = s; } else { delay = s; }
                } else if tok == "infinite" {
                    iter = f32::INFINITY;
                } else if let Ok(n) = tok.parse::<f32>() {
                    iter = n;
                } else if matches!(tok, "linear" | "ease" | "ease-in" | "ease-out" | "ease-in-out" | "step-start" | "step-end")
                    || tok.starts_with("cubic-bezier(") || tok.starts_with("steps(")
                {
                    timing = tok.to_string();
                } else if matches!(tok, "normal" | "reverse" | "alternate" | "alternate-reverse") {
                    direction = tok.to_string();
                } else if matches!(tok, "none" | "forwards" | "backwards" | "both") {
                    fill_mode = tok.to_string();
                } else if matches!(tok, "running" | "paused") {
                    play_state = tok.to_string();
                } else {
                    // Predpokladej name
                    if name.is_none() { name = Some(tok.to_string()); }
                }
            }
        }

        // Longhand override
        if let Some(v) = styles.get("animation-name") { name = Some(v.trim().to_string()); }
        if let Some(v) = styles.get("animation-duration").and_then(|s| parse_time(s.trim())) { duration = v; }
        if let Some(v) = styles.get("animation-timing-function") { timing = v.trim().to_string(); }
        if let Some(v) = styles.get("animation-iteration-count") {
            iter = if v.trim() == "infinite" { f32::INFINITY } else { v.trim().parse().unwrap_or(1.0) };
        }
        if let Some(v) = styles.get("animation-direction") { direction = v.trim().to_string(); }
        if let Some(v) = styles.get("animation-delay").and_then(|s| parse_time(s.trim())) { delay = v; }
        if let Some(v) = styles.get("animation-fill-mode") { fill_mode = v.trim().to_string(); }
        if let Some(v) = styles.get("animation-play-state") { play_state = v.trim().to_string(); }

        let name = name?;
        if name == "none" || duration <= 0.0 { return None; }
        Some(AnimationSpec {
            name, duration_secs: duration, timing_function: timing,
            iteration_count: iter, direction, delay_secs: delay,
            fill_mode, play_state,
        })
    }
}

/// CSS Transitions L1 parsovany shorthand.
/// "transition: <prop> <duration> <timing-function> <delay> [, <next>]"
#[derive(Debug, Clone)]
pub struct TransitionSpec {
    pub property: String,           // "all" / "color" / "transform" / ...
    pub duration_secs: f32,
    pub timing_function: String,    // "linear" / "ease" / "cubic-bezier(...)" / ...
    pub delay_secs: f32,
}

impl TransitionSpec {
    /// Parsuje vsechny transitions z computed styles. Vraci seznam (mozne vice
    /// transitions oddelenych carkou, kazda pro jine property).
    pub fn from_styles(styles: &HashMap<String, String>) -> Vec<TransitionSpec> {
        let mut out = Vec::new();

        // Shorthand "transition" - muze obsahovat carku pro vice transitions
        if let Some(short) = styles.get("transition") {
            for entry in split_top_level_commas_str(short) {
                if let Some(spec) = Self::parse_one(&entry) {
                    out.push(spec);
                }
            }
            if !out.is_empty() { return out; }
        }

        // Longhand: transition-property/-duration/-timing-function/-delay
        let props = styles.get("transition-property").map(|s| s.trim().to_string());
        let durations = styles.get("transition-duration").map(|s| s.trim().to_string());
        let timings = styles.get("transition-timing-function").map(|s| s.trim().to_string());
        let delays = styles.get("transition-delay").map(|s| s.trim().to_string());

        if let Some(p) = props {
            let p_list: Vec<&str> = p.split(',').map(|s| s.trim()).collect();
            let d_list: Vec<&str> = durations.as_deref().unwrap_or("0s").split(',').map(|s| s.trim()).collect();
            let t_list: Vec<&str> = timings.as_deref().unwrap_or("ease").split(',').map(|s| s.trim()).collect();
            let dl_list: Vec<&str> = delays.as_deref().unwrap_or("0s").split(',').map(|s| s.trim()).collect();

            for (i, prop) in p_list.iter().enumerate() {
                let dur = d_list.get(i % d_list.len()).copied().unwrap_or("0s");
                let timing = t_list.get(i % t_list.len()).copied().unwrap_or("ease");
                let delay = dl_list.get(i % dl_list.len()).copied().unwrap_or("0s");
                out.push(TransitionSpec {
                    property: prop.to_string(),
                    duration_secs: parse_time(dur).unwrap_or(0.0),
                    timing_function: timing.to_string(),
                    delay_secs: parse_time(delay).unwrap_or(0.0),
                });
            }
        }
        out
    }

    fn parse_one(entry: &str) -> Option<TransitionSpec> {
        let mut property: Option<String> = None;
        let mut duration: f32 = 0.0;
        let mut timing: String = "ease".into();
        let mut delay: f32 = 0.0;
        let mut times_seen = 0;

        for tok in tokenize_balanced(entry) {
            let tok = tok.as_str();
            if let Some(t) = parse_time(tok) {
                if times_seen == 0 { duration = t; } else { delay = t; }
                times_seen += 1;
            } else if matches!(tok, "linear" | "ease" | "ease-in" | "ease-out" | "ease-in-out" | "step-start" | "step-end")
                || tok.starts_with("cubic-bezier(") || tok.starts_with("steps(")
            {
                timing = tok.to_string();
            } else {
                if property.is_none() { property = Some(tok.to_string()); }
            }
        }
        let property = property.unwrap_or_else(|| "all".to_string());
        if duration <= 0.0 { return None; }
        Some(TransitionSpec { property, duration_secs: duration, timing_function: timing, delay_secs: delay })
    }
}

/// Split na top-level carce (string varianta, navrat Vec<String>).
fn split_top_level_commas_str(s: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    for ch in s.chars() {
        match ch {
            '(' => { depth += 1; cur.push(ch); }
            ')' => { depth -= 1; cur.push(ch); }
            ',' if depth == 0 => {
                if !cur.trim().is_empty() { tokens.push(std::mem::take(&mut cur).trim().to_string()); }
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() { tokens.push(cur.trim().to_string()); }
    tokens
}

/// Tokenize string respektujici vyvazene zavorky (pro cubic-bezier/steps).
fn tokenize_balanced(s: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    for ch in s.chars() {
        match ch {
            '(' => { depth += 1; cur.push(ch); }
            ')' => { depth -= 1; cur.push(ch); }
            c if c.is_whitespace() && depth == 0 => {
                if !cur.is_empty() { tokens.push(std::mem::take(&mut cur)); }
            }
            _ => cur.push(ch),
        }
    }
    if !cur.is_empty() { tokens.push(cur); }
    tokens
}

/// Parsuje "2s" / "500ms" / "0.3s". Vrati sekundy.
fn parse_time(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(num) = s.strip_suffix("ms") {
        num.parse::<f32>().ok().map(|n| n / 1000.0)
    } else if let Some(num) = s.strip_suffix('s') {
        num.parse::<f32>().ok()
    } else {
        None
    }
}

/// Aplikuje easing na linearni progress (0..1).
fn apply_easing(t: f32, easing: &str) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let easing = easing.trim();
    match easing {
        "linear" => return t,
        "ease"        => return cubic_bezier(t, 0.25, 0.1, 0.25, 1.0),
        "ease-in"     => return cubic_bezier(t, 0.42, 0.0, 1.0, 1.0),
        "ease-out"    => return cubic_bezier(t, 0.0, 0.0, 0.58, 1.0),
        "ease-in-out" => return cubic_bezier(t, 0.42, 0.0, 0.58, 1.0),
        "step-start"  => return 1.0,
        "step-end"    => return if t >= 1.0 { 1.0 } else { 0.0 },
        _ => {}
    }
    // cubic-bezier(x1, y1, x2, y2)
    if let Some(args) = easing.strip_prefix("cubic-bezier(").and_then(|s| s.strip_suffix(')')) {
        let nums: Vec<f32> = args.split(',').filter_map(|p| p.trim().parse().ok()).collect();
        if nums.len() == 4 {
            return cubic_bezier(t, nums[0], nums[1], nums[2], nums[3]);
        }
    }
    // steps(n, jump-start|jump-end|jump-both|jump-none|start|end)
    if let Some(args) = easing.strip_prefix("steps(").and_then(|s| s.strip_suffix(')')) {
        let parts: Vec<&str> = args.split(',').map(|s| s.trim()).collect();
        let n: i32 = parts.first().and_then(|p| p.parse().ok()).unwrap_or(1).max(1);
        let kind = parts.get(1).copied().unwrap_or("end");
        return apply_steps(t, n, kind);
    }
    t
}

/// CSS steps() - kvantizuje progress na n diskretnich kroku.
/// kind: "jump-start"/"start", "jump-end"/"end" (default), "jump-both", "jump-none"
fn apply_steps(t: f32, n: i32, kind: &str) -> f32 {
    let n = n as f32;
    match kind {
        "jump-start" | "start" => ((t * n).floor() + 1.0) / n,
        "jump-both"            => ((t * n).floor() + 1.0) / (n + 1.0),
        "jump-none" => {
            if n <= 1.0 { return 0.0; }
            (t * n).floor() / (n - 1.0)
        }
        _ /* jump-end / end */ => (t * n).floor() / n,
    }.clamp(0.0, 1.0)
}

/// Newton-iterace pro cubic-bezier easing kompletne na sjednoceni s CSS spec.
fn cubic_bezier(t: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    // Najdi parametr u takovy ze bezier_x(u) = t, vrat bezier_y(u).
    let mut u = t;
    for _ in 0..8 {
        let x = bezier(u, x1, x2);
        let dx = bezier_deriv(u, x1, x2);
        if dx.abs() < 1e-6 { break; }
        let diff = x - t;
        if diff.abs() < 1e-4 { break; }
        u -= diff / dx;
    }
    bezier(u.clamp(0.0, 1.0), y1, y2)
}

fn bezier(u: f32, p1: f32, p2: f32) -> f32 {
    let iu = 1.0 - u;
    3.0 * iu * iu * u * p1 + 3.0 * iu * u * u * p2 + u * u * u
}

fn bezier_deriv(u: f32, p1: f32, p2: f32) -> f32 {
    let iu = 1.0 - u;
    3.0 * iu * iu * p1 + 6.0 * iu * u * (p2 - p1) + 3.0 * u * u * (1.0 - p2)
}

/// Aktivni transition - po-spu sleduje stav per element + property.
#[derive(Debug, Clone)]
pub struct ActiveTransition {
    pub node_id: usize,
    pub property: String,
    pub from_value: String,
    pub to_value: String,
    pub spec: TransitionSpec,
    /// Cas v sekundach kdy transition zacala.
    pub start_time: f32,
}

/// Detekuje zmeny stylu mezi prev_map a current_map a vyrobi nove ActiveTransitions
/// pro elementy s `transition` property co maji match. Vsechny aktualne probihajici
/// transitions po dokonceni zmizi.
///
/// Vraci aktualizovany seznam transitions po teto frame iteraci.
pub fn detect_transitions(
    prev_map: &StyleMap,
    current_map: &StyleMap,
    active: Vec<ActiveTransition>,
    elapsed_secs: f32,
) -> Vec<ActiveTransition> {
    let mut result: Vec<ActiveTransition> = Vec::new();

    // Zachovaj aktivni transitions ktere jeste nedohrali
    for at in active {
        let total = at.spec.duration_secs + at.spec.delay_secs;
        if elapsed_secs - at.start_time < total {
            result.push(at);
        }
    }

    // Pro kazdy element v current detect zmeny vs prev
    for (node_id, cur) in current_map {
        let prev = match prev_map.get(node_id) { Some(p) => p, None => continue };
        let specs = TransitionSpec::from_styles(cur);
        if specs.is_empty() { continue; }

        for spec in &specs {
            // Match: bud "all" nebo konkretni property
            let props_to_check: Vec<&String> = if spec.property == "all" {
                cur.keys().collect()
            } else {
                if cur.contains_key(&spec.property) { vec![&spec.property] } else { vec![] }
            };

            for prop in props_to_check {
                let cur_val = cur.get(prop).map(|s| s.as_str()).unwrap_or("");
                let prev_val = prev.get(prop).map(|s| s.as_str()).unwrap_or("");
                if cur_val != prev_val && !prev_val.is_empty() {
                    // Skip pokud uz transition na tu prop existuje
                    if result.iter().any(|t| t.node_id == *node_id && t.property == *prop) { continue; }
                    result.push(ActiveTransition {
                        node_id: *node_id,
                        property: prop.clone(),
                        from_value: prev_val.to_string(),
                        to_value: cur_val.to_string(),
                        spec: spec.clone(),
                        start_time: elapsed_secs,
                    });
                }
            }
        }
    }
    result
}

/// Aplikuje aktivni transitions na current style map - interpoluje hodnoty.
pub fn apply_transitions(
    style_map: &mut StyleMap,
    active: &[ActiveTransition],
    elapsed_secs: f32,
) {
    use super::layout::interpolate_keyframes as _;
    for at in active {
        let t = elapsed_secs - at.start_time - at.spec.delay_secs;
        if t < 0.0 { continue; }
        let raw_progress = (t / at.spec.duration_secs).clamp(0.0, 1.0);
        let progress = apply_easing(raw_progress, &at.spec.timing_function);

        // Interpoluj hodnotu - pres parse_length jako f32
        let from = super::layout::parse_length(&at.from_value);
        let to = super::layout::parse_length(&at.to_value);
        let interpolated = if from != 0.0 || to != 0.0 {
            // Numericka prop: interpoluj
            let v = from + (to - from) * progress;
            // Zachovaj jednotku z to_value (heuristika)
            let unit = ["px", "em", "rem", "%", "vw", "vh", "deg", "rad"]
                .iter()
                .find(|u| at.to_value.ends_with(*u))
                .copied()
                .unwrap_or("px");
            format!("{v}{unit}")
        } else {
            // Non-numericka - krokove (snap)
            if progress < 0.5 { at.from_value.clone() } else { at.to_value.clone() }
        };

        if let Some(styles) = style_map.get_mut(&at.node_id) {
            styles.insert(at.property.clone(), interpolated);
        }
    }
}

/// Aplikuje runtime CSS animace na StyleMap pri zadanem elapsed time (sekundy).
/// Pro kazdy element s `animation` / `animation-name`:
///   1. Najdi @keyframes by name v stylesheets.
///   2. Vypocti progress dle duration / iter-count / direction / delay / easing.
///   3. Interpoluj keyframes a override do ComputedStyle.
///
/// Vrati true pokud nejaka animace probiha (= caller by mel re-redrawit).
pub fn apply_animations(
    style_map: &mut StyleMap,
    stylesheets: &[Stylesheet],
    elapsed_secs: f32,
) -> bool {
    use super::layout::interpolate_keyframes;
    let mut any_active = false;

    for styles in style_map.values_mut() {
        let spec = match AnimationSpec::from_styles(styles) {
            Some(s) => s, None => continue,
        };

        // Najdi keyframes
        let frames = stylesheets.iter()
            .flat_map(|s| s.keyframes.iter())
            .find(|k| k.name == spec.name);
        let frames = match frames { Some(k) => &k.frames, None => continue };

        // Cas po zaciatku animace (bez delay)
        let t = elapsed_secs - spec.delay_secs;

        // Pred zacatkem (delay zatim probiha)
        if t < 0.0 {
            // animation-fill-mode: backwards / both -> aplikuj prvni snimek pred zacatkem
            if spec.fill_mode == "backwards" || spec.fill_mode == "both" {
                let initial = match spec.direction.as_str() {
                    "reverse" | "alternate-reverse" => 1.0,
                    _ => 0.0,
                };
                let interp_vals = interpolate_keyframes(frames, initial);
                for (k, v) in interp_vals { styles.insert(k, v); }
                any_active = true;
            }
            continue;
        }

        // Paused: pouzij fixed progress 0 (nebo posledni - zatim 0 pro jednoduchost)
        if spec.play_state == "paused" {
            // Pouzij prvni snimek
            let interp_vals = interpolate_keyframes(frames, 0.0);
            for (k, v) in interp_vals { styles.insert(k, v); }
            continue;
        }

        // Iter count check - dokonceni
        let total_progress = t / spec.duration_secs;
        if total_progress >= spec.iteration_count {
            // Animace dokoncena
            // animation-fill-mode: forwards / both -> drz posledni snimek
            // jinak (none / backwards) -> nepouzivat keyframes (vrati se na puvodni styl)
            if spec.fill_mode == "forwards" || spec.fill_mode == "both" {
                let final_progress = match spec.direction.as_str() {
                    "reverse" => 0.0,
                    "alternate" if (spec.iteration_count as i32) % 2 == 0 => 0.0,
                    "alternate-reverse" if (spec.iteration_count as i32) % 2 == 0 => 1.0,
                    _ => 1.0,
                };
                let interp_vals = interpolate_keyframes(frames, final_progress);
                for (k, v) in interp_vals { styles.insert(k, v); }
            }
            continue;
        }

        // Aktivni iteration
        let iter_idx = total_progress.floor() as i32;
        let mut local = total_progress.fract(); // 0..1 v ramci aktualni iterace

        // Direction handling
        let reverse = match spec.direction.as_str() {
            "reverse" => true,
            "alternate" => iter_idx % 2 == 1,
            "alternate-reverse" => iter_idx % 2 == 0,
            _ => false,
        };
        if reverse { local = 1.0 - local; }

        // Easing
        let progress = apply_easing(local, &spec.timing_function);

        let interp_vals = interpolate_keyframes(frames, progress);
        for (k, v) in interp_vals { styles.insert(k, v); }
        any_active = true;
    }

    any_active
}
