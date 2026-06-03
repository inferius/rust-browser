//! Globalni debug-breakpoint helper.
//!
//! Princip: IDE breakpointy v RustRoveru nejdou snadno conditional na "string equals"
//! (Java/Kotlin frame inspection ano, Rust ne vzdycky). Reseni: empty `#[inline(never)]`
//! funkce - IDE breakpoint sedi na ni a vola se jen kdyz match passne.
//!
//! ## Pouziti
//!
//! 1. Set env var pred run-em:
//!    ```bash
//!    BP_TAG=img cargo run -- browser static/test.html
//!    BP_ID=photo-box cargo run ...
//!    BP_CLASS=card cargo run ...
//!    BP_ID=foo,bar BP_CLASS=card cargo run ...   # multi (OR)
//!    ```
//! 2. V RustRoveru breakpoint na fn `breakpoint_layout` / `breakpoint_paint` /
//!    `breakpoint_cascade` v `src/debug_bp.rs`. (Nebo `breakpoint_hit`.)
//! 3. Run debug. Stop padne jen na elementech matching filtru.
//!
//! ## API
//!
//! - `bp_match(tag, id, class) -> bool` - check matches filter.
//! - `bp_layout!(tag, id, class)` - macro: if match -> call `breakpoint_layout()`.
//! - `bp_paint!(...)`, `bp_cascade!(...)` - per-stage variants.
//! - `bp_here!(tag, id, class)` - generic `breakpoint_hit()`.
//!
//! Macros expanduji na no-op pokud `BP_*` env vars prazdne (fast-path).

use std::sync::OnceLock;

/// Parsed filter z env vars. None = filter off (fast-path skip).
struct BpFilter {
    tags: Vec<String>,
    ids: Vec<String>,
    classes: Vec<String>,
    any: bool, // true pokud aspon jeden filter aktivni
}

fn filter() -> &'static BpFilter {
    static F: OnceLock<BpFilter> = OnceLock::new();
    F.get_or_init(|| {
        let parse = |k: &str| -> Vec<String> {
            std::env::var(k)
                .ok()
                .map(|s| s.split(',').filter(|x| !x.is_empty()).map(|x| x.trim().to_string()).collect())
                .unwrap_or_default()
        };
        let tags = parse("BP_TAG");
        let ids = parse("BP_ID");
        let classes = parse("BP_CLASS");
        let any = !tags.is_empty() || !ids.is_empty() || !classes.is_empty();
        BpFilter { tags, ids, classes, any }
    })
}

/// True pokud filter aktivni - cheap fast-path pred kazdym match call.
#[inline]
pub fn bp_enabled() -> bool {
    filter().any
}

/// Vraci true pokud (tag, id, class) odpovida nektere ze sad ve filtru.
/// Filter pravidla:
/// - prazdny seznam danemu kriteriu = wildcard (nelimituje).
/// - neprazdny seznam: hodnota musi byt v seznamu.
/// - id/class porovnani trim (whitespace) - class atr je vetsinou multi-token,
///   match true pokud aspon jeden token je ve filter classes.
pub fn bp_match(tag: &str, id: &str, class: &str) -> bool {
    let f = filter();
    if !f.any { return false; }
    if !f.tags.is_empty() && !f.tags.iter().any(|t| t == tag) { return false; }
    if !f.ids.is_empty() && !f.ids.iter().any(|i| i == id) { return false; }
    if !f.classes.is_empty() {
        let mut hit = false;
        for tok in class.split_ascii_whitespace() {
            if f.classes.iter().any(|c| c == tok) { hit = true; break; }
        }
        if !hit { return false; }
    }
    true
}

// --- Breakpoint sinks ---
// Prazdne fns. IDE breakpoint sedi na ne. #[inline(never)] kvuli optimizeru -
// jinak by je release build inlinul + breakpoint by ztratil scope.

#[inline(never)]
pub fn breakpoint_hit() {
    // IDE breakpoint sem.
    std::hint::black_box(());
}

#[inline(never)]
pub fn breakpoint_layout() {
    std::hint::black_box(());
}

#[inline(never)]
pub fn breakpoint_paint() {
    std::hint::black_box(());
}

#[inline(never)]
pub fn breakpoint_cascade() {
    std::hint::black_box(());
}

#[inline(never)]
pub fn breakpoint_build() {
    std::hint::black_box(());
}

// --- Active trap (proces sam pause-ne, debugger chytne) ---

/// Vyvola debugger trap (SIGTRAP / int3 / brk). Debugger attached -> stop tady.
/// Bez debuggeru -> proces crashne (SIGTRAP unhandled).
///
/// Use kdyz nechces rucne klikat BP na konkretni line v IDE - staci podminena
/// call uvnitr kodu.
#[inline(never)]
pub fn debug_break() {
    #[cfg(target_arch = "x86_64")]
    unsafe { std::arch::asm!("int3"); }
    #[cfg(target_arch = "x86")]
    unsafe { std::arch::asm!("int3"); }
    #[cfg(target_arch = "aarch64")]
    unsafe { std::arch::asm!("brk #0"); }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "x86", target_arch = "aarch64")))]
    {
        // Fallback: panicovat (jeste lepsi nez nic - debugger break-on-panic).
        std::process::abort();
    }
}

/// Trap pokud (tag,id,class) match filter (BP_TAG/BP_ID/BP_CLASS env vars).
#[inline]
pub fn break_if(tag: &str, id: &str, class: &str) {
    if bp_enabled() && bp_match(tag, id, class) {
        debug_break();
    }
}

// --- Conditional-BP predicates ---
// Tyto fns daj do "Condition" v IDE breakpoint dialogu. IDE evaluuje pri kazdy
// hit, BP fires jen pokud true. RustRover/CLion umi cond BP volat Rust fn.
//
// V RustRoveru: BP -> right-click -> Edit -> Condition -> napr.
//   crate::debug_bp::lb_is_id(bx, "photo-box")
// (bx musi byt local v scope kde BP sedi.)

/// Vraci true pokud LayoutBox match filtru z env vars. Generic for cond BP.
#[inline(never)]
pub fn should_break(tag: &str, id: &str, class: &str) -> bool {
    bp_enabled() && bp_match(tag, id, class)
}

/// LayoutBox helpers - pro conditional BP, kde mas v scope `bx: &LayoutBox`
/// nebo `&mut LayoutBox`.

/// Vraci true pokud LayoutBox ma dane id atr.
#[inline(never)]
pub fn lb_is_id(bx: &crate::browser::layout::LayoutBox, id: &str) -> bool {
    bx.node.as_ref()
        .and_then(|n| n.attr("id"))
        .map(|v| v == id)
        .unwrap_or(false)
}

/// Vraci true pokud LayoutBox ma dany class token (multi-token split).
#[inline(never)]
pub fn lb_is_class(bx: &crate::browser::layout::LayoutBox, class: &str) -> bool {
    bx.node.as_ref()
        .and_then(|n| n.attr("class"))
        .map(|v| v.split_ascii_whitespace().any(|t| t == class))
        .unwrap_or(false)
}

/// Vraci true pokud LayoutBox ma tag.
#[inline(never)]
pub fn lb_is_tag(bx: &crate::browser::layout::LayoutBox, tag: &str) -> bool {
    bx.tag.as_deref() == Some(tag)
}

/// Generic LayoutBox match (tag empty = any tag, atd.).
#[inline(never)]
pub fn lb_match(bx: &crate::browser::layout::LayoutBox, tag: &str, id: &str, class: &str) -> bool {
    if !tag.is_empty() && bx.tag.as_deref() != Some(tag) { return false; }
    let bid = bx.node.as_ref().and_then(|n| n.attr("id")).unwrap_or_default();
    if !id.is_empty() && bid != id { return false; }
    let bclass = bx.node.as_ref().and_then(|n| n.attr("class")).unwrap_or_default();
    if !class.is_empty() && !bclass.split_ascii_whitespace().any(|t| t == class) { return false; }
    true
}

/// Node helpers - pro stage build_box kde mas `node: &Rc<Node>`.

#[inline(never)]
pub fn node_is_id(node: &crate::browser::dom::Node, id: &str) -> bool {
    node.attr("id").map(|v| v == id).unwrap_or(false)
}

#[inline(never)]
pub fn node_is_class(node: &crate::browser::dom::Node, class: &str) -> bool {
    node.attr("class")
        .map(|v| v.split_ascii_whitespace().any(|t| t == class))
        .unwrap_or(false)
}

#[inline(never)]
pub fn node_is_tag(node: &crate::browser::dom::Node, tag: &str) -> bool {
    node.tag_name_ref().map(|t| t == tag).unwrap_or(false)
}

// --- Macros (lazy match - skip pokud filter off) ---

/// Hit generic breakpoint pokud (tag,id,class) match filter.
#[macro_export]
macro_rules! bp_here {
    ($tag:expr, $id:expr, $class:expr) => {
        if $crate::debug_bp::bp_enabled() && $crate::debug_bp::bp_match($tag, $id, $class) {
            $crate::debug_bp::breakpoint_hit();
        }
    };
}

/// Hit layout breakpoint pokud match filter.
#[macro_export]
macro_rules! bp_layout {
    ($tag:expr, $id:expr, $class:expr) => {
        if $crate::debug_bp::bp_enabled() && $crate::debug_bp::bp_match($tag, $id, $class) {
            $crate::debug_bp::breakpoint_layout();
        }
    };
}

/// Hit paint breakpoint pokud match filter.
#[macro_export]
macro_rules! bp_paint {
    ($tag:expr, $id:expr, $class:expr) => {
        if $crate::debug_bp::bp_enabled() && $crate::debug_bp::bp_match($tag, $id, $class) {
            $crate::debug_bp::breakpoint_paint();
        }
    };
}

/// Hit cascade breakpoint pokud match filter.
#[macro_export]
macro_rules! bp_cascade {
    ($tag:expr, $id:expr, $class:expr) => {
        if $crate::debug_bp::bp_enabled() && $crate::debug_bp::bp_match($tag, $id, $class) {
            $crate::debug_bp::breakpoint_cascade();
        }
    };
}

/// Hit build (box construction) breakpoint pokud match filter.
#[macro_export]
macro_rules! bp_build {
    ($tag:expr, $id:expr, $class:expr) => {
        if $crate::debug_bp::bp_enabled() && $crate::debug_bp::bp_match($tag, $id, $class) {
            $crate::debug_bp::breakpoint_build();
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_tag_only() {
        // Direct fn test (env vars set ne v unit testu - obejdeme).
        let f = BpFilter {
            tags: vec!["img".into()],
            ids: vec![],
            classes: vec![],
            any: true,
        };
        let m = |tag: &str, id: &str, class: &str| -> bool {
            if !f.tags.is_empty() && !f.tags.iter().any(|t| t == tag) { return false; }
            if !f.ids.is_empty() && !f.ids.iter().any(|i| i == id) { return false; }
            if !f.classes.is_empty() {
                let mut hit = false;
                for tok in class.split_ascii_whitespace() {
                    if f.classes.iter().any(|c| c == tok) { hit = true; break; }
                }
                if !hit { return false; }
            }
            true
        };
        assert!(m("img", "", ""));
        assert!(!m("div", "", ""));
    }

    #[test]
    fn class_multi_token() {
        let f = BpFilter {
            tags: vec![],
            ids: vec![],
            classes: vec!["card".into()],
            any: true,
        };
        let m = |class: &str| -> bool {
            for tok in class.split_ascii_whitespace() {
                if f.classes.iter().any(|c| c == tok) { return true; }
            }
            false
        };
        assert!(m("foo card bar"));
        assert!(m("card"));
        assert!(!m("foo bar"));
    }
}
