/// Browser engine moduly.
///
/// - dom: DOM tree (Node, Element, TextNode, Document)
/// - html_parser: parse HTML pres html5ever -> DOM
/// - css_parser: parse CSS pres cssparser -> Vec<Rule>
/// - cascade: aplikuj CSS na DOM (computed styles per element)
/// - layout: layout engine (block, inline, flex - zatim block)
/// - paint: painter generuje display list
/// - render: wgpu render loop + window management

pub mod dom;
pub mod html_parser;
pub mod css_parser;
pub mod cascade;
pub mod computed_style;
pub mod layout;
pub mod layout_engine;
pub mod paint;
pub mod compositor;
pub mod render;
pub mod devtools_panel;
pub mod woff;
pub mod variable_fonts;
pub mod emoji_fonts;
pub mod webgl_helpers;
pub mod dom_input_buffer;
pub mod interactive;
pub mod selection;
pub mod textrun;
pub mod editor;
pub mod async_jobs;

/// Vraci true pokud je v env nastaveno RWE_VERBOSE (libovolna hodnota).
/// Gating debug/info eprintln spamu - errors vystupuji vzdy.
/// OnceLock cache - env::var precteni 1x per process.
pub fn rwe_verbose() -> bool {
    static V: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *V.get_or_init(|| std::env::var("RWE_VERBOSE").is_ok())
}

/// Verbose eprintln - vystupuje jen pri RWE_VERBOSE env. Pouzit pro
/// debug/info zpravy ktere by jinak spamovaly stderr (font load OK,
/// startup info, zoom, addr bar, history navigation).
#[macro_export]
macro_rules! vlog {
    ($($arg:tt)*) => {
        if $crate::browser::rwe_verbose() {
            eprintln!($($arg)*);
        }
    };
}

#[cfg(test)]
mod tests;
