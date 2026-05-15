/// Free utility funkce interpreteru - bez self.
///
/// Skupiny:
/// - is_internal_key, is_in_proto_chain - properties checks
/// - native - shortcut pro vytvoreni JsValue::Function(JsFunc::Native)
/// - build_class_chain - instanceof podpora
/// - json_* - JSON serializace/deserializace
/// - now_ms, make_date_object, get_date_ms, ms_to_parts, days_to_date, is_leap - Date
/// - make_settled_promise, get_promise_state, unwrap_promise_result - Promise
/// - collect_iterable_values - sbira hodnoty z Array/Set/Map/Str
/// - radix_string, format_number_locale - Number formatovani
/// - bigdecimal_pow - BigNumber umocneni
/// - js_regex_to_rust, make_regex_object, get_regex_parts, regex_* - RegExp
/// - make_array_iterator - iterator factory

use std::cell::RefCell;
use std::rc::Rc;
use bigdecimal::{BigDecimal, One};
use regex::Regex;
use super::{JsValue, JsFunc, JsObject};

// ─── Properties checks ───────────────────────────────────────────────────

/// Vrati true kdyz klic je interni (`__key__` format - napr. `__class_chain__`).
pub fn is_internal_key(k: &str) -> bool {
    k.len() >= 4 && k.starts_with("__") && k.ends_with("__")
}

/// Zkontroluje jestli `proto` je v prototypovem retezci `target`.
pub fn is_in_proto_chain(proto: &Rc<RefCell<JsObject>>, target: &JsValue) -> bool {
    let mut current = match target {
        JsValue::Object(o) => o.borrow().proto.clone(),
        _ => return false,
    };
    let mut depth = 0;
    while let Some(p) = current {
        if depth > 100 { break; }
        if Rc::ptr_eq(&p, proto) { return true; }
        current = p.borrow().proto.clone();
        depth += 1;
    }
    false
}

// ─── Native function shortcut ────────────────────────────────────────────

/// Shortcut pro vytvoreni JsValue::Function(JsFunc::Native(...)).
pub fn native(name: &str, f: impl Fn(Vec<JsValue>) -> Result<JsValue, String> + 'static) -> JsValue {
    JsValue::Function(JsFunc::Native(name.to_string(), Rc::new(f)))
}

// ─── DOMRect factory ─────────────────────────────────────────────────────

/// Postavi JS object odpovidajici DOMRect / DOMRectReadOnly (per Geometry L1
/// spec): x, y, width, height, top, right, bottom, left, toJSON().
/// toJSON() vraci {x, y, width, height, top, right, bottom, left} jako plain
/// object - umoznuje JSON.stringify(rect) emit useful payload.
pub fn make_dom_rect(x: f32, y: f32, w: f32, h: f32) -> JsValue {
    let mut rect = JsObject::new();
    rect.set("x".into(),      JsValue::Number(x as f64));
    rect.set("y".into(),      JsValue::Number(y as f64));
    rect.set("width".into(),  JsValue::Number(w as f64));
    rect.set("height".into(), JsValue::Number(h as f64));
    rect.set("top".into(),    JsValue::Number(y as f64));
    rect.set("left".into(),   JsValue::Number(x as f64));
    rect.set("right".into(),  JsValue::Number((x + w) as f64));
    rect.set("bottom".into(), JsValue::Number((y + h) as f64));
    // toJSON - DOMRectReadOnly per spec: vracit plain object s vsemi 8 fieldy.
    let xf = x as f64;
    let yf = y as f64;
    let wf = w as f64;
    let hf = h as f64;
    rect.set("toJSON".into(), native("DOMRect.toJSON", move |_| {
        let mut j = JsObject::new();
        j.set("x".into(),      JsValue::Number(xf));
        j.set("y".into(),      JsValue::Number(yf));
        j.set("width".into(),  JsValue::Number(wf));
        j.set("height".into(), JsValue::Number(hf));
        j.set("top".into(),    JsValue::Number(yf));
        j.set("left".into(),   JsValue::Number(xf));
        j.set("right".into(),  JsValue::Number(xf + wf));
        j.set("bottom".into(), JsValue::Number(yf + hf));
        Ok(JsValue::Object(Rc::new(RefCell::new(j))))
    }));
    JsValue::Object(Rc::new(RefCell::new(rect)))
}

// ─── Iterator factory ────────────────────────────────────────────────────

/// Vytvori iterator objekt (s `next()` a `Symbol.iterator`) z pole hodnot.
pub fn make_array_iterator(values: Vec<JsValue>) -> JsValue {
    let values = Rc::new(values);
    let index  = Rc::new(RefCell::new(0usize));

    let v1 = Rc::clone(&values);
    let i1 = Rc::clone(&index);
    let next_fn = native("(iterator).next", move |_| {
        let i = *i1.borrow();
        if i < v1.len() {
            *i1.borrow_mut() = i + 1;
            let mut r = JsObject::new();
            r.set("value".into(), v1[i].clone());
            r.set("done".into(),  JsValue::Bool(false));
            Ok(JsValue::Object(Rc::new(RefCell::new(r))))
        } else {
            let mut r = JsObject::new();
            r.set("value".into(), JsValue::Undefined);
            r.set("done".into(),  JsValue::Bool(true));
            Ok(JsValue::Object(Rc::new(RefCell::new(r))))
        }
    });

    let values2 = Rc::clone(&values);
    let _index2 = Rc::new(RefCell::new(0usize));
    let self_iter = native("(iterator)[Symbol.iterator]", move |_| {
        Ok(make_array_iterator(values2.as_ref().clone()))
    });

    let mut obj = JsObject::new();
    obj.set("next".into(), next_fn);
    obj.set("Symbol.iterator".into(), self_iter);
    JsValue::Object(Rc::new(RefCell::new(obj)))
}

// ─── instanceof support ──────────────────────────────────────────────────

/// Vybuduje retezec jmen trid pro `instanceof` kontrolu.
pub fn build_class_chain(class_name: &str, super_val: Option<&JsValue>) -> String {
    let mut chain = class_name.to_string();
    let mut current = super_val;
    while let Some(JsValue::Function(JsFunc::Class { name, super_val: sv, .. })) = current {
        if let Some(n) = name {
            if !n.is_empty() {
                chain.push(',');
                chain.push_str(n);
            }
        }
        current = sv.as_deref();
    }
    chain
}

// ─── Iterable values ─────────────────────────────────────────────────────

/// Sbira hodnoty z iterable (Array/Set/Map/Str) bez self.
pub fn collect_iterable_values(val: &JsValue) -> Vec<JsValue> {
    match val {
        JsValue::Array(a) => a.borrow().clone(),
        JsValue::Set(s)   => s.borrow().values.clone(),
        JsValue::Map(m)   => m.borrow().entries.iter()
            .map(|(k,_)| k.clone()).collect(),
        JsValue::Str(s)   => s.chars().map(|c| JsValue::Str(c.to_string())).collect(),
        _ => Vec::new(),
    }
}

// ─── JSON ────────────────────────────────────────────────────────────────

/// Serializuje JsValue do JSON retezce. Vyhazuje chybu pri cyklicke referenci.
pub fn json_stringify(val: &JsValue, indent: usize, depth: usize) -> Option<String> {
    let mut seen = std::collections::HashSet::new();
    json_stringify_inner(val, indent, depth, &mut seen).ok().flatten()
}

/// Jako json_stringify, ale vraci Err pri detekci cyklicke reference (pro JSON.stringify).
pub fn json_stringify_checked(val: &JsValue, indent: usize, depth: usize) -> Result<Option<String>, String> {
    let mut seen = std::collections::HashSet::new();
    json_stringify_inner(val, indent, depth, &mut seen)
}

fn json_stringify_inner(val: &JsValue, indent: usize, depth: usize, seen: &mut std::collections::HashSet<usize>) -> Result<Option<String>, String> {
    match val {
        JsValue::Null             => Ok(Some("null".into())),
        JsValue::Bool(b)          => Ok(Some(b.to_string())),
        JsValue::Number(n) if n.is_nan() || n.is_infinite() => Ok(Some("null".into())),
        JsValue::Number(n) => {
            if *n == n.trunc() && n.abs() < 1e15 { Ok(Some(format!("{}", *n as i64))) }
            else { Ok(Some(format!("{n}"))) }
        }
        JsValue::Str(s) => Ok(Some(json_escape_str(s))),
        JsValue::Array(a) => {
            let ptr = Rc::as_ptr(a) as usize;
            if !seen.insert(ptr) {
                return Err("TypeError: Converting circular structure to JSON".into());
            }
            let mut items: Vec<String> = Vec::new();
            for v in a.borrow().iter() {
                items.push(json_stringify_inner(v, indent, depth + 1, seen)?.unwrap_or_else(|| "null".into()));
            }
            seen.remove(&ptr);
            if indent == 0 || items.is_empty() {
                Ok(Some(format!("[{}]", items.join(","))))
            } else {
                let pad = " ".repeat(indent * (depth + 1));
                let close_pad = " ".repeat(indent * depth);
                Ok(Some(format!("[\n{}{}\n{}]", pad, items.join(&format!(",\n{pad}")), close_pad)))
            }
        }
        JsValue::Object(o) => {
            let ptr = Rc::as_ptr(o) as usize;
            if !seen.insert(ptr) {
                return Err("TypeError: Converting circular structure to JSON".into());
            }
            let mut pairs: Vec<String> = Vec::new();
            let keys: Vec<String> = {
                let borrowed = o.borrow();
                let mut ks: Vec<String> = borrowed.props.keys()
                    .filter(|k| !is_internal_key(k)).cloned().collect();
                ks.sort();
                ks
            };
            for k in &keys {
                let v = o.borrow().props.get(k).cloned().unwrap_or(JsValue::Undefined);
                if let Some(serialized) = json_stringify_inner(&v, indent, depth + 1, seen)? {
                    pairs.push(format!("{}:{}", json_escape_str(k), serialized));
                }
            }
            seen.remove(&ptr);
            if indent == 0 || pairs.is_empty() {
                Ok(Some(format!("{{{}}}", pairs.join(","))))
            } else {
                let pad = " ".repeat(indent * (depth + 1));
                let close_pad = " ".repeat(indent * depth);
                Ok(Some(format!("{{\n{}{}\n{}}}", pad, pairs.join(&format!(",\n{pad}")), close_pad)))
            }
        }
        _ => Ok(None),
    }
}

pub fn json_escape_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"'  => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => { out.push_str(&format!("\\u{:04x}", c as u32)); }
            c    => out.push(c),
        }
    }
    out.push('"');
    out
}

pub fn json_parse(s: &str) -> Result<JsValue, String> {
    let chars: Vec<char> = s.chars().collect();
    let (val, _) = json_parse_value(&chars, 0)?;
    Ok(val)
}

fn json_skip_ws(chars: &[char], mut i: usize) -> usize {
    while i < chars.len() && matches!(chars[i], ' ' | '\t' | '\n' | '\r') { i += 1; }
    i
}

fn json_parse_value(chars: &[char], pos: usize) -> Result<(JsValue, usize), String> {
    let i = json_skip_ws(chars, pos);
    if i >= chars.len() { return Err("Neocekavany konec JSON".into()); }
    match chars[i] {
        '"' => {
            let (s, end) = json_parse_string(chars, i)?;
            Ok((JsValue::Str(s), end))
        }
        '[' => json_parse_array(chars, i),
        '{' => json_parse_object(chars, i),
        't' => {
            if chars.get(i..i+4) == Some(&['t','r','u','e']) { Ok((JsValue::Bool(true), i+4)) }
            else { Err(format!("Neplatny JSON token na pozici {i}")) }
        }
        'f' => {
            if chars.get(i..i+5) == Some(&['f','a','l','s','e']) { Ok((JsValue::Bool(false), i+5)) }
            else { Err(format!("Neplatny JSON token na pozici {i}")) }
        }
        'n' => {
            if chars.get(i..i+4) == Some(&['n','u','l','l']) { Ok((JsValue::Null, i+4)) }
            else { Err(format!("Neplatny JSON token na pozici {i}")) }
        }
        '-' | '0'..='9' => json_parse_number(chars, i),
        c => Err(format!("Neocekavany znak '{c}' na pozici {i}")),
    }
}

fn json_parse_string(chars: &[char], start: usize) -> Result<(String, usize), String> {
    let mut s = String::new();
    let mut i = start + 1;
    while i < chars.len() {
        match chars[i] {
            '"' => return Ok((s, i + 1)),
            '\\' => {
                i += 1;
                if i >= chars.len() { break; }
                match chars[i] {
                    '"'  => s.push('"'),
                    '\\' => s.push('\\'),
                    '/'  => s.push('/'),
                    'n'  => s.push('\n'),
                    'r'  => s.push('\r'),
                    't'  => s.push('\t'),
                    'b'  => s.push('\x08'),
                    'f'  => s.push('\x0C'),
                    'u' if i + 4 < chars.len() => {
                        let hex: String = chars[i+1..=i+4].iter().collect();
                        if let Ok(n) = u32::from_str_radix(&hex, 16) {
                            if let Some(c) = char::from_u32(n) { s.push(c); }
                        }
                        i += 4;
                    }
                    c => s.push(c),
                }
                i += 1;
            }
            c => { s.push(c); i += 1; }
        }
    }
    Err("Neuzavreny JSON retezec".into())
}

fn json_parse_number(chars: &[char], start: usize) -> Result<(JsValue, usize), String> {
    let mut i = start;
    if i < chars.len() && chars[i] == '-' { i += 1; }
    while i < chars.len() && chars[i].is_ascii_digit() { i += 1; }
    if i < chars.len() && chars[i] == '.' {
        i += 1;
        while i < chars.len() && chars[i].is_ascii_digit() { i += 1; }
    }
    if i < chars.len() && matches!(chars[i], 'e' | 'E') {
        i += 1;
        if i < chars.len() && matches!(chars[i], '+' | '-') { i += 1; }
        while i < chars.len() && chars[i].is_ascii_digit() { i += 1; }
    }
    let num_str: String = chars[start..i].iter().collect();
    let n: f64 = num_str.parse().map_err(|_| format!("Neplatne cislo: {num_str}"))?;
    Ok((JsValue::Number(n), i))
}

fn json_parse_array(chars: &[char], start: usize) -> Result<(JsValue, usize), String> {
    let mut items = Vec::new();
    let mut i = json_skip_ws(chars, start + 1);
    if i < chars.len() && chars[i] == ']' { return Ok((JsValue::Array(Rc::new(RefCell::new(items))), i + 1)); }
    loop {
        let (val, end) = json_parse_value(chars, i)?;
        items.push(val);
        i = json_skip_ws(chars, end);
        match chars.get(i) {
            Some(',') => i += 1,
            Some(']') => return Ok((JsValue::Array(Rc::new(RefCell::new(items))), i + 1)),
            _         => return Err(format!("Ocekavano ',' nebo ']' na pozici {i}")),
        }
    }
}

fn json_parse_object(chars: &[char], start: usize) -> Result<(JsValue, usize), String> {
    let mut obj = JsObject::new();
    let mut i = json_skip_ws(chars, start + 1);
    if i < chars.len() && chars[i] == '}' { return Ok((JsValue::Object(Rc::new(RefCell::new(obj))), i + 1)); }
    loop {
        i = json_skip_ws(chars, i);
        if chars.get(i) != Some(&'"') { return Err(format!("Ocekavan klic na pozici {i}")); }
        let (key, end) = json_parse_string(chars, i)?;
        i = json_skip_ws(chars, end);
        if chars.get(i) != Some(&':') { return Err(format!("Ocekavano ':' na pozici {i}")); }
        i += 1;
        let (val, end2) = json_parse_value(chars, i)?;
        obj.set(key, val);
        i = json_skip_ws(chars, end2);
        match chars.get(i) {
            Some(',') => i += 1,
            Some('}') => return Ok((JsValue::Object(Rc::new(RefCell::new(obj))), i + 1)),
            _         => return Err(format!("Ocekavano ',' nebo '}}' na pozici {i}")),
        }
    }
}

// ─── Date ────────────────────────────────────────────────────────────────

pub fn now_ms() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0)
}

/// Parse ISO 8601 / RFC 2822 / common date string formats.
/// Returns NaN pri neparsovatelne hodnote.
pub fn parse_date_string(s: &str) -> f64 {
    let s = s.trim();
    // ISO 8601: YYYY-MM-DD[THH:MM:SS[.sss][Z|±HH:MM]]
    // YYYY-MM-DD
    if s.len() == 10 && s.chars().nth(4) == Some('-') && s.chars().nth(7) == Some('-') {
        let y: i64 = s[0..4].parse().unwrap_or(0);
        let mo: u32 = s[5..7].parse::<u32>().unwrap_or(1).saturating_sub(1);
        let d: u32 = s[8..10].parse().unwrap_or(1);
        return parts_to_ms(y, mo, d, 0, 0, 0, 0);
    }
    // YYYY-MM-DDTHH:MM[:SS[.sss]][Z]
    if s.len() >= 16 && s.chars().nth(4) == Some('-') && s.chars().nth(10) == Some('T') {
        let y: i64 = s[0..4].parse().unwrap_or(0);
        let mo: u32 = s[5..7].parse::<u32>().unwrap_or(1).saturating_sub(1);
        let d: u32 = s[8..10].parse().unwrap_or(1);
        let h: u32 = s[11..13].parse().unwrap_or(0);
        let mi: u32 = s[14..16].parse().unwrap_or(0);
        let mut sec = 0u32;
        let mut ms_p = 0u32;
        if s.len() >= 19 && s.as_bytes()[16] == b':' {
            sec = s[17..19].parse().unwrap_or(0);
        }
        // Najit '.' pro ms
        if let Some(dot) = s.find('.') {
            let after = &s[dot+1..];
            let end = after.find(|c: char| !c.is_ascii_digit()).unwrap_or(after.len());
            let ms_str = &after[..end.min(3)];
            ms_p = ms_str.parse().unwrap_or(0);
            // Pad zprava: "5" -> 500ms
            ms_p *= 10u32.pow((3 - ms_str.len() as u32).min(3));
        }
        return parts_to_ms(y, mo, d, h, mi, sec, ms_p);
    }
    // YYYY (rok jako single)
    if s.len() == 4 {
        if let Ok(y) = s.parse::<i64>() {
            return parts_to_ms(y, 0, 1, 0, 0, 0, 0);
        }
    }
    f64::NAN
}

pub fn make_date_object(ms: f64) -> JsValue {
    let mut obj = JsObject::new();
    obj.set("__date_ms__".into(), JsValue::Number(ms));
    JsValue::Object(Rc::new(RefCell::new(obj)))
}

pub fn get_date_ms(val: &JsValue) -> Option<f64> {
    if let JsValue::Object(o) = val {
        if let JsValue::Number(ms) = o.borrow().props.get("__date_ms__")? {
            return Some(*ms);
        }
    }
    None
}

pub fn ms_to_parts(ms: f64) -> (i64, u32, u32, u32, u32, u32, u32) {
    let total_secs = (ms / 1000.0) as i64;
    let ms_part = (ms as i64 % 1000).unsigned_abs() as u32;
    let sec = (total_secs % 60).unsigned_abs() as u32;
    let total_min = total_secs / 60;
    let min = (total_min % 60).unsigned_abs() as u32;
    let total_hour = total_min / 60;
    let hour = (total_hour % 24).unsigned_abs() as u32;
    let total_days = total_hour / 24;
    let (year, month, day) = days_to_date(total_days);
    (year, month, day, hour, min, sec, ms_part)
}

pub fn days_to_date(mut days: i64) -> (i64, u32, u32) {
    let mut year = 1970i64;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year { break; }
        days -= days_in_year;
        year += 1;
    }
    let months = [31u32, if is_leap(year) { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 0u32;
    for &m in &months {
        if days < m as i64 { break; }
        days -= m as i64;
        month += 1;
    }
    (year, month, (days + 1) as u32)
}

pub fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub fn parts_to_ms(yr: i64, mo: u32, day: u32, hr: u32, min: u32, sec: u32, ms_part: u32) -> f64 {
    let mut days: i64 = 0;
    if yr >= 1970 {
        for y in 1970..yr { days += if is_leap(y) { 366 } else { 365 }; }
    } else {
        for y in yr..1970 { days -= if is_leap(y) { 366 } else { 365 }; }
    }
    let month_days = [31u32, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for m in 0..mo {
        let md = if m == 1 && is_leap(yr) { 29 } else { month_days[m as usize] };
        days += md as i64;
    }
    days += (day as i64) - 1;
    days as f64 * 86_400_000.0
        + hr as f64 * 3_600_000.0
        + min as f64 * 60_000.0
        + sec as f64 * 1_000.0
        + ms_part as f64
}

// ─── Promise ─────────────────────────────────────────────────────────────

/// Vyrobi iterator object z pred-vypocitanych hodnot - s next() / Symbol.iterator
/// + __iterator_helpers__ markerem pro Iterator.prototype.* helpers.
pub fn make_iterator_from_values(values: Vec<JsValue>) -> JsValue {
    let values_rc: Rc<RefCell<Vec<JsValue>>> = Rc::new(RefCell::new(values));
    let index: Rc<RefCell<usize>> = Rc::new(RefCell::new(0));
    let mut iter_obj = JsObject::new();
    iter_obj.set("__iterator_helpers__".into(), JsValue::Bool(true));
    let v1 = Rc::clone(&values_rc);
    let i1 = Rc::clone(&index);
    iter_obj.set("next".into(), native("next", move |_| {
        let i = *i1.borrow();
        let vals = v1.borrow();
        let mut r = JsObject::new();
        if i < vals.len() {
            *i1.borrow_mut() = i + 1;
            r.set("value".into(), vals[i].clone());
            r.set("done".into(), JsValue::Bool(false));
        } else {
            r.set("value".into(), JsValue::Undefined);
            r.set("done".into(), JsValue::Bool(true));
        }
        Ok(JsValue::Object(Rc::new(RefCell::new(r))))
    }));
    JsValue::Object(Rc::new(RefCell::new(iter_obj)))
}

pub fn make_settled_promise(state: &str, value: JsValue) -> JsValue {
    let mut obj = JsObject::new();
    obj.set("__promise_state__".into(), JsValue::Str(state.into()));
    obj.set("__promise_value__".into(), value);
    JsValue::Object(Rc::new(RefCell::new(obj)))
}

pub fn get_promise_state(val: &JsValue) -> Option<(String, JsValue)> {
    if let JsValue::Object(o) = val {
        let b = o.borrow();
        if let Some(JsValue::Str(state)) = b.props.get("__promise_state__") {
            let value = b.props.get("__promise_value__").cloned().unwrap_or(JsValue::Undefined);
            return Some((state.clone(), value));
        }
    }
    None
}

pub fn unwrap_promise_result(val: JsValue) -> Result<JsValue, JsValue> {
    match get_promise_state(&val) {
        Some((state, v)) if state == "fulfilled" => Ok(v),
        Some((state, v)) if state == "rejected"  => Err(v),
        Some(_) => Ok(val),
        None => Ok(val),
    }
}

// ─── Number formatovani ──────────────────────────────────────────────────

pub fn radix_string(mut n: u64, radix: u32) -> String {
    if n == 0 { return "0".into(); }
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut buf = Vec::new();
    while n > 0 {
        buf.push(DIGITS[(n % radix as u64) as usize] as char);
        n /= radix as u64;
    }
    buf.iter().rev().collect()
}

// ─── Web APIs pomucky (Base64, URL, UUID) ─────────────────────────────────

/// Base64 encode (RFC 4648).
/// SHA-256 hash. Vraci 32 bytes. Self-contained Rust impl (FIPS 180-4).
pub fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];
    // Padding
    let bit_len = (data.len() as u64) * 8;
    let mut padded = data.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 { padded.push(0); }
    padded.extend_from_slice(&bit_len.to_be_bytes());
    // Process 512-bit blocks
    for block in padded.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([block[i*4], block[i*4+1], block[i*4+2], block[i*4+3]]);
        }
        for i in 16..64 {
            let s0 = w[i-15].rotate_right(7) ^ w[i-15].rotate_right(18) ^ (w[i-15] >> 3);
            let s1 = w[i-2].rotate_right(17) ^ w[i-2].rotate_right(19) ^ (w[i-2] >> 10);
            w[i] = w[i-16].wrapping_add(s0).wrapping_add(w[i-7]).wrapping_add(s1);
        }
        let mut a = h[0]; let mut b = h[1]; let mut c = h[2]; let mut d = h[3];
        let mut e = h[4]; let mut f = h[5]; let mut g = h[6]; let mut hh = h[7];
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g; g = f; f = e; e = d.wrapping_add(t1);
            d = c; c = b; b = a; a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a); h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c); h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e); h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g); h[7] = h[7].wrapping_add(hh);
    }
    let mut out = [0u8; 32];
    for (i, w) in h.iter().enumerate() {
        out[i*4..i*4+4].copy_from_slice(&w.to_be_bytes());
    }
    out
}

/// SHA-1 hash. Vraci 20 bytes. (Insecure, ale potreba pro legacy compat).
pub fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let bit_len = (data.len() as u64) * 8;
    let mut padded = data.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 { padded.push(0); }
    padded.extend_from_slice(&bit_len.to_be_bytes());
    for block in padded.chunks_exact(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([block[i*4], block[i*4+1], block[i*4+2], block[i*4+3]]);
        }
        for i in 16..80 {
            w[i] = (w[i-3] ^ w[i-8] ^ w[i-14] ^ w[i-16]).rotate_left(1);
        }
        let mut a = h[0]; let mut b = h[1]; let mut c = h[2]; let mut d = h[3]; let mut e = h[4];
        for i in 0..80 {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A827999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let temp = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(w[i]);
            e = d; d = c; c = b.rotate_left(30); b = a; a = temp;
        }
        h[0] = h[0].wrapping_add(a); h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c); h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    let mut out = [0u8; 20];
    for (i, w) in h.iter().enumerate() {
        out[i*4..i*4+4].copy_from_slice(&w.to_be_bytes());
    }
    out
}

pub fn base64_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        out.push(ALPHABET[(b0 >> 2) as usize] as char);
        out.push(ALPHABET[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(((b1 & 0x0F) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(b2 & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Base64 decode.
pub fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    fn decode_char(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let cleaned: Vec<u8> = input.bytes().filter(|&c| !c.is_ascii_whitespace() && c != b'=').collect();
    let mut out = Vec::with_capacity(cleaned.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits = 0;
    for c in cleaned {
        let v = decode_char(c).ok_or_else(|| format!("InvalidCharacterError: '{}'", c as char))?;
        buf = (buf << 6) | (v as u32);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Ok(out)
}

/// Parsovany URL.
pub struct ParsedUrl {
    pub protocol: String,
    pub hostname: String,
    pub port: String,
    pub pathname: String,
    pub search: String,
    pub hash: String,
    pub host: String,
    pub origin: String,
}

pub fn parse_url(url: &str) -> ParsedUrl {
    let mut s = url.to_string();
    let mut hash = String::new();
    if let Some(i) = s.find('#') {
        hash = s[i..].to_string();
        s = s[..i].to_string();
    }
    let mut search = String::new();
    if let Some(i) = s.find('?') {
        search = s[i..].to_string();
        s = s[..i].to_string();
    }
    let (protocol, rest) = if let Some(i) = s.find("://") {
        (format!("{}:", &s[..i]), s[i+3..].to_string())
    } else {
        ("".into(), s)
    };
    let (host_full, pathname) = if let Some(i) = rest.find('/') {
        (rest[..i].to_string(), rest[i..].to_string())
    } else {
        (rest, "/".to_string())
    };
    let (hostname, port) = if let Some(i) = host_full.find(':') {
        (host_full[..i].to_string(), host_full[i+1..].to_string())
    } else {
        (host_full.clone(), String::new())
    };
    let origin = if !protocol.is_empty() {
        format!("{protocol}//{host_full}")
    } else {
        String::new()
    };
    ParsedUrl {
        protocol, hostname, port, pathname, search, hash,
        host: host_full, origin,
    }
}

/// Parse query string ("?a=1&b=2" nebo "a=1&b=2") na Vec<(key, value)>.
pub fn parse_query_string(s: &str) -> Vec<(String, String)> {
    let s = s.trim_start_matches('?');
    if s.is_empty() { return Vec::new(); }
    s.split('&').filter_map(|pair| {
        if let Some(eq) = pair.find('=') {
            Some((url_decode(&pair[..eq]), url_decode(&pair[eq+1..])))
        } else if !pair.is_empty() {
            Some((url_decode(pair), String::new()))
        } else {
            None
        }
    }).collect()
}

pub fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex: String = bytes[i+1..=i+2].iter().map(|&b| b as char).collect();
            if let Ok(n) = u8::from_str_radix(&hex, 16) {
                out.push(n);
                i += 3;
                continue;
            }
        } else if bytes[i] == b'+' {
            out.push(b' ');
            i += 1;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Generuje UUID v4 (random).
/// Format: XXXXXXXX-XXXX-4XXX-YXXX-XXXXXXXXXXXX kde Y = 8/9/A/B
pub fn generate_uuid_v4() -> String {
    let mut bytes = [0u8; 16];
    for byte in bytes.iter_mut() {
        *byte = (random_u32() & 0xFF) as u8;
    }
    bytes[6] = (bytes[6] & 0x0F) | 0x40; // version 4
    bytes[8] = (bytes[8] & 0x3F) | 0x80; // variant 10
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5], bytes[6], bytes[7],
        bytes[8], bytes[9], bytes[10], bytes[11],
        bytes[12], bytes[13], bytes[14], bytes[15],
    )
}

// ─── Persistent storage (localStorage FS backend) ────────────────────────

/// Vraci cestu k souboru pro persistent localStorage.
/// Default: ~/.rust-web-engine/local-storage.json
/// Override: env var RUST_WEB_ENGINE_STORAGE_PATH
pub fn storage_file_path(name: &str) -> std::path::PathBuf {
    let env_name = name.to_uppercase().replace('-', "_");
    if let Ok(p) = std::env::var(format!("RUST_WEB_ENGINE_{env_name}_PATH")) {
        return std::path::PathBuf::from(p);
    }
    let base = if let Ok(home) = std::env::var("USERPROFILE") {
        std::path::PathBuf::from(home)
    } else if let Ok(home) = std::env::var("HOME") {
        std::path::PathBuf::from(home)
    } else {
        std::env::temp_dir()
    };
    base.join(".rust-web-engine").join(format!("{name}.json"))
}

/// Nacte storage z disku jako Vec<(key, value)>.
pub fn load_storage_from_disk(name: &str) -> Vec<(String, String)> {
    let path = storage_file_path(name);
    let content = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    // Format: kazdy radek "key=value" (encoded)
    content.lines().filter_map(|line| {
        let mut split = line.splitn(2, '\t');
        let k = split.next()?;
        let v = split.next()?;
        Some((url_decode(k), url_decode(v)))
    }).collect()
}

/// Uloz storage na disk. Format: tab-separated, URL-encoded.
pub fn save_storage_to_disk(name: &str, entries: &[(String, String)]) -> std::io::Result<()> {
    let path = storage_file_path(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content: String = entries.iter()
        .map(|(k, v)| format!("{}\t{}", url_encode(k), url_encode(v)))
        .collect::<Vec<_>>().join("\n");
    std::fs::write(&path, content)
}

/// URL-encode: pro storage klice/hodnoty (escape \t a \n).
pub fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.bytes() {
        match ch {
            b'\t' => out.push_str("%09"),
            b'\n' => out.push_str("%0A"),
            b'\r' => out.push_str("%0D"),
            b'%'  => out.push_str("%25"),
            c     => out.push(c as char),
        }
    }
    out
}

/// Real HTTP request pres ureq (blocking).
/// Vraci (status, status_text, body, headers).
pub fn perform_http_request(
    url: &str,
    method: &str,
    headers: &[(String, String)],
    body: Option<&str>,
) -> Result<(u16, String, String, Vec<(String, String)>), String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(30))
        .build();

    let mut req = match method {
        "GET"     => agent.get(url),
        "POST"    => agent.post(url),
        "PUT"     => agent.put(url),
        "DELETE"  => agent.delete(url),
        "PATCH"   => agent.request("PATCH", url),
        "HEAD"    => agent.head(url),
        "OPTIONS" => agent.request("OPTIONS", url),
        m         => agent.request(m, url),
    };

    for (k, v) in headers {
        req = req.set(k, v);
    }

    let resp_result = match body {
        Some(b) => req.send_string(b),
        None    => req.call(),
    };

    match resp_result {
        Ok(resp) => {
            let status = resp.status();
            let status_text = resp.status_text().to_string();
            let header_names: Vec<String> = resp.headers_names();
            let mut resp_headers: Vec<(String, String)> = Vec::new();
            for h in &header_names {
                if let Some(v) = resp.header(h) {
                    resp_headers.push((h.clone(), v.to_string()));
                }
            }
            let resp_body = resp.into_string().unwrap_or_default();
            Ok((status, status_text, resp_body, resp_headers))
        }
        Err(ureq::Error::Status(code, resp)) => {
            // HTTP error response (4xx/5xx) - vratime jako successful s daným status
            let status_text = resp.status_text().to_string();
            let header_names: Vec<String> = resp.headers_names();
            let mut resp_headers: Vec<(String, String)> = Vec::new();
            for h in &header_names {
                if let Some(v) = resp.header(h) {
                    resp_headers.push((h.clone(), v.to_string()));
                }
            }
            let resp_body = resp.into_string().unwrap_or_default();
            Ok((code, status_text, resp_body, resp_headers))
        }
        Err(e) => Err(format!("{e}")),
    }
}

/// Pseudo-random u32 (LCG, deterministicky pro testy).
pub fn random_u32() -> u32 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static S: AtomicU64 = AtomicU64::new(0xDEADBEEF12345678);
    let s = S.fetch_add(6364136223846793005, Ordering::Relaxed);
    (s >> 32) as u32
}

// ─── Intl - real implementace pres ICU4X ──────────────────────────────────

/// Formatuje cislo podle locale pres ICU4X FixedDecimalFormatter.
/// Fallback na manualni implementaci kdyz locale nelze parsovat.
pub fn format_number_intl(n: f64, locale: &str) -> String {
    use icu::locid::Locale;
    use icu::decimal::FixedDecimalFormatter;
    use fixed_decimal::FixedDecimal;
    use std::str::FromStr;

    if n.is_nan()      { return "NaN".into(); }
    if n.is_infinite() { return if n > 0.0 { "∞".into() } else { "-∞".into() }; }

    // Parse locale
    let loc = match Locale::from_str(locale) {
        Ok(l) => l,
        Err(_) => return format_number_locale_fallback(n, locale),
    };

    // Konverze f64 -> FixedDecimal pres string (zachovava presnost)
    let s = format!("{n}");
    let fd = match FixedDecimal::from_str(&s) {
        Ok(d) => d,
        Err(_) => return format_number_locale_fallback(n, locale),
    };

    let formatter = match FixedDecimalFormatter::try_new(
        &loc.into(),
        Default::default(),
    ) {
        Ok(f) => f,
        Err(_) => return format_number_locale_fallback(n, locale),
    };

    formatter.format(&fd).to_string()
}

/// Manualni fallback formatter pokud ICU selze.
fn format_number_locale_fallback(n: f64, locale: &str) -> String {
    let (thousand_sep, decimal_sep) = match locale {
        s if s.starts_with("cs") || s.starts_with("sk") || s.starts_with("fr")
            || s.starts_with("pl") || s.starts_with("ru") => (' ', ','),
        s if s.starts_with("de") || s.starts_with("it") || s.starts_with("es") => ('.', ','),
        _ => (',', '.'),
    };
    let s = format!("{n}");
    let (int_part, dec_part) = if let Some(dot) = s.find('.') {
        (&s[..dot], Some(&s[dot+1..]))
    } else { (s.as_str(), None) };
    let (neg, digits) = if int_part.starts_with('-') {
        (true, &int_part[1..])
    } else { (false, int_part) };
    let with_sep: String = digits.chars().rev().enumerate()
        .flat_map(|(i, c)| {
            if i > 0 && i % 3 == 0 { vec![thousand_sep, c] } else { vec![c] }
        })
        .collect::<String>()
        .chars().rev().collect();
    let result = match dec_part {
        Some(d) => format!("{with_sep}{decimal_sep}{d}"),
        None    => with_sep,
    };
    if neg { format!("-{result}") } else { result }
}

/// Formatuje date/time podle locale pres ICU4X DateTimeFormatter.
/// Pri selhani fallback na manualni format.
pub fn format_datetime_intl(ms: f64, locale: &str) -> String {
    use icu::locid::Locale;
    use icu::datetime::{DateTimeFormatter, options::length};
    use icu::calendar::DateTime;
    use std::str::FromStr;

    let loc = match Locale::from_str(locale) {
        Ok(l) => l,
        Err(_) => return format_datetime_fallback(ms, locale),
    };

    // Konverze ms -> DateTime
    let (yr, mo, day, hr, min, sec, _) = ms_to_parts(ms);
    let dt = match DateTime::try_new_iso_datetime(
        yr as i32,
        (mo + 1) as u8,
        day as u8,
        hr as u8,
        min as u8,
        sec as u8,
    ) {
        Ok(d) => d.to_any(),
        Err(_) => return format_datetime_fallback(ms, locale),
    };

    let options = length::Bag::from_date_time_style(
        length::Date::Medium,
        length::Time::Medium,
    );

    let formatter = match DateTimeFormatter::try_new(&loc.into(), options.into()) {
        Ok(f) => f,
        Err(_) => return format_datetime_fallback(ms, locale),
    };

    formatter.format_to_string(&dt).unwrap_or_else(|_| format_datetime_fallback(ms, locale))
}

fn format_datetime_fallback(ms: f64, locale: &str) -> String {
    let (yr, mo, day, hr, min, sec, _) = ms_to_parts(ms);
    match locale {
        s if s.starts_with("cs") => format!("{day}. {}. {yr} {hr:02}:{min:02}:{sec:02}", mo+1),
        s if s.starts_with("de") => format!("{day}.{}.{yr} {hr:02}:{min:02}:{sec:02}", mo+1),
        _ => {
            let pm = hr >= 12;
            let h12 = if hr == 0 { 12 } else if hr > 12 { hr - 12 } else { hr };
            format!("{}/{day}/{yr} {h12}:{min:02}:{sec:02} {}", mo+1, if pm { "PM" } else { "AM" })
        }
    }
}

/// Plural kategorie pres ICU4X PluralRules.
/// Pokryva CLDR rules pro vsechny world locales.
pub fn plural_select(n: f64, locale: &str) -> String {
    use icu::locid::Locale;
    use icu::plurals::{PluralRules, PluralRuleType};
    use fixed_decimal::FixedDecimal;
    use std::str::FromStr;

    let loc = match Locale::from_str(locale) {
        Ok(l) => l,
        Err(_) => return plural_select_fallback(n, locale),
    };

    let rules = match PluralRules::try_new(&loc.into(), PluralRuleType::Cardinal) {
        Ok(r) => r,
        Err(_) => return plural_select_fallback(n, locale),
    };

    let s = format!("{n}");
    let fd = match FixedDecimal::from_str(&s) {
        Ok(d) => d,
        Err(_) => return plural_select_fallback(n, locale),
    };

    use icu::plurals::PluralCategory::*;
    match rules.category_for(&fd) {
        Zero  => "zero".into(),
        One   => "one".into(),
        Two   => "two".into(),
        Few   => "few".into(),
        Many  => "many".into(),
        Other => "other".into(),
    }
}

fn plural_select_fallback(n: f64, _locale: &str) -> String {
    if n.abs() == 1.0 { "one".into() } else { "other".into() }
}

/// Collator compare pres ICU4X.
pub fn collator_compare_intl(a: &str, b: &str, locale: &str) -> i32 {
    use icu::locid::Locale;
    use icu::collator::Collator;
    use std::str::FromStr;

    let loc = Locale::from_str(locale).unwrap_or_default();
    let collator = match Collator::try_new(&loc.into(), Default::default()) {
        Ok(c) => c,
        Err(_) => return a.cmp(b) as i32,
    };
    match collator.compare(a, b) {
        std::cmp::Ordering::Less    => -1,
        std::cmp::Ordering::Greater =>  1,
        std::cmp::Ordering::Equal   =>  0,
    }
}

pub fn format_number_locale(n: f64) -> String {
    if n.is_nan()      { return "NaN".into(); }
    if n.is_infinite() { return if n > 0.0 { "Infinity".into() } else { "-Infinity".into() }; }
    let s = format!("{n}");
    let (integer_part, decimal_part) = if let Some(dot) = s.find('.') {
        (&s[..dot], Some(&s[dot+1..]))
    } else {
        (s.as_str(), None)
    };
    let (neg, digits) = if integer_part.starts_with('-') {
        (true, &integer_part[1..])
    } else {
        (false, integer_part)
    };
    let with_sep: String = digits.chars().rev().enumerate()
        .flat_map(|(i, c)| {
            if i > 0 && i % 3 == 0 { vec![',', c] } else { vec![c] }
        })
        .collect::<String>()
        .chars().rev().collect();
    let result = match decimal_part {
        Some(d) => format!("{with_sep}.{d}"),
        None    => with_sep,
    };
    if neg { format!("-{result}") } else { result }
}

// ─── BigNumber ───────────────────────────────────────────────────────────

pub fn bigdecimal_pow(base: BigDecimal, exp: u64) -> BigDecimal {
    if exp == 0 { return BigDecimal::one(); }
    let mut result = BigDecimal::one();
    let mut b = base;
    let mut e = exp;
    while e > 0 {
        if e & 1 == 1 { result = result * b.clone(); }
        b = b.clone() * b.clone();
        e >>= 1;
    }
    result
}

// ─── RegExp ──────────────────────────────────────────────────────────────

pub fn js_regex_to_rust(pattern: &str, flags: &str) -> Result<Regex, String> {
    // ES2024 /v flag (Unicode sets) - akceptujeme stejne jako /u
    // /d flag (hasIndices) - ignorujeme (ne support v Rust regex)
    let ignore_case = flags.contains('i');
    let multiline = flags.contains('m');
    let dot_all = flags.contains('s');
    let prefix = format!(
        "(?{}{}{})",
        if ignore_case { "i" } else { "" },
        if multiline  { "m" } else { "" },
        if dot_all    { "s" } else { "" },
    );
    let full = if prefix == "(?)" {
        pattern.to_string()
    } else {
        format!("{prefix}{pattern}")
    };
    Regex::new(&full).map_err(|e| format!("SyntaxError: Neplatny regex /{pattern}/{flags}: {e}"))
}

/// Detekuje jestli pattern vyzaduje fancy-regex (lookbehind, backreference, atd.).
fn needs_fancy_regex(pattern: &str) -> bool {
    // Lookbehind: (?<= nebo (?<!
    if pattern.contains("(?<=") || pattern.contains("(?<!") { return true; }
    // Backreferences: \1 .. \9 (ne v char tridach)
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            if (b'1'..=b'9').contains(&bytes[i + 1]) {
                return true;
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    false
}

/// Sjednocene rozhrani: bud std regex (rychly) nebo fancy-regex (vice features).
pub enum JsRegex {
    Std(Regex),
    Fancy(fancy_regex::Regex),
}

impl JsRegex {
    pub fn compile(pattern: &str, flags: &str) -> Result<Self, String> {
        let ignore_case = flags.contains('i');
        let multiline = flags.contains('m');
        let dot_all = flags.contains('s');
        let prefix = format!(
            "(?{}{}{})",
            if ignore_case { "i" } else { "" },
            if multiline  { "m" } else { "" },
            if dot_all    { "s" } else { "" },
        );
        let full = if prefix == "(?)" {
            pattern.to_string()
        } else {
            format!("{prefix}{pattern}")
        };
        if needs_fancy_regex(pattern) {
            fancy_regex::Regex::new(&full)
                .map(JsRegex::Fancy)
                .map_err(|e| format!("SyntaxError: Neplatny regex /{pattern}/{flags}: {e}"))
        } else {
            // Try std regex first (faster)
            match Regex::new(&full) {
                Ok(re) => Ok(JsRegex::Std(re)),
                Err(_) => fancy_regex::Regex::new(&full)
                    .map(JsRegex::Fancy)
                    .map_err(|e| format!("SyntaxError: Neplatny regex /{pattern}/{flags}: {e}")),
            }
        }
    }

    pub fn is_match(&self, text: &str) -> bool {
        match self {
            JsRegex::Std(r)   => r.is_match(text),
            JsRegex::Fancy(r) => r.is_match(text).unwrap_or(false),
        }
    }

    /// Najde prvni shodu, vrati positional groups.
    pub fn captures(&self, text: &str) -> Option<Vec<Option<String>>> {
        match self {
            JsRegex::Std(r) => {
                let caps = r.captures(text)?;
                let mut out = Vec::new();
                for i in 0..caps.len() {
                    out.push(caps.get(i).map(|m| m.as_str().to_string()));
                }
                Some(out)
            }
            JsRegex::Fancy(r) => {
                let caps = r.captures(text).ok()??;
                let mut out = Vec::new();
                for i in 0..caps.len() {
                    out.push(caps.get(i).map(|m| m.as_str().to_string()));
                }
                Some(out)
            }
        }
    }

    pub fn find_all(&self, text: &str) -> Vec<String> {
        match self {
            JsRegex::Std(r)   => r.find_iter(text).map(|m| m.as_str().to_string()).collect(),
            JsRegex::Fancy(r) => r.find_iter(text).filter_map(|res| res.ok())
                .map(|m| m.as_str().to_string()).collect(),
        }
    }
}

/// regex_exec s podporou named groups - vraci (positional, named).
/// Pouziva JsRegex (std nebo fancy podle patternu).
pub fn regex_exec_named(pattern: &str, flags: &str, text: &str)
    -> Option<(Vec<Option<String>>, Vec<(String, Option<String>)>)>
{
    let re = JsRegex::compile(pattern, flags).ok()?;
    match re {
        JsRegex::Std(r) => {
            let caps = r.captures(text)?;
            let mut positional = Vec::new();
            for i in 0..caps.len() {
                positional.push(caps.get(i).map(|m| m.as_str().to_string()));
            }
            let mut named = Vec::new();
            for name in r.capture_names().flatten() {
                named.push((name.to_string(), caps.name(name).map(|m| m.as_str().to_string())));
            }
            Some((positional, named))
        }
        JsRegex::Fancy(r) => {
            let caps = r.captures(text).ok()??;
            let mut positional = Vec::new();
            for i in 0..caps.len() {
                positional.push(caps.get(i).map(|m| m.as_str().to_string()));
            }
            let mut named = Vec::new();
            for name in r.capture_names().flatten() {
                named.push((name.to_string(), caps.name(name).map(|m| m.as_str().to_string())));
            }
            Some((positional, named))
        }
    }
}

pub fn make_regex_object(pattern: &str, flags: &str) -> JsValue {
    let mut obj = JsObject::new();
    obj.set("__regex_pattern__".into(), JsValue::Str(pattern.to_string()));
    obj.set("__regex_flags__".into(),   JsValue::Str(flags.to_string()));
    obj.set("source".into(),            JsValue::Str(pattern.to_string()));
    obj.set("flags".into(),             JsValue::Str(flags.to_string()));
    obj.set("global".into(),            JsValue::Bool(flags.contains('g')));
    obj.set("ignoreCase".into(),        JsValue::Bool(flags.contains('i')));
    obj.set("multiline".into(),         JsValue::Bool(flags.contains('m')));
    obj.set("lastIndex".into(),         JsValue::Number(0.0));
    JsValue::Object(Rc::new(RefCell::new(obj)))
}

pub fn get_regex_parts(val: &JsValue) -> Option<(String, String)> {
    if let JsValue::Object(o) = val {
        let b = o.borrow();
        let pat = b.props.get("__regex_pattern__")?.clone();
        let flags = b.props.get("__regex_flags__")?.clone();
        if let (JsValue::Str(p), JsValue::Str(f)) = (pat, flags) {
            return Some((p, f));
        }
    }
    None
}

pub fn regex_test(pattern: &str, flags: &str, text: &str) -> bool {
    JsRegex::compile(pattern, flags)
        .map(|re| re.is_match(text))
        .unwrap_or(false)
}

pub fn regex_exec(pattern: &str, flags: &str, text: &str) -> Option<Vec<Option<String>>> {
    let re = JsRegex::compile(pattern, flags).ok()?;
    re.captures(text)
}

pub fn regex_match_all(pattern: &str, flags: &str, text: &str) -> Vec<String> {
    match JsRegex::compile(pattern, flags) {
        Ok(re) => re.find_all(text),
        Err(_) => vec![],
    }
}
