//! HTML5 named character references.
//!
//! Spec: https://html.spec.whatwg.org/multipage/named-characters.html
//! Subset of the most common references; full set is ~2200 entries.

pub fn decode_named(name: &str) -> Option<&'static str> {
    Some(match name {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        "nbsp" => "\u{00A0}",
        "copy" => "\u{00A9}",
        "reg" => "\u{00AE}",
        "trade" => "\u{2122}",
        "mdash" => "\u{2014}",
        "ndash" => "\u{2013}",
        "hellip" => "\u{2026}",
        "ldquo" => "\u{201C}",
        "rdquo" => "\u{201D}",
        "lsquo" => "\u{2018}",
        "rsquo" => "\u{2019}",
        "laquo" => "\u{00AB}",
        "raquo" => "\u{00BB}",
        "bull" => "\u{2022}",
        "middot" => "\u{00B7}",
        "deg" => "\u{00B0}",
        "plusmn" => "\u{00B1}",
        "times" => "\u{00D7}",
        "divide" => "\u{00F7}",
        "le" => "\u{2264}",
        "ge" => "\u{2265}",
        "ne" => "\u{2260}",
        "infin" => "\u{221E}",
        "sum" => "\u{2211}",
        "prod" => "\u{220F}",
        "para" => "\u{00B6}",
        "sect" => "\u{00A7}",
        "euro" => "\u{20AC}",
        "pound" => "\u{00A3}",
        "yen" => "\u{00A5}",
        "cent" => "\u{00A2}",
        "alpha" => "\u{03B1}",
        "beta" => "\u{03B2}",
        "gamma" => "\u{03B3}",
        "delta" => "\u{03B4}",
        "epsilon" => "\u{03B5}",
        "pi" => "\u{03C0}",
        "sigma" => "\u{03C3}",
        "omega" => "\u{03C9}",
        "Pi" => "\u{03A0}",
        "Sigma" => "\u{03A3}",
        "Omega" => "\u{03A9}",
        "larr" => "\u{2190}",
        "uarr" => "\u{2191}",
        "rarr" => "\u{2192}",
        "darr" => "\u{2193}",
        "harr" => "\u{2194}",
        "AElig" => "\u{00C6}",
        "Auml" => "\u{00C4}",
        "Ouml" => "\u{00D6}",
        "Uuml" => "\u{00DC}",
        "auml" => "\u{00E4}",
        "ouml" => "\u{00F6}",
        "uuml" => "\u{00FC}",
        "szlig" => "\u{00DF}",
        "Acirc" => "\u{00C2}",
        "Ecirc" => "\u{00CA}",
        _ => return None,
    })
}

/// Decode `&entity;` / `&#NNN;` / `&#xHH;` in input. Loose: matches even without `;`.
pub fn decode_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&' {
            // Find ; or whitespace.
            let end = bytes[i + 1..].iter().position(|b| *b == b';' || *b == b' ' || *b == b'\n').map(|p| p + i + 1);
            if let Some(j) = end {
                let inner = &s[i + 1..j];
                if let Some(decoded) = decode_one(inner) {
                    out.push_str(decoded.as_str());
                    i = if bytes.get(j) == Some(&b';') { j + 1 } else { j };
                    continue;
                }
            }
        }
        out.push(s[i..].chars().next().unwrap_or(' '));
        i += s[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
    }
    out
}

fn decode_one(inner: &str) -> Option<String> {
    if inner.is_empty() { return None; }
    if let Some(hex_part) = inner.strip_prefix("#x").or_else(|| inner.strip_prefix("#X")) {
        let cp = u32::from_str_radix(hex_part, 16).ok()?;
        return std::char::from_u32(cp).map(|c| c.to_string());
    }
    if let Some(dec_part) = inner.strip_prefix('#') {
        let cp: u32 = dec_part.parse().ok()?;
        return std::char::from_u32(cp).map(|c| c.to_string());
    }
    decode_named(inner).map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_amp() {
        assert_eq!(decode_named("amp"), Some("&"));
    }

    #[test]
    fn decode_em_dash() {
        assert_eq!(decode_named("mdash"), Some("\u{2014}"));
    }

    #[test]
    fn decode_unknown() {
        assert_eq!(decode_named("foobar"), None);
    }

    #[test]
    fn decode_decimal_ref() {
        let r = decode_text("A&#65;B");
        assert_eq!(r, "AAB");
    }

    #[test]
    fn decode_hex_ref() {
        let r = decode_text("&#x41;");
        assert_eq!(r, "A");
    }

    #[test]
    fn decode_named_ref() {
        let r = decode_text("Tom &amp; Jerry");
        assert_eq!(r, "Tom & Jerry");
    }

    #[test]
    fn passes_through_unknown() {
        let r = decode_text("&unknown;");
        assert_eq!(r, "&unknown;");
    }
}
