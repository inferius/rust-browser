/// String prototype metody (slice/substring/indexOf/replace/match/split/...).
///
/// Standalone funkce bez self - dispatchovana z Interpreter::eval_call
/// pro `JsValue::Str(s)` arm.

use std::cell::RefCell;
use std::rc::Rc;
use super::{JsValue, JsError};
use super::helpers::{get_regex_parts, js_regex_to_rust, regex_exec, regex_match_all};

pub fn call_string_method(
    s: &str,
    method: &str,
    args: Vec<JsValue>,
) -> Result<Option<JsValue>, JsError> {
    let chars: Vec<char> = s.chars().collect();
    match method {
        "slice" => {
            let len = chars.len() as i64;
            let start = args.get(0).map(|v| v.to_number() as i64).unwrap_or(0);
            let end   = args.get(1).map(|v| v.to_number() as i64).unwrap_or(len);
            let s2 = if start < 0 { (len + start).max(0) } else { start.min(len) } as usize;
            let e2 = if end   < 0 { (len + end  ).max(0) } else { end  .min(len) } as usize;
            Ok(Some(JsValue::Str(chars[s2..e2.max(s2)].iter().collect())))
        }
        "substring" => {
            let len = chars.len();
            let a = args.get(0).map(|v| (v.to_number() as usize).min(len)).unwrap_or(0);
            let b = args.get(1).map(|v| (v.to_number() as usize).min(len)).unwrap_or(len);
            let (s2, e2) = if a <= b { (a, b) } else { (b, a) };
            Ok(Some(JsValue::Str(chars[s2..e2].iter().collect())))
        }
        "indexOf" => {
            let needle = args.first().map(|v| v.to_string()).unwrap_or_default();
            Ok(Some(JsValue::Number(s.find(&*needle).map(|i| s[..i].chars().count() as f64).unwrap_or(-1.0))))
        }
        "lastIndexOf" => {
            let needle = args.first().map(|v| v.to_string()).unwrap_or_default();
            Ok(Some(JsValue::Number(s.rfind(&*needle).map(|i| s[..i].chars().count() as f64).unwrap_or(-1.0))))
        }
        "includes"    => {
            let needle = args.first().map(|v| v.to_string()).unwrap_or_default();
            Ok(Some(JsValue::Bool(s.contains(&*needle))))
        }
        "startsWith"  => {
            let needle = args.first().map(|v| v.to_string()).unwrap_or_default();
            Ok(Some(JsValue::Bool(s.starts_with(&*needle))))
        }
        "endsWith"    => {
            let needle = args.first().map(|v| v.to_string()).unwrap_or_default();
            Ok(Some(JsValue::Bool(s.ends_with(&*needle))))
        }
        "toLowerCase"  => Ok(Some(JsValue::Str(s.to_lowercase()))),
        "toUpperCase"  => Ok(Some(JsValue::Str(s.to_uppercase()))),
        "trim"         => Ok(Some(JsValue::Str(s.trim().to_string()))),
        "trimStart"    => Ok(Some(JsValue::Str(s.trim_start().to_string()))),
        "trimEnd"      => Ok(Some(JsValue::Str(s.trim_end().to_string()))),
        "charAt"       => {
            let i = args.first().map(|v| v.to_number() as usize).unwrap_or(0);
            Ok(Some(JsValue::Str(chars.get(i).map(|c| c.to_string()).unwrap_or_default())))
        }
        "charCodeAt"   => {
            let i = args.first().map(|v| v.to_number() as usize).unwrap_or(0);
            Ok(Some(JsValue::Number(chars.get(i).map(|c| *c as u32 as f64).unwrap_or(f64::NAN))))
        }
        "at"           => {
            let len = chars.len() as i64;
            let idx = args.first().map(|v| v.to_number() as i64).unwrap_or(0);
            let real = if idx < 0 { len + idx } else { idx };
            Ok(Some(chars.get(real as usize).map(|c| JsValue::Str(c.to_string())).unwrap_or(JsValue::Undefined)))
        }
        "padStart"     => {
            let target = args.first().map(|v| v.to_number() as usize).unwrap_or(0);
            let pad = args.get(1).map(|v| v.to_string()).unwrap_or_else(|| " ".into());
            if chars.len() >= target { return Ok(Some(JsValue::Str(s.to_string()))); }
            let needed = target - chars.len();
            let pad_chars: Vec<char> = pad.chars().collect();
            let padding: String = (0..needed).map(|i| pad_chars[i % pad_chars.len()]).collect();
            Ok(Some(JsValue::Str(format!("{padding}{s}"))))
        }
        "padEnd"       => {
            let target = args.first().map(|v| v.to_number() as usize).unwrap_or(0);
            let pad = args.get(1).map(|v| v.to_string()).unwrap_or_else(|| " ".into());
            if chars.len() >= target { return Ok(Some(JsValue::Str(s.to_string()))); }
            let needed = target - chars.len();
            let pad_chars: Vec<char> = pad.chars().collect();
            let padding: String = (0..needed).map(|i| pad_chars[i % pad_chars.len()]).collect();
            Ok(Some(JsValue::Str(format!("{s}{padding}"))))
        }
        "repeat"       => {
            let n = args.first().map(|v| v.to_number() as usize).unwrap_or(0);
            Ok(Some(JsValue::Str(s.repeat(n))))
        }
        "replace"      => {
            let repl = args.get(1).map(|v| v.to_string()).unwrap_or_default();
            match args.first() {
                Some(re) if get_regex_parts(re).is_some() => {
                    let (pat, flags) = get_regex_parts(re).unwrap();
                    match js_regex_to_rust(&pat, &flags) {
                        Ok(regex) => {
                            let result = regex.replacen(s, 1, repl.as_str());
                            Ok(Some(JsValue::Str(result.into_owned())))
                        }
                        Err(e) => Err(JsError::Runtime(e)),
                    }
                }
                Some(from) => {
                    Ok(Some(JsValue::Str(s.replacen(&*from.to_string(), &repl, 1))))
                }
                None => Ok(Some(JsValue::Str(s.to_string()))),
            }
        }
        "replaceAll"   => {
            let repl = args.get(1).map(|v| v.to_string()).unwrap_or_default();
            match args.first() {
                Some(re) if get_regex_parts(re).is_some() => {
                    let (pat, flags) = get_regex_parts(re).unwrap();
                    match js_regex_to_rust(&pat, &flags) {
                        Ok(regex) => {
                            let result = regex.replace_all(s, repl.as_str());
                            Ok(Some(JsValue::Str(result.into_owned())))
                        }
                        Err(e) => Err(JsError::Runtime(e)),
                    }
                }
                Some(from) => {
                    Ok(Some(JsValue::Str(s.replace(&*from.to_string(), &repl))))
                }
                None => Ok(Some(JsValue::Str(s.to_string()))),
            }
        }
        "match" => {
            match args.into_iter().next() {
                Some(re) if get_regex_parts(&re).is_some() => {
                    let (pat, flags) = get_regex_parts(&re).unwrap();
                    let global = flags.contains('g');
                    if global {
                        let matches = regex_match_all(&pat, &flags, s);
                        if matches.is_empty() {
                            Ok(Some(JsValue::Null))
                        } else {
                            let arr: Vec<JsValue> = matches.into_iter().map(JsValue::Str).collect();
                            Ok(Some(JsValue::Array(Rc::new(RefCell::new(arr)))))
                        }
                    } else {
                        match regex_exec(&pat, &flags, s) {
                            None => Ok(Some(JsValue::Null)),
                            Some(groups) => {
                                let arr: Vec<JsValue> = groups.into_iter()
                                    .map(|g| g.map(JsValue::Str).unwrap_or(JsValue::Undefined))
                                    .collect();
                                Ok(Some(JsValue::Array(Rc::new(RefCell::new(arr)))))
                            }
                        }
                    }
                }
                Some(pattern) => {
                    let p = pattern.to_string();
                    if s.contains(&*p) {
                        let arr = vec![JsValue::Str(p)];
                        Ok(Some(JsValue::Array(Rc::new(RefCell::new(arr)))))
                    } else {
                        Ok(Some(JsValue::Null))
                    }
                }
                None => Ok(Some(JsValue::Null)),
            }
        }
        "search" => {
            match args.into_iter().next() {
                Some(re) if get_regex_parts(&re).is_some() => {
                    let (pat, flags) = get_regex_parts(&re).unwrap();
                    match js_regex_to_rust(&pat, &flags) {
                        Ok(regex) => {
                            let idx = regex.find(s).map(|m| m.start() as f64).unwrap_or(-1.0);
                            Ok(Some(JsValue::Number(idx)))
                        }
                        Err(e) => Err(JsError::Runtime(e)),
                    }
                }
                Some(pattern) => {
                    let p = pattern.to_string();
                    let idx = s.find(&*p).map(|i| i as f64).unwrap_or(-1.0);
                    Ok(Some(JsValue::Number(idx)))
                }
                None => Ok(Some(JsValue::Number(-1.0))),
            }
        }
        "split" => {
            let sep = args.first().cloned();
            let limit = args.get(1).map(|v| v.to_number() as usize);
            let parts: Vec<JsValue> = match &sep {
                None => vec![JsValue::Str(s.to_string())],
                Some(re) if get_regex_parts(re).is_some() => {
                    let (pat, flags) = get_regex_parts(re).unwrap();
                    match js_regex_to_rust(&pat, &flags) {
                        Ok(regex) => {
                            let mut result: Vec<JsValue> = regex.split(s)
                                .map(|p| JsValue::Str(p.to_string()))
                                .collect();
                            if let Some(lim) = limit { result.truncate(lim); }
                            result
                        }
                        Err(_) => vec![JsValue::Str(s.to_string())],
                    }
                }
                Some(v) => {
                    let d = v.to_string();
                    let mut result: Vec<JsValue> = if d == "undefined" {
                        vec![JsValue::Str(s.to_string())]
                    } else if d.is_empty() {
                        chars.iter().map(|c| JsValue::Str(c.to_string())).collect()
                    } else {
                        s.split(&*d).map(|p| JsValue::Str(p.to_string())).collect()
                    };
                    if let Some(lim) = limit { result.truncate(lim); }
                    result
                }
            };
            Ok(Some(JsValue::Array(Rc::new(RefCell::new(parts)))))
        }
        "toString" | "valueOf" => Ok(Some(JsValue::Str(s.to_string()))),
        "concat" => {
            let mut result = s.to_string();
            for arg in args { result.push_str(&arg.to_string()); }
            Ok(Some(JsValue::Str(result)))
        }
        "substr" => {
            // substr(start[, length]) - start moze byt negativni
            let len = chars.len() as i64;
            let start = args.get(0).map(|v| v.to_number() as i64).unwrap_or(0);
            let s2 = if start < 0 { (len + start).max(0) } else { start.min(len) } as usize;
            let count = args.get(1).map(|v| v.to_number() as usize).unwrap_or(chars.len() - s2);
            let e2 = (s2 + count).min(chars.len());
            Ok(Some(JsValue::Str(chars[s2..e2].iter().collect())))
        }
        "codePointAt" => {
            let i = args.first().map(|v| v.to_number() as usize).unwrap_or(0);
            Ok(Some(match chars.get(i) {
                Some(c) => JsValue::Number(*c as u32 as f64),
                None    => JsValue::Undefined,
            }))
        }
        "normalize" => {
            // Plna normalizace vyzaduje unicode-normalization crate; vracime original (NFC approx).
            Ok(Some(JsValue::Str(s.to_string())))
        }
        "matchAll" => {
            // String.prototype.matchAll(regex) - vraci iterator nad pres vsech zacatku.
            let arg = args.into_iter().next().unwrap_or(JsValue::Undefined);
            let mut pattern = String::new();
            if let JsValue::Object(o) = &arg {
                let b = o.borrow();
                if let Some(JsValue::Str(p)) = b.props.get("__regex_pattern__") {
                    pattern = p.clone();
                }
            } else if let JsValue::Str(p) = &arg {
                pattern = p.clone();
            }
            let mut matches: Vec<JsValue> = Vec::new();
            if !pattern.is_empty() {
                if let Ok(re) = fancy_regex::Regex::new(&pattern) {
                    let mut start = 0usize;
                    while start <= s.len() {
                        match re.find_from_pos(s, start) {
                            Ok(Some(mat)) => {
                                let m_str = mat.as_str().to_string();
                                let arr = vec![JsValue::Str(m_str)];
                                matches.push(JsValue::Array(Rc::new(RefCell::new(arr))));
                                start = if mat.end() == mat.start() { mat.end() + 1 } else { mat.end() };
                            }
                            _ => break,
                        }
                    }
                }
            }
            Ok(Some(crate::interpreter::helpers::make_iterator_from_values(matches)))
        }
        "isWellFormed" => {
            // ES2024 - test zda string ma jen valid UTF-16 (zadne lone surrogates).
            // Nase Rust strings jsou UTF-8 -> vsechny well-formed.
            // Edge case: simulujeme failure pri \uD800-\uDFFF range bez par.
            let chars: Vec<char> = s.chars().collect();
            let mut well_formed = true;
            for c in chars {
                let cp = c as u32;
                if (0xD800..=0xDFFF).contains(&cp) {
                    well_formed = false;
                    break;
                }
            }
            Ok(Some(JsValue::Bool(well_formed)))
        }
        "toWellFormed" => {
            // ES2024 - replace lone surrogates s U+FFFD.
            let mut out = String::new();
            for c in s.chars() {
                let cp = c as u32;
                if (0xD800..=0xDFFF).contains(&cp) {
                    out.push('\u{FFFD}');
                } else {
                    out.push(c);
                }
            }
            Ok(Some(JsValue::Str(out)))
        }
        _ => Ok(None),
    }
}
