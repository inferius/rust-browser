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
pub mod scroll;
pub mod scroll_anim;
pub mod security;
pub mod view_transitions;
pub mod tree_walker;
pub mod lcd_aa;
pub mod a11y;
pub mod a11y_prefs;
pub mod atlas_multipage;
pub mod image_decoders;
pub mod modules_esm;
pub mod floats;
pub mod hdr_color;
pub mod net;
pub mod forced_colors;
pub mod render;
pub mod spatial_hit;
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
pub mod sandbox;
pub mod image_decoder;
pub mod avif_decode;
pub mod jxl_decode;
pub mod heif_decode;
pub mod accessibility_tree;
pub mod url_parser;
pub mod text_bidi;
pub mod unicode_segmenter;
pub mod font_fallback;
pub mod opentype_features;
pub mod media;
pub mod event_dispatch;
pub mod shadow_dom;
pub mod selector_engine;
pub mod input;
pub mod svg;
pub mod css;
pub mod html5;
pub mod viewport;
pub mod hidpi;
pub mod drag_drop;
pub mod autoscroll;
pub mod spellcheck;
pub mod autofill;
pub mod locale;
pub mod favicon;
pub mod manifest;
pub mod password_manager;
pub mod extensions;
pub mod bookmarks;
pub mod history_db;
pub mod downloads;
pub mod dialog_manager;
pub mod private_browsing;
pub mod session_state;
pub mod tab_groups;
pub mod reader_mode;
pub mod translator;
pub mod zoom_levels;
pub mod site_settings;
pub mod reload_strategy;
pub mod proxy_resolver;
pub mod web_vitals;
pub mod safe_browsing;
pub mod speculation_rules;
pub mod origin_trials;
pub mod webdriver_protocol;
pub mod contenteditable_model;
pub mod spatial_nav;
pub mod lazy_loading;
pub mod page_visibility;
pub mod print_preview;
pub mod snap_scroll;
pub mod overscroll;
pub mod input_devices;
pub mod battery_status;
pub mod network_info;
pub mod focus_manager;
pub mod clipboard_history;
pub mod ad_blocker;
pub mod bf_cache;
pub mod wheel_normalize;
pub mod window_features;
pub mod crash_reporter;
pub mod pull_to_refresh;
pub mod screen_orientation;
pub mod display_link;
pub mod telemetry;
pub mod experiment_flags;
pub mod quirks_mode;
pub mod charset_detect;
pub mod geolocation_provider;
pub mod os_clipboard;

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
