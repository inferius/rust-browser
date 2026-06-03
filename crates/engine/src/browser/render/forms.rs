//! Form submission helpers (action URL build, POST, url-encode).

use crate::browser::dom::{Node, NodeKind};
use std::rc::Rc;
use super::resolve_url;

/// Najde nejblizsi <form> ancestor.
pub fn find_ancestor_form(node: &Rc<Node>) -> Option<Rc<Node>> {
    let mut current = Some(std::rc::Rc::clone(node));
    while let Some(n) = current {
        if n.tag_name().as_deref() == Some("form") { return Some(n); }
        current = n.parent.borrow().upgrade();
    }
    None
}

/// Vrati (resolved_url, method, querystring_or_body).
/// Pri GET: body=None, params kombinovany do URL ?k=v&k=v.
/// Pri POST: vrati url cely + body separately. Caller posli pres ureq POST.
pub fn build_form_request(form: &Rc<Node>, base_url: Option<&str>) -> Option<(String, String, Option<String>)> {
    let action = form.attr("action").unwrap_or_default();
    let method = form.attr("method").unwrap_or_default().to_lowercase();
    let method = if method.is_empty() { "get".to_string() } else { method };
    let action_resolved = if action.is_empty() {
        base_url.unwrap_or("").to_string()
    } else if let Some(b) = base_url {
        resolve_url(b, &action)
    } else { action };
    let mut params: Vec<(String, String)> = Vec::new();
    fn collect(node: &Rc<Node>, out: &mut Vec<(String, String)>) {
        if matches!(node.kind, NodeKind::Element(_)) {
            if let Some(tag) = node.tag_name() {
                if matches!(tag.as_str(), "input" | "select" | "textarea") {
                    let name = node.attr("name").unwrap_or_default();
                    if !name.is_empty() {
                        let val = match tag.as_str() {
                            "input" => {
                                let t = node.attr("type").unwrap_or_default().to_lowercase();
                                if t == "checkbox" || t == "radio" {
                                    if node.attr("checked").is_some() {
                                        node.attr("value").unwrap_or_else(|| "on".to_string())
                                    } else { return; }
                                } else if t == "submit" || t == "button" || t == "reset" || t == "image" {
                                    return; // skip submit-type inputs from data
                                } else {
                                    node.attr("value").unwrap_or_default()
                                }
                            }
                            "select" => {
                                let mut selected: Option<String> = None;
                                let mut first: Option<String> = None;
                                for ch in node.children.borrow().iter() {
                                    if ch.tag_name().as_deref() == Some("option") {
                                        let v = ch.attr("value").unwrap_or_else(|| ch.text_content().trim().to_string());
                                        if first.is_none() { first = Some(v.clone()); }
                                        if ch.attr("selected").is_some() { selected = Some(v); break; }
                                    }
                                }
                                selected.or(first).unwrap_or_default()
                            }
                            "textarea" => node.text_content(),
                            _ => String::new(),
                        };
                        out.push((name, val));
                    }
                }
            }
        }
        for ch in node.children.borrow().iter() {
            collect(ch, out);
        }
    }
    collect(form, &mut params);
    let qs: Vec<String> = params.into_iter().map(|(k, v)|
        format!("{}={}", url_encode(&k), url_encode(&v))).collect();
    let body = qs.join("&");
    if method == "post" {
        Some((action_resolved, method, Some(body)))
    } else {
        let separator = if action_resolved.contains('?') { "&" } else { "?" };
        let url = if body.is_empty() { action_resolved }
                  else { format!("{action_resolved}{separator}{body}") };
        Some((url, method, None))
    }
}

/// POST request s url-encoded form body. Vrati response HTML.
pub fn post_form(url: &str, body: &str) -> Option<String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        eprintln!("[form POST] non-http URL: {url}");
        return None;
    }
    match ureq::post(url)
        .set("User-Agent", "Mozilla/5.0 RustWebEngine/0.1")
        .set("Content-Type", "application/x-www-form-urlencoded")
        .timeout(std::time::Duration::from_secs(15))
        .send_string(body)
    {
        Ok(resp) => resp.into_string().ok(),
        Err(e) => { eprintln!("[form POST] {url}: {e}"); None }
    }
}

/// Minimal URL encoder - escape non-ASCII + reserved chars.
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
