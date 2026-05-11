//! URL fetch + resolve helpers + HTTP cache.

use std::io::Read;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::collections::HashMap;

/// Cache dir pro fetched HTTP resources (CSS / fonts / images / scripts).
/// Pres CCHE / TMP / project root .cache. Created on demand.
fn cache_dir() -> PathBuf {
    if let Ok(p) = std::env::var("RWE_CACHE_DIR") {
        return PathBuf::from(p);
    }
    // Default: project root / .cache. (CWD = project root pri cargo run.)
    let dir = PathBuf::from(".cache/rustwebengine");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Stabilni hash URL -> filename. 64-bit FxHash hex string. Bez kolizi pro
/// realne use case (typicky <10k cached resources).
fn cache_key(url: &str) -> String {
    use std::hash::{BuildHasher, Hash, Hasher};
    let s = ahash::RandomState::with_seeds(
        0x1234567890abcdef, 0xfedcba0987654321,
        0xdeadbeefcafebabe, 0x0123456789abcdef);
    let mut h = s.build_hasher();
    url.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// In-memory cache - HTTP resource bytes. Doplneny disk-cache vrstvou.
fn mem_cache() -> &'static Mutex<HashMap<String, Vec<u8>>> {
    static C: OnceLock<Mutex<HashMap<String, Vec<u8>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Cached HTTP byte fetch. Pri opt-out RWE_NO_CACHE=1 fetchne vzdy.
/// Cache-bypass per request: predef'd "?nocache" in URL (rare).
/// Returns None pri network/IO error.
pub fn cached_fetch_bytes(url: &str) -> Option<Vec<u8>> {
    // FS / data URL = no caching (fast direct path).
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return fetch_bytes_uncached(url);
    }
    let disabled = std::env::var("RWE_NO_CACHE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if disabled {
        return fetch_bytes_uncached(url);
    }
    // 1) In-memory cache.
    {
        let m = mem_cache().lock().unwrap();
        if let Some(b) = m.get(url) { return Some(b.clone()); }
    }
    // 2) Disk cache.
    let key = cache_key(url);
    let path = cache_dir().join(&key);
    if let Ok(bytes) = std::fs::read(&path) {
        mem_cache().lock().unwrap().insert(url.to_string(), bytes.clone());
        return Some(bytes);
    }
    // 3) Real fetch + write through.
    let bytes = fetch_bytes_uncached(url)?;
    let _ = std::fs::write(&path, &bytes);
    mem_cache().lock().unwrap().insert(url.to_string(), bytes.clone());
    Some(bytes)
}

/// Cached HTTP text fetch. Vraci String pres UTF-8 decode bytes.
pub fn cached_fetch_text(url: &str) -> Option<String> {
    let bytes = cached_fetch_bytes(url)?;
    // Try UTF-8 prvni (vetsina web stranek), pak lossy fallback.
    match String::from_utf8(bytes.clone()) {
        Ok(s) => Some(s),
        Err(_) => Some(String::from_utf8_lossy(&bytes).into_owned()),
    }
}

/// Pure HTTP fetch bez cache (interni - cached_fetch_bytes vola tady pri miss).
fn fetch_bytes_uncached(url: &str) -> Option<Vec<u8>> {
    if url.starts_with("http://") || url.starts_with("https://") {
        match ureq::get(url)
            .set("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 RustWebEngine/0.1")
            .set("Accept", "*/*")
            .set("Accept-Language", "cs-CZ,cs;q=0.9,en-US;q=0.8,en;q=0.7")
            .set("Accept-Encoding", "identity")
            .timeout(std::time::Duration::from_secs(15))
            .call()
        {
            Ok(resp) => {
                let mut buf = Vec::new();
                if resp.into_reader().read_to_end(&mut buf).is_ok() {
                    Some(buf)
                } else { None }
            }
            Err(e) => {
                eprintln!("[fetch] {url}: {e}");
                None
            }
        }
    } else if let Some(rest) = url.strip_prefix("file:///") {
        std::fs::read(rest.replace('/', std::path::MAIN_SEPARATOR_STR)).ok()
    } else {
        std::fs::read(url).ok()
    }
}

/// Fetch text resource (HTML/CSS/JS) z URL nebo FS path. Pres cached_fetch_text -
/// HTTP resources jsou disk + RAM cached. Pri reload stejneho hosta = OK,
/// nemusi se redownload kazdy refresh.
pub fn fetch_text_url(url: &str) -> Option<String> {
    cached_fetch_text(url)
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
/// HTTP cached pres cached_fetch_bytes. Data URI inline base64 decode.
pub fn fetch_image_bytes(src: &str) -> Option<Vec<u8>> {
    if src.starts_with("http://") || src.starts_with("https://") {
        return cached_fetch_bytes(src);
    }
    if let Some(rest) = src.strip_prefix("data:") {
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
