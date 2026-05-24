//! window.open() features string parser.
//!
//! Spec: https://html.spec.whatwg.org/multipage/window-object.html#window-open-steps

use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct WindowFeatures {
    pub raw: HashMap<String, String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub left: Option<i32>,
    pub top: Option<i32>,
    pub menubar: bool,
    pub toolbar: bool,
    pub location: bool,
    pub status: bool,
    pub resizable: bool,
    pub scrollbars: bool,
    pub noopener: bool,
    pub noreferrer: bool,
    pub popup_explicit: bool,         // "popup" feature present
}

pub fn parse(features: &str) -> WindowFeatures {
    let mut w = WindowFeatures::default();
    // Defaults per spec when feature string is empty -> noopener-window-like.
    let empty = features.trim().is_empty();
    if empty {
        // Empty features: replicate current window UI.
        w.menubar = true;
        w.toolbar = true;
        w.location = true;
        w.status = true;
        w.resizable = true;
        w.scrollbars = true;
        return w;
    }
    // Tokenize comma- or space-separated pairs of name=value.
    for pair in features.split(|c| c == ',' || c == ' ' || c == ';') {
        let pair = pair.trim();
        if pair.is_empty() { continue; }
        let (k, v) = match pair.find('=') {
            Some(i) => (pair[..i].to_ascii_lowercase(), pair[i + 1..].trim().to_string()),
            None => (pair.to_ascii_lowercase(), "yes".to_string()),
        };
        w.raw.insert(k.clone(), v.clone());
        let truthy = is_truthy(&v);
        match k.as_str() {
            "width" | "innerwidth" => w.width = v.trim_end_matches("px").parse().ok(),
            "height" | "innerheight" => w.height = v.trim_end_matches("px").parse().ok(),
            "left" | "screenx" => w.left = v.parse().ok(),
            "top" | "screeny" => w.top = v.parse().ok(),
            "menubar" => w.menubar = truthy,
            "toolbar" => w.toolbar = truthy,
            "location" => w.location = truthy,
            "status" => w.status = truthy,
            "resizable" => w.resizable = truthy,
            "scrollbars" => w.scrollbars = truthy,
            "noopener" => w.noopener = truthy,
            "noreferrer" => w.noreferrer = truthy,
            "popup" => w.popup_explicit = truthy,
            _ => {}
        }
    }
    w
}

fn is_truthy(v: &str) -> bool {
    match v.trim().to_ascii_lowercase().as_str() {
        "" | "0" | "no" | "false" => false,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_string_full_chrome() {
        let w = parse("");
        assert!(w.menubar);
        assert!(w.toolbar);
    }

    #[test]
    fn parse_width_height() {
        let w = parse("width=400,height=300");
        assert_eq!(w.width, Some(400));
        assert_eq!(w.height, Some(300));
    }

    #[test]
    fn noopener_set() {
        let w = parse("noopener");
        assert!(w.noopener);
    }

    #[test]
    fn truthy_variants() {
        let w = parse("resizable=yes,scrollbars=no,menubar=1,location=0");
        assert!(w.resizable);
        assert!(!w.scrollbars);
        assert!(w.menubar);
        assert!(!w.location);
    }

    #[test]
    fn popup_explicit() {
        let w = parse("popup=yes,width=200");
        assert!(w.popup_explicit);
    }

    #[test]
    fn px_suffix_stripped() {
        let w = parse("width=400px");
        assert_eq!(w.width, Some(400));
    }
}
