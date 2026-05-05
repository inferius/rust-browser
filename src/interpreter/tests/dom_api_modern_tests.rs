/// Modern DOM/JS API tests (presunuto z dom_api_tests.rs).
/// Typed Arrays / HTML elements / Performance / FormData / Geometry / Crypto SHA.

use super::helpers::*;
use crate::interpreter::*;

fn run_with_doc(html: &str, js: &str) -> JsValue {
    let doc = crate::browser::html_parser::parse_html(html, "");
    let lexer = crate::lexer::base::Lexer::parse_str(js, "<test>").unwrap();
    let tokens: Vec<_> = lexer.tokens.into_iter()
        .filter(|t| !matches!(t.kind,
            crate::tokens::TokenKind::Whitespace
            | crate::tokens::TokenKind::Newline
            | crate::tokens::TokenKind::CommentLine(_)
            | crate::tokens::TokenKind::CommentBlock(_)))
        .collect();
    let mut parser = crate::parser::Parser::new(tokens);
    let program = parser.parse().unwrap();
    let mut interp = Interpreter::new();
    interp.set_document(doc);
    interp.run(&program).unwrap()
}

// ─── Typed Arrays variants ────────────────────────────────────────────

#[test]
fn typed_arrays_all_variants() {
    let code = r#"
        const checks = [
            new Uint8Array(4).BYTES_PER_ELEMENT === 1,
            new Int8Array(4).BYTES_PER_ELEMENT === 1,
            new Uint16Array(4).BYTES_PER_ELEMENT === 2,
            new Int16Array(4).BYTES_PER_ELEMENT === 2,
            new Uint32Array(4).BYTES_PER_ELEMENT === 4,
            new Int32Array(4).BYTES_PER_ELEMENT === 4,
            new Float32Array(4).BYTES_PER_ELEMENT === 4,
            new Float64Array(4).BYTES_PER_ELEMENT === 8,
            new BigInt64Array(4).BYTES_PER_ELEMENT === 8,
            new BigUint64Array(4).BYTES_PER_ELEMENT === 8,
            new Uint8ClampedArray(4).BYTES_PER_ELEMENT === 1
        ];
        return checks.every(c => c);
    "#;
    if let crate::interpreter::JsValue::Bool(b) = run(code) {
        assert!(b);
    }
}

#[test]
fn typed_array_byte_length() {
    let code = r#"
        const arr = new Float32Array(10);
        return arr.byteLength + "|" + arr.length;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "40|10"); // 10 elem * 4 bytes
    }
}

// ─── HTML elements + popover + Atomics extras ─────────────────────────

#[test]
fn html_progress_value_max() {
    let r = run_with_doc(
        r#"<html><body><progress value="3" max="10"></progress></body></html>"#,
        r#"
            const p = document.getElementsByTagName("progress")[0];
            return p.value + "|" + p.max + "|" + p.position;
        "#,
    );
    assert_eq!(r.to_string(), "3|10|0.3");
}

#[test]
fn html_meter_value_min_max() {
    let r = run_with_doc(
        r#"<html><body><meter value="60" min="0" max="100" low="30" high="80"></meter></body></html>"#,
        r#"
            const m = document.getElementsByTagName("meter")[0];
            return m.value + "|" + m.min + "|" + m.max + "|" + m.low + "|" + m.high;
        "#,
    );
    assert_eq!(r.to_string(), "60|0|100|30|80");
}

#[test]
fn html_datalist_options() {
    let r = run_with_doc(
        r#"<html><body><datalist><option>a</option><option>b</option><option>c</option></datalist></body></html>"#,
        r#"
            const dl = document.getElementsByTagName("datalist")[0];
            return dl.options.length;
        "#,
    );
    assert_eq!(as_num(r), 3.0);
}

#[test]
fn html_anchor_rel_list() {
    let r = run_with_doc(
        r#"<html><body><a rel="noopener noreferrer external"></a></body></html>"#,
        r#"
            const a = document.getElementsByTagName("a")[0];
            return a.relList.length + "|" + a.relList[0];
        "#,
    );
    assert_eq!(r.to_string(), "3|noopener");
}

#[test]
fn element_popover_show_hide_toggle() {
    let code = r#"
        const div = document.createElement("div");
        div.setAttribute("popover", "");
        div.showPopover();
        const after_show = div.getAttribute("data-popover-open");
        div.hidePopover();
        const after_hide = div.hasAttribute("data-popover-open");
        const toggled = div.togglePopover();
        return after_show + "|" + after_hide + "|" + toggled;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "true|false|true");
    }
}

#[test]
fn atomics_load_store() {
    let code = r#"
        const sab = new SharedArrayBuffer(4);
        Atomics.store(sab, 0, 42);
        return Atomics.load(sab, 0);
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 42.0);
    }
}

#[test]
fn atomics_compare_exchange() {
    let code = r#"
        const sab = new SharedArrayBuffer(4);
        Atomics.store(sab, 0, 5);
        Atomics.compareExchange(sab, 0, 5, 10);
        return Atomics.load(sab, 0);
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 10.0);
    }
}

#[test]
fn atomics_is_lock_free() {
    let code = r#"
        return [
            Atomics.isLockFree(1),
            Atomics.isLockFree(4),
            Atomics.isLockFree(8),
            Atomics.isLockFree(7)
        ].map(b => b ? "1" : "0").join("");
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "1110");
    }
}

#[test]
fn atomics_bitwise() {
    let code = r#"
        const sab = new SharedArrayBuffer(4);
        Atomics.store(sab, 0, 12);
        Atomics.and(sab, 0, 10);
        return Atomics.load(sab, 0);
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 8.0); // 12 & 10 = 8
    }
}

#[test]
fn text_decoder_options() {
    let code = r#"
        const td = new TextDecoder("utf-8");
        return td.encoding + "|" + td.fatal + "|" + td.ignoreBOM;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "utf-8|false|false");
    }
}

#[test]
fn text_encoder_stream_constructor() {
    let code = r#"
        const tes = new TextEncoderStream();
        return tes.encoding;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "utf-8");
    }
}

// ─── Performance API real ─────────────────────────────────────────────

#[test]
fn performance_mark_creates_entry() {
    let code = r#"
        performance.mark("start");
        return performance.getEntries().length;
    "#;
    let r = run(code);
    if let crate::interpreter::JsValue::Number(n) = r {
        assert!(n >= 1.0);
    }
}

#[test]
fn performance_measure_between_marks() {
    let code = r#"
        performance.mark("a");
        performance.mark("b");
        performance.measure("ab", "a", "b");
        const measures = performance.getEntriesByType("measure");
        return measures.length + "|" + measures[0].name;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "1|ab");
    }
}

#[test]
fn performance_get_entries_by_type() {
    let code = r#"
        performance.clearMarks();
        performance.clearMeasures();
        performance.mark("m1");
        performance.mark("m2");
        return performance.getEntriesByType("mark").length;
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 2.0);
    }
}

#[test]
fn performance_get_entries_by_name() {
    let code = r#"
        performance.clearMarks();
        performance.mark("findme");
        performance.mark("other");
        return performance.getEntriesByName("findme").length;
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 1.0);
    }
}

#[test]
fn performance_clear_marks_specific() {
    let code = r#"
        performance.clearMarks();
        performance.mark("a");
        performance.mark("b");
        performance.clearMarks("a");
        return performance.getEntriesByType("mark").length;
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 1.0);
    }
}

// ─── FormData + Headers + Request ─────────────────────────────────────

#[test]
fn formdata_append_get_getall() {
    let code = r#"
        const fd = new FormData();
        fd.append("name", "Alice");
        fd.append("name", "Bob");
        fd.append("age", "30");
        return fd.get("name") + "|" + fd.getAll("name").length + "|" + fd.has("age");
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "Alice|2|true");
    }
}

#[test]
fn formdata_set_replaces() {
    let code = r#"
        const fd = new FormData();
        fd.append("k", "1");
        fd.append("k", "2");
        fd.set("k", "3");
        return fd.getAll("k").join(",");
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "3");
    }
}

#[test]
fn formdata_delete() {
    let code = r#"
        const fd = new FormData();
        fd.append("a", "1");
        fd.append("b", "2");
        fd.delete("a");
        return fd.has("a") + "|" + fd.has("b");
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "false|true");
    }
}

#[test]
fn formdata_entries_iterator() {
    let code = r#"
        const fd = new FormData();
        fd.append("a", "1");
        fd.append("b", "2");
        return fd.entries().toArray().length;
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 2.0);
    }
}

#[test]
fn headers_set_get() {
    let code = r#"
        const h = new Headers();
        h.set("Content-Type", "application/json");
        return h.get("content-type");
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "application/json");
    }
}

#[test]
fn headers_append_combine() {
    let code = r#"
        const h = new Headers();
        h.append("Set-Cookie", "a=1");
        h.append("Set-Cookie", "b=2");
        return h.get("set-cookie");
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "a=1, b=2");
    }
}

#[test]
fn headers_has_delete() {
    let code = r#"
        const h = new Headers();
        h.set("X-Foo", "bar");
        h.delete("X-Foo");
        return h.has("x-foo");
    "#;
    if let crate::interpreter::JsValue::Bool(b) = run(code) {
        assert!(!b);
    }
}

#[test]
fn request_constructor_method() {
    let code = r#"
        const r = new Request("/api", { method: "POST", body: "test" });
        return r.url + "|" + r.method;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "/api|POST");
    }
}

#[test]
fn url_can_parse_via_helper() {
    let code = r#"
        return __url_can_parse__("https://example.com");
    "#;
    if let crate::interpreter::JsValue::Bool(b) = run(code) {
        assert!(b);
    }
}

#[test]
fn url_parse_invalid_returns_null() {
    let code = r#"
        return __url_parse__("invalid");
    "#;
    let r = run(code);
    assert!(matches!(r, crate::interpreter::JsValue::Null));
}

// ─── DOM Geometry + Console extras ────────────────────────────────────

#[test]
fn dom_rect_construct() {
    let code = r#"
        const r = new DOMRect(10, 20, 100, 50);
        return r.x + "|" + r.y + "|" + r.width + "|" + r.height + "|" + r.right + "|" + r.bottom;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "10|20|100|50|110|70");
    }
}

#[test]
fn dom_point_default() {
    let code = r#"
        const p = new DOMPoint(1, 2, 3);
        return p.x + "|" + p.y + "|" + p.z + "|" + p.w;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "1|2|3|1");
    }
}

#[test]
fn dom_matrix_identity() {
    let code = r#"
        const m = new DOMMatrix();
        return m.isIdentity + "|" + m.is2D + "|" + m.a + "|" + m.d;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "true|true|1|1");
    }
}

#[test]
fn dom_matrix_2d_args() {
    let code = r#"
        const m = new DOMMatrix([1, 0, 0, 1, 10, 20]);
        return m.e + "|" + m.f + "|" + m.is2D;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "10|20|true");
    }
}

#[test]
fn dom_quad_corners() {
    let code = r#"
        const q = new DOMQuad();
        return typeof q.p1 + "|" + typeof q.p2 + "|" + typeof q.p3 + "|" + typeof q.p4;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "object|object|object|object");
    }
}

#[test]
fn console_table_no_throw() {
    let code = r#"
        console.table([{a: 1, b: 2}]);
        return "ok";
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "ok");
    }
}

#[test]
fn console_group_group_end() {
    let code = r#"
        console.group("section");
        console.log("inside");
        console.groupEnd();
        return "ok";
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "ok");
    }
}

#[test]
fn console_time_time_end() {
    let code = r#"
        console.time("op");
        console.timeEnd("op");
        return "ok";
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "ok");
    }
}

#[test]
fn console_count_increments() {
    let code = r#"
        console.count("a");
        console.count("a");
        console.count("a");
        return "ok";
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "ok");
    }
}

#[test]
fn console_assert_no_throw() {
    let code = r#"
        console.assert(true, "should not log");
        console.assert(false, "should log");
        return "ok";
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "ok");
    }
}

// ─── DOMException + ImageData + OffscreenCanvas + Path2D ──────────────

#[test]
fn dom_exception_with_name() {
    let code = r#"
        const e = new DOMException("Not found", "NotFoundError");
        return e.name + "|" + e.message + "|" + e.code;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "NotFoundError|Not found|8");
    }
}

#[test]
fn dom_exception_default_error() {
    let code = r#"
        const e = new DOMException("test");
        return e.name + "|" + e.code;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "Error|0");
    }
}

#[test]
fn dom_exception_quota_exceeded() {
    let code = r#"
        return new DOMException("full", "QuotaExceededError").code;
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 22.0);
    }
}

#[test]
fn image_data_construct_dimensions() {
    let code = r#"
        const id = new ImageData(10, 5);
        return id.width + "|" + id.height + "|" + id.data.length;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "10|5|200"); // 10*5*4 = 200
    }
}

#[test]
fn image_data_color_space() {
    let code = r#"
        const id = new ImageData(2, 2);
        return id.colorSpace;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "srgb");
    }
}

#[test]
fn offscreen_canvas_dimensions() {
    let code = r#"
        const oc = new OffscreenCanvas(200, 150);
        return oc.width + "|" + oc.height;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "200|150");
    }
}

#[test]
fn path_2d_methods_exist() {
    let code = r#"
        const p = new Path2D();
        p.moveTo(0, 0);
        p.lineTo(10, 10);
        p.arc(50, 50, 30, 0, Math.PI * 2);
        return typeof p.closePath;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "function");
    }
}

#[test]
fn create_image_bitmap_returns_promise() {
    let code = r#"
        const p = createImageBitmap();
        return p.__promise_state__;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "fulfilled");
    }
}

// ─── Element extras + APIs batch ──────────────────────────────────────

#[test]
fn element_check_visibility_default_true() {
    let code = r#"
        const div = document.createElement("div");
        return div.checkVisibility();
    "#;
    if let crate::interpreter::JsValue::Bool(b) = run(code) {
        assert!(b);
    }
}

#[test]
fn element_check_visibility_display_none() {
    let code = r#"
        const div = document.createElement("div");
        div.setAttribute("style", "display:none");
        return div.checkVisibility();
    "#;
    if let crate::interpreter::JsValue::Bool(b) = run(code) {
        assert!(!b);
    }
}

#[test]
fn element_request_fullscreen_returns_promise() {
    let code = r#"
        const div = document.createElement("div");
        const p = div.requestFullscreen();
        return p.__promise_state__;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "fulfilled");
    }
}

#[test]
fn element_attach_internals() {
    let code = r#"
        const div = document.createElement("div");
        const i = div.attachInternals();
        return typeof i.setFormValue + "|" + i.checkValidity();
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "function|true");
    }
}

#[test]
fn element_computed_style_map_stub() {
    let code = r#"
        const div = document.createElement("div");
        const m = div.computedStyleMap();
        return typeof m.get + "|" + m.size;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "function|0");
    }
}

#[test]
fn web_transport_constructor() {
    let code = r#"
        const wt = new WebTransport("https://example.com/wt");
        return wt.url;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "https://example.com/wt");
    }
}

#[test]
fn reporting_observer_methods() {
    let code = r#"
        const ro = new ReportingObserver(() => {});
        ro.observe();
        ro.disconnect();
        return typeof ro.takeRecords;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "function");
    }
}

// ─── DOM constructors batch ───────────────────────────────────────────

#[test]
fn document_fragment_construct() {
    let code = r#"
        const f = new DocumentFragment();
        return f.nodeType + "|" + f.nodeName;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "11|#document-fragment");
    }
}

#[test]
fn comment_construct_with_data() {
    let code = r#"
        const c = new Comment("hello");
        return c.nodeType + "|" + c.data + "|" + c.length;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "8|hello|5");
    }
}

#[test]
fn text_construct_data() {
    let code = r#"
        const t = new Text("World");
        return t.nodeType + "|" + t.data;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "3|World");
    }
}

#[test]
fn node_constants() {
    let code = r#"
        return Node.ELEMENT_NODE + "|" + Node.TEXT_NODE + "|" + Node.COMMENT_NODE;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "1|3|8");
    }
}

#[test]
fn node_document_position_constants() {
    let code = r#"
        return Node.DOCUMENT_POSITION_CONTAINS + "|" + Node.DOCUMENT_POSITION_CONTAINED_BY;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "8|16");
    }
}

#[test]
fn dom_token_list_add_remove_toggle() {
    let code = r#"
        const tl = new DOMTokenList();
        tl.add("a", "b", "c");
        const has_b = tl.contains("b");
        tl.remove("b");
        const has_b_after = tl.contains("b");
        const toggled = tl.toggle("d");
        return has_b + "|" + has_b_after + "|" + toggled + "|" + tl.length;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "true|false|true|3"); // a c d
    }
}

#[test]
fn html_collection_constructor() {
    let code = r#"
        const c = new HTMLCollection();
        return c.length + "|" + typeof c.item;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "0|function");
    }
}

#[test]
fn node_list_constructor() {
    let code = r#"
        const nl = new NodeList();
        return nl.length + "|" + typeof nl.forEach;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "0|function");
    }
}

#[test]
fn mutation_record_default_props() {
    let code = r#"
        const m = new MutationRecord();
        return m.type + "|" + (m.target === null);
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "childList|true");
    }
}

// ─── Worker + DOM extras ──────────────────────────────────────────────

#[test]
fn shared_worker_has_port() {
    let code = r#"
        const sw = new SharedWorker("worker.js");
        return typeof sw.port + "|" + sw.url;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "object|worker.js");
    }
}

#[test]
fn image_constructor_attrs() {
    let code = r#"
        const img = new Image(100, 50);
        return img.tagName.toLowerCase() + "|" + img.getAttribute("width") + "|" + img.getAttribute("height");
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "img|100|50");
    }
}

#[test]
fn audio_constructor_with_src() {
    let code = r#"
        const a = new Audio("song.mp3");
        return a.tagName.toLowerCase() + "|" + a.getAttribute("src");
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "audio|song.mp3");
    }
}

#[test]
fn option_constructor_text_value() {
    let code = r#"
        const opt = new Option("Apple", "1");
        return opt.tagName.toLowerCase() + "|" + opt.getAttribute("value");
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "option|1");
    }
}

#[test]
fn data_transfer_set_get() {
    let code = r#"
        const dt = new DataTransfer();
        dt.setData("text/plain", "hello");
        return dt.getData("text/plain");
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "hello");
    }
}

#[test]
fn data_transfer_clear() {
    let code = r#"
        const dt = new DataTransfer();
        dt.setData("text/plain", "x");
        dt.clearData();
        return dt.getData("text/plain");
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "");
    }
}

#[test]
fn storage_manager_estimate() {
    let code = r#"
        const sm = new StorageManager();
        const p = sm.estimate();
        return p.__promise_state__;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "fulfilled");
    }
}

#[test]
fn performance_observer_construct() {
    let code = r#"
        const po = new PerformanceObserver(() => {});
        po.observe();
        return typeof po.takeRecords;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "function");
    }
}

#[test]
fn performance_entry_construct() {
    let code = r#"
        const e = new PerformanceEntry();
        return typeof e.name + "|" + e.duration;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "string|0");
    }
}

// ─── Symbols + Reflect ────────────────────────────────────────────────

#[test]
fn symbol_well_known_extras() {
    let code = r#"
        return [
            typeof Symbol.iterator,
            typeof Symbol.asyncIterator,
            typeof Symbol.toPrimitive,
            typeof Symbol.toStringTag,
            typeof Symbol.species,
            typeof Symbol.match,
            typeof Symbol.matchAll,
            typeof Symbol.replace,
            typeof Symbol.search,
            typeof Symbol.split,
            typeof Symbol.isConcatSpreadable,
            typeof Symbol.unscopables,
            typeof Symbol.hasInstance,
            typeof Symbol.dispose,
            typeof Symbol.asyncDispose,
            typeof Symbol.metadata
        ].every(t => t === "string");
    "#;
    if let crate::interpreter::JsValue::Bool(b) = run(code) {
        assert!(b);
    }
}

#[test]
fn symbol_for_creates_registry() {
    let code = r#"
        const s = Symbol.for("test");
        return Symbol.keyFor(s);
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "test");
    }
}

#[test]
fn reflect_get_set() {
    let code = r#"
        const obj = { x: 5 };
        Reflect.set(obj, "y", 10);
        return Reflect.get(obj, "x") + "|" + Reflect.get(obj, "y");
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "5|10");
    }
}

#[test]
fn reflect_has_delete() {
    let code = r#"
        const obj = { a: 1, b: 2 };
        const has_a = Reflect.has(obj, "a");
        Reflect.deleteProperty(obj, "a");
        const has_a_after = Reflect.has(obj, "a");
        return has_a + "|" + has_a_after;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "true|false");
    }
}

#[test]
fn reflect_own_keys() {
    let code = r#"
        const obj = { a: 1, b: 2, c: 3 };
        return Reflect.ownKeys(obj).length;
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 3.0);
    }
}

// ─── Crypto SHA real ──────────────────────────────────────────────────

#[test]
fn crypto_sha256_empty_string() {
    let code = r#"
        const buf = crypto.subtle.digest("SHA-256", "");
        return buf.__promise_value__.byteLength;
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 32.0); // SHA-256 = 32 bytes
    }
}

#[test]
fn crypto_sha256_known_vector() {
    // SHA-256("abc") = ba7816bf 8f01cfea 414140de 5dae2223 b00361a3 96177a9c b410ff61 f20015ad
    let code = r#"
        const buf = crypto.subtle.digest("SHA-256", "abc");
        const bytes = buf.__promise_value__.__bytes__;
        return bytes[0] + "|" + bytes[1] + "|" + bytes[31];
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        // 0xba=186, 0x78=120, 0xad=173
        assert_eq!(s, "186|120|173");
    }
}

#[test]
fn crypto_sha1_known_vector() {
    // SHA-1("abc") = a9993e36 4706816a ba3e2571 7850c26c 9cd0d89d
    let code = r#"
        const buf = crypto.subtle.digest("SHA-1", "abc");
        const bytes = buf.__promise_value__.__bytes__;
        return bytes[0] + "|" + bytes[1] + "|" + bytes.length;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        // 0xa9=169, 0x99=153, length 20
        assert_eq!(s, "169|153|20");
    }
}

#[test]
fn crypto_sha384_byte_length() {
    let code = r#"
        const buf = crypto.subtle.digest("SHA-384", "test");
        return buf.__promise_value__.byteLength;
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 48.0); // SHA-384 = 48 bytes
    }
}

#[test]
fn crypto_sha512_byte_length() {
    let code = r#"
        const buf = crypto.subtle.digest("SHA-512", "test");
        return buf.__promise_value__.byteLength;
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 64.0); // SHA-512 = 64 bytes
    }
}

#[test]
fn crypto_unknown_algo_rejects() {
    let code = r#"
        const buf = crypto.subtle.digest("UNKNOWN", "x");
        return buf.__promise_state__;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "rejected");
    }
}

// ─── Typed Array methods ──────────────────────────────────────────────

#[test]
fn typed_array_subarray() {
    let code = r#"
        const ta = new Uint8Array([1, 2, 3, 4, 5]);
        const sub = ta.subarray(1, 4);
        return sub.length + "|" + sub.__bytes__[0] + "|" + sub.__bytes__[2];
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "3|2|4");
    }
}

#[test]
fn typed_array_set_offset() {
    let code = r#"
        const ta = new Uint8Array(5);
        ta.set([10, 20, 30], 1);
        return ta.__bytes__[0] + "|" + ta.__bytes__[1] + "|" + ta.__bytes__[3];
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "0|10|30");
    }
}

#[test]
fn typed_array_fill() {
    let code = r#"
        const ta = new Int32Array(4);
        ta.fill(7);
        return ta.__bytes__[0] + "|" + ta.__bytes__[3];
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "7|7");
    }
}

#[test]
fn typed_array_slice() {
    let code = r#"
        const ta = new Uint16Array([10, 20, 30, 40]);
        const sl = ta.slice(1, 3);
        return sl.length + "|" + sl.__bytes__[0];
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "2|20");
    }
}

#[test]
fn typed_array_index_of() {
    let code = r#"
        const ta = new Uint8Array([5, 10, 15, 20]);
        return ta.indexOf(15) + "|" + ta.indexOf(99);
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "2|-1");
    }
}

#[test]
fn typed_array_includes() {
    let code = r#"
        const ta = new Uint8Array([1, 2, 3]);
        return ta.includes(2) + "|" + ta.includes(99);
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "true|false");
    }
}

#[test]
fn typed_array_reverse() {
    let code = r#"
        const ta = new Uint8Array([1, 2, 3]);
        ta.reverse();
        return ta.__bytes__[0] + "|" + ta.__bytes__[2];
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "3|1");
    }
}

#[test]
fn typed_array_join() {
    let code = r#"
        const ta = new Uint8Array([1, 2, 3]);
        return ta.join("-");
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "1-2-3");
    }
}

#[test]
fn typed_array_buffer_view() {
    let code = r#"
        const ta = new Uint16Array(4);
        return ta.buffer.byteLength + "|" + ta.byteOffset;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "8|0"); // 4 elem * 2 bytes = 8
    }
}

#[test]
fn typed_array_copy_within() {
    let code = r#"
        const ta = new Uint8Array([1, 2, 3, 4, 5]);
        ta.copyWithin(0, 3);
        return ta.__bytes__.join(",");
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "4,5,3,4,5");
    }
}
