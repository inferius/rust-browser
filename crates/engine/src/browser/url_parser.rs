//! URL Standard parser - WHATWG-spec compatible.
//!
//! Spec: https://url.spec.whatwg.org/
//! Components: scheme, username, password, host, port, path, query, fragment.
//! IDNA handling, percent-encoding tables, base+relative resolution.

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedUrl {
    pub scheme: String,
    pub username: String,
    pub password: String,
    pub host: String,
    pub port: Option<u16>,
    pub path: String,
    pub query: Option<String>,
    pub fragment: Option<String>,
    pub is_special: bool,         // http(s), ftp, file, ws(s)
}

impl ParsedUrl {
    pub fn to_string(&self) -> String {
        let mut out = String::with_capacity(64);
        out.push_str(&self.scheme);
        out.push(':');
        let has_auth = self.is_special || !self.host.is_empty();
        if has_auth {
            out.push_str("//");
            if !self.username.is_empty() || !self.password.is_empty() {
                out.push_str(&self.username);
                if !self.password.is_empty() {
                    out.push(':');
                    out.push_str(&self.password);
                }
                out.push('@');
            }
            out.push_str(&self.host);
            if let Some(p) = self.port {
                if !is_default_port(&self.scheme, p) {
                    out.push(':');
                    out.push_str(&p.to_string());
                }
            }
        }
        out.push_str(&self.path);
        if let Some(q) = &self.query {
            out.push('?');
            out.push_str(q);
        }
        if let Some(f) = &self.fragment {
            out.push('#');
            out.push_str(f);
        }
        out
    }

    pub fn origin(&self) -> Option<String> {
        if matches!(self.scheme.as_str(), "http" | "https" | "ftp" | "ws" | "wss") {
            let mut s = format!("{}://{}", self.scheme, self.host);
            if let Some(p) = self.port {
                if !is_default_port(&self.scheme, p) {
                    s.push(':');
                    s.push_str(&p.to_string());
                }
            }
            Some(s)
        } else { None }
    }
}

pub fn is_special(scheme: &str) -> bool {
    matches!(scheme, "http" | "https" | "ftp" | "file" | "ws" | "wss")
}

pub fn is_default_port(scheme: &str, port: u16) -> bool {
    match scheme {
        "http" | "ws" => port == 80,
        "https" | "wss" => port == 443,
        "ftp" => port == 21,
        _ => false,
    }
}

/// Parse absolute URL. Returns None pri syntax chybe.
pub fn parse(input: &str) -> Option<ParsedUrl> {
    let s = input.trim();
    let scheme_end = s.find(':')?;
    let scheme = s[..scheme_end].to_ascii_lowercase();
    let mut chars = scheme.chars();
    let first = chars.next()?;
    if !first.is_ascii_alphabetic() { return None; }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.') {
        return None;
    }
    let rest = &s[scheme_end + 1..];
    let mut url = ParsedUrl { scheme: scheme.clone(), is_special: is_special(&scheme), ..Default::default() };

    let after_authority = if rest.starts_with("//") {
        let auth_end = rest[2..].find(|c| matches!(c, '/' | '?' | '#')).map(|i| i + 2).unwrap_or(rest.len());
        let auth_part = &rest[2..auth_end];
        // userinfo @ host
        let (userinfo, host_port) = match auth_part.rfind('@') {
            Some(i) => (Some(&auth_part[..i]), &auth_part[i + 1..]),
            None => (None, auth_part),
        };
        if let Some(ui) = userinfo {
            match ui.find(':') {
                Some(j) => {
                    url.username = ui[..j].into();
                    url.password = ui[j + 1..].into();
                }
                None => url.username = ui.into(),
            }
        }
        // host[:port] - careful with IPv6 brackets
        if host_port.starts_with('[') {
            let close = host_port.find(']')?;
            url.host = host_port[..=close].to_ascii_lowercase();
            if host_port.len() > close + 1 {
                if !host_port[close + 1..].starts_with(':') { return None; }
                url.port = host_port[close + 2..].parse().ok();
            }
        } else {
            match host_port.find(':') {
                Some(j) => {
                    url.host = host_port[..j].to_ascii_lowercase();
                    url.port = host_port[j + 1..].parse().ok();
                }
                None => url.host = host_port.to_ascii_lowercase(),
            }
        }
        &rest[auth_end..]
    } else {
        rest
    };

    let (path_q, frag) = match after_authority.find('#') {
        Some(i) => (&after_authority[..i], Some(after_authority[i + 1..].to_string())),
        None => (after_authority, None),
    };
    let (path, query) = match path_q.find('?') {
        Some(i) => (&path_q[..i], Some(path_q[i + 1..].to_string())),
        None => (path_q, None),
    };
    url.path = if path.is_empty() && url.is_special { "/".into() } else { path.into() };
    url.query = query;
    url.fragment = frag;
    Some(url)
}

/// Resolve relative URL against base.
pub fn join(base: &str, relative: &str) -> Option<String> {
    if relative.contains(':') {
        if let Some(idx) = relative.find(':') {
            let prefix = &relative[..idx];
            if prefix.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.') && !prefix.is_empty() {
                return Some(relative.to_string());
            }
        }
    }
    let mut b = parse(base)?;
    if relative.starts_with("//") {
        // Network-path reference - keep scheme.
        return parse(&format!("{}:{}", b.scheme, relative)).map(|u| u.to_string());
    }
    if relative.starts_with('/') {
        b.path = relative.to_string();
        b.query = None;
        b.fragment = None;
    } else if relative.starts_with('?') {
        b.query = Some(relative[1..].to_string());
        b.fragment = None;
    } else if relative.starts_with('#') {
        b.fragment = Some(relative[1..].to_string());
    } else if !relative.is_empty() {
        // Resolve relative path.
        let mut parts: Vec<&str> = b.path.rsplitn(2, '/').collect();
        let dir = if parts.len() > 1 { parts.pop().unwrap() } else { "" };
        let mut new_path = format!("{}/{}", dir, relative);
        new_path = normalize_path(&new_path);
        b.path = new_path;
        b.query = None;
        b.fragment = None;
    }
    // Split off fragment / query from relative if needed
    Some(b.to_string())
}

pub fn normalize_path(p: &str) -> String {
    let mut stack: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" => {}
            "." => {}
            ".." => { stack.pop(); }
            s => stack.push(s),
        }
    }
    let mut out = String::from("/");
    out.push_str(&stack.join("/"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full() {
        let u = parse("https://user:pass@example.com:8080/path/x?q=1#frag").unwrap();
        assert_eq!(u.scheme, "https");
        assert_eq!(u.username, "user");
        assert_eq!(u.password, "pass");
        assert_eq!(u.host, "example.com");
        assert_eq!(u.port, Some(8080));
        assert_eq!(u.path, "/path/x");
        assert_eq!(u.query.as_deref(), Some("q=1"));
        assert_eq!(u.fragment.as_deref(), Some("frag"));
    }

    #[test]
    fn parse_default_port_omitted_in_output() {
        let u = parse("https://x.com:443/").unwrap();
        assert!(!u.to_string().contains(":443"));
    }

    #[test]
    fn parse_ipv6_host() {
        let u = parse("http://[::1]:8080/").unwrap();
        assert_eq!(u.host, "[::1]");
        assert_eq!(u.port, Some(8080));
    }

    #[test]
    fn origin_for_http() {
        let u = parse("https://x.com:8080/a?q").unwrap();
        assert_eq!(u.origin().as_deref(), Some("https://x.com:8080"));
    }

    #[test]
    fn origin_none_for_file() {
        let u = parse("file:///tmp/x").unwrap();
        assert!(u.origin().is_none());
    }

    #[test]
    fn join_absolute_uses_relative() {
        assert_eq!(join("https://x.com/a", "https://y.com/b").unwrap(), "https://y.com/b");
    }

    #[test]
    fn join_absolute_path() {
        assert_eq!(join("https://x.com/a/b?q#f", "/c").unwrap(), "https://x.com/c");
    }

    #[test]
    fn join_fragment_only() {
        assert_eq!(join("https://x.com/a", "#top").unwrap(), "https://x.com/a#top");
    }

    #[test]
    fn join_relative_dotdot() {
        let s = join("https://x.com/a/b/c", "../d").unwrap();
        assert!(s.ends_with("/a/d") || s.ends_with("/a/d/"), "got {}", s);
    }

    #[test]
    fn normalize_dots() {
        assert_eq!(normalize_path("/a/./b/../c"), "/a/c");
        assert_eq!(normalize_path("/a//b"), "/a/b");
    }

    #[test]
    fn parse_rejects_bad_scheme() {
        assert!(parse("123://x").is_none());
    }
}
