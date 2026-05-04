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
        "border" => {
            // "1px solid red" - parse postupne
            let parts: Vec<&str> = value.split_whitespace().collect();
            for p in &parts {
                if p.ends_with("px") || p.ends_with("em") || p.ends_with("rem") {
                    out.insert("border-width".into(), p.to_string());
                } else if matches!(*p, "solid" | "dashed" | "dotted" | "double" | "none") {
                    out.insert("border-style".into(), p.to_string());
                } else if super::layout::parse_color(p).is_some() {
                    out.insert("border-color".into(), p.to_string());
                }
            }
            out.insert("border".into(), value.into());
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

/// Mapa: pointer na Node -> computed styles.
pub type StyleMap = HashMap<usize, HashMap<String, String>>;

/// Pomocnik: vrati pointer hodnotu Rc<Node> jako klic.
fn node_id(node: &Rc<Node>) -> usize {
    Rc::as_ptr(node) as usize
}

/// Resolvuje CSS var(--name) a calc() expressions.
/// Pri var(--x, fallback): pokud --x v variables, pouzij ho, jinak fallback.
pub fn resolve_value(value: &str, variables: &HashMap<String, String>) -> String {
    let mut out = value.to_string();
    let mut iters = 0;
    // Iterativne nahrazuj var() - max 10 urovnich nesteni
    while out.contains("var(") && iters < 10 {
        let new_out = replace_var_once(&out, variables);
        if new_out == out { break; }
        out = new_out;
        iters += 1;
    }
    // Calc() - jednoduchy parser (jen + - * /)
    if out.contains("calc(") {
        out = resolve_calc(&out);
    }
    out
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

/// Cascade s viewport pro @media queries.
pub fn cascade_with_viewport(root: &Rc<Node>, stylesheets: &[Stylesheet],
                              viewport_w: f32, viewport_h: f32) -> StyleMap {
    // Sjednotit rules + matching media query rules do jednoho seznamu
    let mut effective: Vec<Stylesheet> = Vec::new();
    for sheet in stylesheets {
        let mut combined = sheet.clone();
        // Aplikuj jen vyhovujici media queries
        for mq in &sheet.media_queries {
            if super::css_parser::evaluate_media_query(&mq.query, viewport_w, viewport_h) {
                combined.rules.extend(mq.rules.clone());
            }
        }
        combined.media_queries.clear();
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
            for rule in &sheet.rules {
                for sel in &rule.selectors {
                    if matches_selector(node, sel) {
                        let spec = specificity(sel);
                        for decl in &rule.declarations {
                            let key = (
                                if decl.important { 1 } else { 0 },
                                spec.0,
                                spec.1 + spec.2,
                                order,
                            );
                            matched_decls.push((
                                (key.0, key.1, key.2, key.3),
                                decl,
                            ));
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
                        current_node = next;
                        found = true;
                        break;
                    }
                    let next = p.parent.borrow().upgrade();
                    current_node = next;
                }
                if !found { return false; }
            }
            // Sibling combinators - zatim nepodporujeme spravne
            _ => return false,
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

    // Pseudo-classes :hover/:active/:focus zatim ignorujeme (vyzaduji runtime stav)
    // Skip - vraci match aby se pravidlo neaplikovalo zbytecne
    for pc in &sel.pseudo_classes {
        match pc.as_str() {
            "root" => {
                // :root match jen html element
                if tag != "html" { return false; }
            }
            "first-child" => {
                let parent = node.parent.borrow().upgrade();
                if let Some(p) = parent {
                    let children = p.children.borrow();
                    let first_el = children.iter().find(|c| matches!(c.kind, NodeKind::Element(_)));
                    if first_el.map(|f| !std::rc::Rc::ptr_eq(f, node)).unwrap_or(true) {
                        return false;
                    }
                }
            }
            "last-child" => {
                let parent = node.parent.borrow().upgrade();
                if let Some(p) = parent {
                    let children = p.children.borrow();
                    let last_el = children.iter().rev().find(|c| matches!(c.kind, NodeKind::Element(_)));
                    if last_el.map(|f| !std::rc::Rc::ptr_eq(f, node)).unwrap_or(true) {
                        return false;
                    }
                }
            }
            // hover/active/focus - bez runtime nemuzu - skip (pravidlo se NEaplikuje)
            "hover" | "active" | "focus" | "visited" | "link" => return false,
            _ => {}
        }
    }

    true
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
    pub timing_function: String, // "linear" / "ease" / "ease-in" / "ease-out" / "ease-in-out"
    pub iteration_count: f32,    // f32::INFINITY pro "infinite"
    pub direction: String,        // "normal" / "reverse" / "alternate" / "alternate-reverse"
    pub delay_secs: f32,
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

        // Shorthand parsing
        if let Some(short) = styles.get("animation") {
            for tok in short.split_whitespace() {
                if let Some(s) = parse_time(tok) {
                    if duration == 0.0 { duration = s; } else { delay = s; }
                } else if tok == "infinite" {
                    iter = f32::INFINITY;
                } else if let Ok(n) = tok.parse::<f32>() {
                    iter = n;
                } else if matches!(tok, "linear" | "ease" | "ease-in" | "ease-out" | "ease-in-out" | "step-start" | "step-end") {
                    timing = tok.to_string();
                } else if matches!(tok, "normal" | "reverse" | "alternate" | "alternate-reverse") {
                    direction = tok.to_string();
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

        let name = name?;
        if name == "none" || duration <= 0.0 { return None; }
        Some(AnimationSpec {
            name, duration_secs: duration, timing_function: timing,
            iteration_count: iter, direction, delay_secs: delay,
        })
    }
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
    match easing {
        "linear" => t,
        "ease"        => cubic_bezier(t, 0.25, 0.1, 0.25, 1.0),
        "ease-in"     => cubic_bezier(t, 0.42, 0.0, 1.0, 1.0),
        "ease-out"    => cubic_bezier(t, 0.0, 0.0, 0.58, 1.0),
        "ease-in-out" => cubic_bezier(t, 0.42, 0.0, 0.58, 1.0),
        "step-start"  => 1.0,
        "step-end"    => if t >= 1.0 { 1.0 } else { 0.0 },
        _ => t,
    }
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
        if t < 0.0 { continue; }

        // Iter count check
        let total_progress = t / spec.duration_secs; // pocet kompletnich + zlomek iteraci
        if total_progress >= spec.iteration_count {
            // Animace dokoncena - aplikuj posledni snimek
            let final_progress = match spec.direction.as_str() {
                "reverse" => 0.0,
                "alternate" if (spec.iteration_count as i32) % 2 == 0 => 0.0,
                "alternate-reverse" if (spec.iteration_count as i32) % 2 == 0 => 1.0,
                _ => 1.0,
            };
            let interp_vals = interpolate_keyframes(frames, final_progress);
            for (k, v) in interp_vals { styles.insert(k, v); }
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
