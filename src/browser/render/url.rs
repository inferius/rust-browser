//! URL fetch + resolve helpers.

use std::io::Read;

/// Fetch text resource (HTML/CSS) z URL nebo FS path.
/// Pro http(s):// pres ureq, jinak std::fs::read_to_string.
/// Default User-Agent identifikuje engine.
pub fn fetch_text_url(url: &str) -> Option<String> {
    if url.starts_with("http://") || url.starts_with("https://") {
        // UA emuluje moderni Chrome - nektere stranky (Google, ad serverů,
        // analytika) podavaji stripped/legacy HTML kdyz UA nepoznaji jako
        // realny browser. Nas branding je hidden v RustWebEngine note ale
        // zacatek = standardni Chrome string.
        match ureq::get(url)
            .set("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 RustWebEngine/0.1")
            .set("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8")
            .set("Accept-Language", "cs-CZ,cs;q=0.9,en-US;q=0.8,en;q=0.7")
            .set("Accept-Encoding", "identity")
            .set("Sec-Ch-Ua", "\"Google Chrome\";v=\"131\", \"Chromium\";v=\"131\", \"Not_A Brand\";v=\"24\"")
            .set("Sec-Ch-Ua-Mobile", "?0")
            .set("Sec-Ch-Ua-Platform", "\"Windows\"")
            .timeout(std::time::Duration::from_secs(15))
            .call()
        {
            Ok(resp) => resp.into_string().ok(),
            Err(e) => {
                eprintln!("[fetch] {url}: {e}");
                None
            }
        }
    } else if let Some(rest) = url.strip_prefix("file:///") {
        std::fs::read_to_string(rest.replace('/', std::path::MAIN_SEPARATOR_STR)).ok()
    } else {
        std::fs::read_to_string(url).ok()
    }
}

/// Resolve relative URL proti base URL. Vraci absolutni URL.
/// `base` muze byt http(s)://... nebo file:///....
pub fn resolve_url(base: &str, relative: &str) -> String {
    // Absolute URL/data uri - return as-is.
    if relative.starts_with("http://") || relative.starts_with("https://")
        || relative.starts_with("data:") || relative.starts_with("file:") {
        return relative.to_string();
    }
    // Protocol-relative: //example.com/path
    if let Some(stripped) = relative.strip_prefix("//") {
        let scheme = if base.starts_with("https://") { "https:" } else { "http:" };
        return format!("{scheme}//{stripped}");
    }
    // Find base scheme + host root.
    let (scheme_host, base_path) = if let Some(rest) = base.strip_prefix("https://") {
        let path_pos = rest.find('/').unwrap_or(rest.len());
        (format!("https://{}", &rest[..path_pos]), rest[path_pos..].to_string())
    } else if let Some(rest) = base.strip_prefix("http://") {
        let path_pos = rest.find('/').unwrap_or(rest.len());
        (format!("http://{}", &rest[..path_pos]), rest[path_pos..].to_string())
    } else if let Some(rest) = base.strip_prefix("file:///") {
        // file path - relative se resolvuje proti dir.
        let last_slash = rest.rfind('/').unwrap_or(0);
        let dir = &rest[..last_slash];
        if relative.starts_with('/') {
            return format!("file:///{relative}");
        }
        return format!("file:///{dir}/{relative}");
    } else {
        // Neznamy base - return relative as-is.
        return relative.to_string();
    };
    // Absolute path: /foo/bar
    if relative.starts_with('/') {
        return format!("{scheme_host}{relative}");
    }
    // Relative path: resolve proti directory v base_path.
    // Empty base_path (google.com bez path) -> base_dir = "/" (root).
    let base_dir: String = if base_path.is_empty() {
        "/".to_string()
    } else {
        let last_slash = base_path.rfind('/').unwrap_or(0);
        // Inclusive range bezpecne na non-empty stringu.
        base_path[..=last_slash.min(base_path.len() - 1)].to_string()
    };
    let mut combined = format!("{scheme_host}{base_dir}{relative}");
    // Resolve .. and . segments.
    if combined.contains("/../") || combined.contains("/./") {
        let scheme_end = combined.find("://").map(|p| p + 3).unwrap_or(0);
        let (prefix, path_part) = combined.split_at(scheme_end + combined[scheme_end..].find('/').unwrap_or(0));
        let mut segs: Vec<&str> = Vec::new();
        for s in path_part.split('/') {
            if s == ".." { segs.pop(); }
            else if s == "." || s.is_empty() { /* skip empty */ }
            else { segs.push(s); }
        }
        combined = format!("{prefix}/{}", segs.join("/"));
    }
    combined
}

/// Fetch image bytes - podporuje http(s)://, data: URI, FS path.
/// Vrati None pri chybe (timeout, neplatny format, IO error).
pub fn fetch_image_bytes(src: &str) -> Option<Vec<u8>> {
    if src.starts_with("http://") || src.starts_with("https://") {
        // HTTP fetch pres ureq sync
        match ureq::get(src).timeout(std::time::Duration::from_secs(10)).call() {
            Ok(resp) => {
                let mut buf = Vec::new();
                if resp.into_reader().read_to_end(&mut buf).is_ok() {
                    return Some(buf);
                }
                None
            }
            Err(_) => None,
        }
    } else if let Some(rest) = src.strip_prefix("data:") {
        // data:[<mime>][;base64],<payload>
        let comma = rest.find(',')?;
        let header = &rest[..comma];
        let payload = &rest[comma+1..];
        if header.contains(";base64") {
            decode_base64(payload)
        } else {
            // URL-encoded text - vratit bytes (image neni typicky raw text)
            Some(payload.as_bytes().to_vec())
        }
    } else {
        // FS path
        let path = if src.starts_with('/') {
            src.to_string()
        } else {
            format!("static/{src}")
        };
        std::fs::read(&path).ok()
    }
}

/// Decode base64 string -> bytes. Self-contained, bez external crate.
fn decode_base64(s: &str) -> Option<Vec<u8>> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let chars: Vec<char> = s.chars().collect();
    let val = |c: char| -> Option<u8> {
        match c {
            'A'..='Z' => Some(c as u8 - b'A'),
            'a'..='z' => Some(c as u8 - b'a' + 26),
            '0'..='9' => Some(c as u8 - b'0' + 52),
            '+' | '-' => Some(62),
            '/' | '_' => Some(63),
            '=' => Some(0),
            _ => None,
        }
    };
    let mut i = 0;
    while i + 3 < chars.len() {
        let a = val(chars[i])?;
        let b = val(chars[i+1])?;
        let c = val(chars[i+2])?;
        let d = val(chars[i+3])?;
        out.push((a << 2) | (b >> 4));
        if chars[i+2] != '=' { out.push(((b & 0xF) << 4) | (c >> 2)); }
        if chars[i+3] != '=' { out.push(((c & 0x3) << 6) | d); }
        i += 4;
    }
    Some(out)
}
