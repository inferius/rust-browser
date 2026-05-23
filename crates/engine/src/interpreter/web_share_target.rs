//! Web Share Target API - PWA receives shared content from OS share sheet.
//!
//! Spec: https://w3c.github.io/web-share-target/
//! Manifest "share_target": {action, method, params: {title, text, url, files: [...]}}.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShareMethod {
    Get,
    Post,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShareEncoding {
    Urlencoded,
    Multipart,
}

#[derive(Debug, Clone, Default)]
pub struct ShareParams {
    pub title: String,            // form param name
    pub text: String,
    pub url: String,
    pub files: Vec<ShareFileSpec>,
}

#[derive(Debug, Clone)]
pub struct ShareFileSpec {
    pub name: String,
    pub accept: Vec<String>,      // MIME types, e.g. ["image/*", "video/mp4"]
}

#[derive(Debug, Clone)]
pub struct ShareTargetConfig {
    pub action: String,
    pub method: ShareMethod,
    pub encoding: ShareEncoding,
    pub params: ShareParams,
}

impl ShareTargetConfig {
    pub fn new(action: &str) -> Self {
        Self {
            action: action.into(),
            method: ShareMethod::Get,
            encoding: ShareEncoding::Urlencoded,
            params: ShareParams::default(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct IncomingShare {
    pub title: Option<String>,
    pub text: Option<String>,
    pub url: Option<String>,
    pub files: Vec<(String, Vec<u8>)>,
}

/// Build navigate target dle config + incoming share data.
/// GET: appended jako query params. POST: vraci body + content-type.
pub struct DispatchedShare {
    pub url: String,
    pub method: ShareMethod,
    pub body: Vec<u8>,
    pub content_type: String,
    pub form_fields: HashMap<String, String>,
}

pub fn dispatch(config: &ShareTargetConfig, data: &IncomingShare) -> DispatchedShare {
    let mut fields = HashMap::new();
    if let Some(t) = &data.title { fields.insert(config.params.title.clone(), t.clone()); }
    if let Some(t) = &data.text { fields.insert(config.params.text.clone(), t.clone()); }
    if let Some(u) = &data.url { fields.insert(config.params.url.clone(), u.clone()); }
    match config.method {
        ShareMethod::Get => {
            let mut url = config.action.clone();
            if !fields.is_empty() {
                let sep = if url.contains('?') { '&' } else { '?' };
                url.push(sep);
                let qs: Vec<String> = fields.iter().map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v))).collect();
                url.push_str(&qs.join("&"));
            }
            DispatchedShare {
                url,
                method: ShareMethod::Get,
                body: Vec::new(),
                content_type: String::new(),
                form_fields: fields,
            }
        }
        ShareMethod::Post => {
            let (body, ct) = match config.encoding {
                ShareEncoding::Urlencoded => {
                    let qs: Vec<String> = fields.iter().map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v))).collect();
                    (qs.join("&").into_bytes(), "application/x-www-form-urlencoded".to_string())
                }
                ShareEncoding::Multipart => {
                    let boundary = "----WebKitFormBoundary7MA4YWxkTrZu0gW";
                    let mut body = Vec::new();
                    for (k, v) in &fields {
                        body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
                        body.extend_from_slice(format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n{}\r\n", k, v).as_bytes());
                    }
                    body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());
                    (body, format!("multipart/form-data; boundary={}", boundary))
                }
            };
            DispatchedShare {
                url: config.action.clone(),
                method: ShareMethod::Post,
                body,
                content_type: ct,
                form_fields: fields,
            }
        }
    }
}

fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> ShareTargetConfig {
        let mut c = ShareTargetConfig::new("/share");
        c.params.title = "t".into();
        c.params.text = "x".into();
        c.params.url = "u".into();
        c
    }

    #[test]
    fn get_appends_query() {
        let mut c = cfg();
        c.method = ShareMethod::Get;
        let d = dispatch(&c, &IncomingShare {
            title: Some("hi".into()),
            text: None, url: None, files: vec![],
        });
        assert!(d.url.starts_with("/share?"));
        assert!(d.url.contains("t=hi"));
    }

    #[test]
    fn post_urlencoded_body() {
        let mut c = cfg();
        c.method = ShareMethod::Post;
        let d = dispatch(&c, &IncomingShare {
            title: Some("a b".into()),
            text: Some("hi".into()),
            url: None, files: vec![],
        });
        assert_eq!(d.content_type, "application/x-www-form-urlencoded");
        let s = String::from_utf8(d.body).unwrap();
        assert!(s.contains("t=a+b"));
        assert!(s.contains("x=hi"));
    }

    #[test]
    fn post_multipart_has_boundary() {
        let mut c = cfg();
        c.method = ShareMethod::Post;
        c.encoding = ShareEncoding::Multipart;
        let d = dispatch(&c, &IncomingShare {
            title: Some("x".into()), text: None, url: None, files: vec![],
        });
        assert!(d.content_type.starts_with("multipart/form-data"));
        let s = String::from_utf8(d.body).unwrap();
        assert!(s.contains("Content-Disposition"));
    }
}
