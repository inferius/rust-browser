//! DevTools Target Adapter (D2).
//!
//! Adapter mezi devtools-proto wire typy (DevtoolsRequest/Response/Event)
//! a internim WebView API. Frontend (devtools UI, Phase D3-D4) posila
//! `DevtoolsRequest` -> target ho dispatchne na DOM/CSS/Runtime/Debugger
//! domain handlery -> vrati `DevtoolsResponse`.
//!
//! ## Architektura
//!
//! ```text
//! +---------------+    DevtoolsRequest   +-----------------+
//! | DevTools UI   | -------------------> | DevtoolsTarget  |
//! | (WebView 2)   |                      |  + &mut WebView |
//! |               | <------------------- |                 |
//! +---------------+    DevtoolsResponse  +-----------------+
//!                      DevtoolsEvent
//! ```
//!
//! ## API design
//!
//! DevtoolsTarget drzi jen stav (events buffer + breakpoint counter), ne
//! WebView referenci. Pri kazdem `handle_request` predame `&mut WebView`
//! - target dispatchne handler s primym borrow do page state.
//!
//! Vyhoda: shell ma `webview: Option<WebView>` (ne Rc<RefCell>), dispatch
//! probiha v main loop, kde uz mame `&mut self.webview`.
//!
//! ## Aktualni status (D2 refactored pro D6b)
//!
//! - DevtoolsTarget = events buffer + bp_id counter (no webview field).
//! - handle_request bere `&mut WebView` parametr.
//! - DOM/Debugger.resume/setBreakpoint real impl. CSS/Runtime/Network/
//!   Performance stub-level.

use std::cell::RefCell;
use std::rc::Rc;

use rwe_devtools_proto::{
    DevtoolsEvent, DevtoolsError, DevtoolsRequest, DevtoolsResponse, Method, error_codes,
};

use super::webview::WebView;

/// DevTools target - per-page adapter mezi protocol wire + WebView state.
///
/// Holds: events buffer + breakpoint id counter. WebView se predava
/// jako `&mut WebView` parametr na kazdy `handle_request`.
pub struct DevtoolsTarget {
    /// Pending events buffer - flush pres `take_events` z host loop.
    events: RefCell<Vec<DevtoolsEvent>>,
    /// Sekvencni breakpoint ID generator (Debugger.setBreakpoint vrati id).
    next_breakpoint_id: RefCell<u64>,
}

impl Default for DevtoolsTarget {
    fn default() -> Self { Self::new() }
}

impl DevtoolsTarget {
    /// Vytvori novy target (stateless mimo events + bp counter).
    pub fn new() -> Self {
        Self {
            events: RefCell::new(Vec::new()),
            next_breakpoint_id: RefCell::new(1),
        }
    }

    /// Dispatch jedne request pres method string. Vrati response s result
    /// nebo error. Neznamy method = `METHOD_NOT_FOUND` (-32601).
    pub fn handle_request(&self, webview: &mut WebView, req: DevtoolsRequest) -> DevtoolsResponse {
        let method = match Method::from_method_str(&req.method) {
            Some(m) => m,
            None => return Self::error_response(req.id, error_codes::METHOD_NOT_FOUND,
                format!("Unknown method '{}'", req.method)),
        };

        match method {
            // DOM domain
            Method::DomGetDocument => self.handle_dom_get_document(webview, req),
            Method::DomQuerySelector => self.handle_dom_query_selector(webview, req),
            Method::DomQuerySelectorAll => self.handle_dom_query_selector_all(webview, req),
            Method::DomGetAttributes => self.handle_dom_get_attributes(webview, req),
            Method::DomSetAttributeValue => self.handle_dom_set_attribute_value(webview, req),
            Method::DomRemoveAttribute => self.handle_dom_remove_attribute(webview, req),

            // CSS domain
            Method::CssGetMatchedStylesForNode => self.handle_css_get_matched_styles(webview, req),
            Method::CssGetComputedStyleForNode => self.handle_css_get_computed_style(webview, req),
            Method::CssSetPropertyText => self.handle_css_set_property_text(webview, req),

            // Runtime domain
            Method::RuntimeEvaluate => self.handle_runtime_evaluate(webview, req),

            // Debugger domain
            Method::DebuggerSetBreakpoint => self.handle_debugger_set_breakpoint(webview, req),
            Method::DebuggerRemoveBreakpoint => self.handle_debugger_remove_breakpoint(webview, req),
            Method::DebuggerResume => self.handle_debugger_resume(webview, req),
            Method::DebuggerStepOver => self.handle_debugger_step_over(webview, req),
            Method::DebuggerStepInto => self.handle_debugger_step_into(webview, req),
            Method::DebuggerStepOut => self.handle_debugger_step_out(webview, req),
            Method::DebuggerPause => self.handle_debugger_pause(webview, req),

            // Network domain
            Method::NetworkGetResponseBody => self.handle_network_get_response_body(webview, req),

            // Performance domain
            Method::PerformanceGetMetrics => self.handle_performance_get_metrics(webview, req),
        }
    }

    /// Vyber vsechny pending events (buffered od posledniho call) + clear.
    /// Host loop volat per frame -> push do frontend WebView pres JS bridge.
    pub fn take_events(&self) -> Vec<DevtoolsEvent> {
        std::mem::take(&mut *self.events.borrow_mut())
    }

    /// Vlozi event do bufferu - intra-engine signal (e.g. po nav, BP hit).
    pub fn push_event(&self, event: DevtoolsEvent) {
        self.events.borrow_mut().push(event);
    }

    // ============================================================
    // DOM domain handlers
    // ============================================================

    fn handle_dom_get_document(&self, webview: &WebView, req: DevtoolsRequest) -> DevtoolsResponse {
        use rwe_devtools_proto::dom::{GetDocumentParams, GetDocumentResult};
        let params: GetDocumentParams = match serde_json::from_value(req.params.clone()) {
            Ok(p) => p,
            Err(_) => GetDocumentParams { depth: Some(-1), pierce: Some(false) },
        };
        let depth = params.depth.unwrap_or(-1);
        let doc = match webview.document() {
            Some(d) => d,
            None => return Self::error_response(req.id, error_codes::INTERNAL_ERROR,
                "No document loaded".to_string()),
        };
        let root = serialize_node(&doc.root, depth, 0);
        let result = GetDocumentResult { root };
        Self::ok_response(req.id, &result)
    }

    fn handle_dom_query_selector(&self, _webview: &WebView, req: DevtoolsRequest) -> DevtoolsResponse {
        use rwe_devtools_proto::dom::{QuerySelectorParams, QuerySelectorResult};
        let _params: QuerySelectorParams = match serde_json::from_value(req.params.clone()) {
            Ok(p) => p,
            Err(e) => return Self::error_response(req.id, error_codes::INVALID_PARAMS,
                format!("Invalid params: {e}")),
        };
        // Stub: real impl pres selectors::matching engine + DOM walk.
        let result = QuerySelectorResult { node_id: None };
        Self::ok_response(req.id, &result)
    }

    fn handle_dom_query_selector_all(&self, _webview: &WebView, req: DevtoolsRequest) -> DevtoolsResponse {
        use rwe_devtools_proto::dom::{QuerySelectorAllParams, QuerySelectorAllResult};
        let _params: QuerySelectorAllParams = match serde_json::from_value(req.params.clone()) {
            Ok(p) => p,
            Err(e) => return Self::error_response(req.id, error_codes::INVALID_PARAMS,
                format!("Invalid params: {e}")),
        };
        let result = QuerySelectorAllResult { node_ids: Vec::new() };
        Self::ok_response(req.id, &result)
    }

    fn handle_dom_get_attributes(&self, webview: &WebView, req: DevtoolsRequest) -> DevtoolsResponse {
        use rwe_devtools_proto::dom::{GetAttributesParams, GetAttributesResult};
        let params: GetAttributesParams = match serde_json::from_value(req.params.clone()) {
            Ok(p) => p,
            Err(e) => return Self::error_response(req.id, error_codes::INVALID_PARAMS,
                format!("Invalid params: {e}")),
        };
        let doc = match webview.document() {
            Some(d) => d,
            None => return Self::error_response(req.id, error_codes::INTERNAL_ERROR,
                "No document loaded".to_string()),
        };
        let node = match find_node_by_id(&doc.root, params.node_id) {
            Some(n) => n,
            None => return Self::error_response(req.id, error_codes::NODE_NOT_FOUND,
                format!("Node {} not found", params.node_id)),
        };
        let attrs = node.attributes.borrow();
        let mut flat = Vec::with_capacity(attrs.len() * 2);
        for (k, v) in attrs.iter() {
            flat.push(k.clone());
            flat.push(v.clone());
        }
        let result = GetAttributesResult { attributes: flat };
        Self::ok_response(req.id, &result)
    }

    fn handle_dom_set_attribute_value(&self, webview: &WebView, req: DevtoolsRequest) -> DevtoolsResponse {
        use rwe_devtools_proto::dom::SetAttributeValueParams;
        let params: SetAttributeValueParams = match serde_json::from_value(req.params.clone()) {
            Ok(p) => p,
            Err(e) => return Self::error_response(req.id, error_codes::INVALID_PARAMS,
                format!("Invalid params: {e}")),
        };
        let doc = match webview.document() {
            Some(d) => d,
            None => return Self::error_response(req.id, error_codes::INTERNAL_ERROR,
                "No document loaded".to_string()),
        };
        let node = match find_node_by_id(&doc.root, params.node_id) {
            Some(n) => n,
            None => return Self::error_response(req.id, error_codes::NODE_NOT_FOUND,
                format!("Node {} not found", params.node_id)),
        };
        node.attributes.borrow_mut().insert(params.name.clone(), params.value.clone());
        // Broadcast event - frontend updatne UI.
        self.push_event(DevtoolsEvent {
            method: "DOM.attributeModified".to_string(),
            params: serde_json::json!({
                "nodeId": params.node_id,
                "name": params.name,
                "value": params.value,
            }),
        });
        Self::ok_response_raw(req.id, serde_json::json!({}))
    }

    fn handle_dom_remove_attribute(&self, webview: &WebView, req: DevtoolsRequest) -> DevtoolsResponse {
        use rwe_devtools_proto::dom::RemoveAttributeParams;
        let params: RemoveAttributeParams = match serde_json::from_value(req.params.clone()) {
            Ok(p) => p,
            Err(e) => return Self::error_response(req.id, error_codes::INVALID_PARAMS,
                format!("Invalid params: {e}")),
        };
        let doc = match webview.document() {
            Some(d) => d,
            None => return Self::error_response(req.id, error_codes::INTERNAL_ERROR,
                "No document loaded".to_string()),
        };
        let node = match find_node_by_id(&doc.root, params.node_id) {
            Some(n) => n,
            None => return Self::error_response(req.id, error_codes::NODE_NOT_FOUND,
                format!("Node {} not found", params.node_id)),
        };
        node.attributes.borrow_mut().remove(&params.name);
        Self::ok_response_raw(req.id, serde_json::json!({}))
    }

    // ============================================================
    // CSS domain handlers
    // ============================================================

    fn handle_css_get_matched_styles(&self, _webview: &WebView, req: DevtoolsRequest) -> DevtoolsResponse {
        use rwe_devtools_proto::css::{GetMatchedStylesForNodeParams, GetMatchedStylesForNodeResult};
        let _params: GetMatchedStylesForNodeParams = match serde_json::from_value(req.params.clone()) {
            Ok(p) => p,
            Err(e) => return Self::error_response(req.id, error_codes::INVALID_PARAMS,
                format!("Invalid params: {e}")),
        };
        // Stub: real impl projde stylesheets + cascade::match_selector pro daly node.
        let result = GetMatchedStylesForNodeResult {
            inline_style: None,
            matched_rules: Vec::new(),
        };
        Self::ok_response(req.id, &result)
    }

    fn handle_css_get_computed_style(&self, _webview: &WebView, req: DevtoolsRequest) -> DevtoolsResponse {
        use rwe_devtools_proto::css::{GetComputedStyleForNodeParams, GetComputedStyleForNodeResult};
        let _params: GetComputedStyleForNodeParams = match serde_json::from_value(req.params.clone()) {
            Ok(p) => p,
            Err(e) => return Self::error_response(req.id, error_codes::INVALID_PARAMS,
                format!("Invalid params: {e}")),
        };
        let result = GetComputedStyleForNodeResult {
            computed_style: Vec::new(),
        };
        Self::ok_response(req.id, &result)
    }

    fn handle_css_set_property_text(&self, _webview: &WebView, req: DevtoolsRequest) -> DevtoolsResponse {
        use rwe_devtools_proto::css::SetPropertyTextParams;
        let _params: SetPropertyTextParams = match serde_json::from_value(req.params.clone()) {
            Ok(p) => p,
            Err(e) => return Self::error_response(req.id, error_codes::INVALID_PARAMS,
                format!("Invalid params: {e}")),
        };
        Self::ok_response_raw(req.id, serde_json::json!({}))
    }

    // ============================================================
    // Runtime domain handlers
    // ============================================================

    fn handle_runtime_evaluate(&self, _webview: &mut WebView, req: DevtoolsRequest) -> DevtoolsResponse {
        use rwe_devtools_proto::runtime::{EvaluateParams, EvaluateResult, RemoteObject};
        let params: EvaluateParams = match serde_json::from_value(req.params.clone()) {
            Ok(p) => p,
            Err(e) => return Self::error_response(req.id, error_codes::INVALID_PARAMS,
                format!("Invalid params: {e}")),
        };
        let _expr = params.expression;
        // Stub: real impl pres Interpreter::run nebo eval_string.
        let result = EvaluateResult {
            result: RemoteObject {
                type_: "undefined".to_string(),
                value: None,
                description: Some("undefined".to_string()),
            },
            exception_details: None,
        };
        Self::ok_response(req.id, &result)
    }

    // ============================================================
    // Debugger domain handlers
    // ============================================================

    fn handle_debugger_set_breakpoint(&self, webview: &WebView, req: DevtoolsRequest) -> DevtoolsResponse {
        use rwe_devtools_proto::debugger::{Location, SetBreakpointParams, SetBreakpointResult};
        let params: SetBreakpointParams = match serde_json::from_value(req.params.clone()) {
            Ok(p) => p,
            Err(e) => return Self::error_response(req.id, error_codes::INVALID_PARAMS,
                format!("Invalid params: {e}")),
        };
        let interp = match webview.interpreter() {
            Some(i) => i,
            None => return Self::error_response(req.id, error_codes::INTERNAL_ERROR,
                "No interpreter".to_string()),
        };
        interp.debugger.borrow_mut().breakpoints.insert(params.line_number);
        let id = {
            let mut n = self.next_breakpoint_id.borrow_mut();
            let id = *n;
            *n += 1;
            id
        };
        let result = SetBreakpointResult {
            breakpoint_id: format!("bp-{id}"),
            actual_location: Location {
                script_id: params.script_id,
                line_number: params.line_number,
                column_number: params.column_number,
            },
        };
        Self::ok_response(req.id, &result)
    }

    fn handle_debugger_remove_breakpoint(&self, _webview: &WebView, req: DevtoolsRequest) -> DevtoolsResponse {
        use rwe_devtools_proto::debugger::RemoveBreakpointParams;
        let _params: RemoveBreakpointParams = match serde_json::from_value(req.params.clone()) {
            Ok(p) => p,
            Err(e) => return Self::error_response(req.id, error_codes::INVALID_PARAMS,
                format!("Invalid params: {e}")),
        };
        Self::ok_response_raw(req.id, serde_json::json!({}))
    }

    fn handle_debugger_resume(&self, webview: &WebView, req: DevtoolsRequest) -> DevtoolsResponse {
        if let Some(interp) = webview.interpreter() {
            interp.debugger.borrow_mut().resume();
        }
        self.push_event(DevtoolsEvent {
            method: "Debugger.resumed".to_string(),
            params: serde_json::json!({}),
        });
        Self::ok_response_raw(req.id, serde_json::json!({}))
    }

    fn handle_debugger_step_over(&self, _webview: &WebView, req: DevtoolsRequest) -> DevtoolsResponse {
        Self::ok_response_raw(req.id, serde_json::json!({}))
    }

    fn handle_debugger_step_into(&self, _webview: &WebView, req: DevtoolsRequest) -> DevtoolsResponse {
        Self::ok_response_raw(req.id, serde_json::json!({}))
    }

    fn handle_debugger_step_out(&self, _webview: &WebView, req: DevtoolsRequest) -> DevtoolsResponse {
        Self::ok_response_raw(req.id, serde_json::json!({}))
    }

    fn handle_debugger_pause(&self, _webview: &WebView, req: DevtoolsRequest) -> DevtoolsResponse {
        Self::ok_response_raw(req.id, serde_json::json!({}))
    }

    // ============================================================
    // Network domain handlers
    // ============================================================

    fn handle_network_get_response_body(&self, _webview: &WebView, req: DevtoolsRequest) -> DevtoolsResponse {
        use rwe_devtools_proto::network::{GetResponseBodyParams, GetResponseBodyResult};
        let _params: GetResponseBodyParams = match serde_json::from_value(req.params.clone()) {
            Ok(p) => p,
            Err(e) => return Self::error_response(req.id, error_codes::INVALID_PARAMS,
                format!("Invalid params: {e}")),
        };
        let result = GetResponseBodyResult {
            body: String::new(),
            base64_encoded: false,
        };
        Self::ok_response(req.id, &result)
    }

    // ============================================================
    // Performance domain handlers
    // ============================================================

    fn handle_performance_get_metrics(&self, _webview: &WebView, req: DevtoolsRequest) -> DevtoolsResponse {
        use rwe_devtools_proto::performance::{GetMetricsResult, Metric};
        let result = GetMetricsResult {
            metrics: vec![
                Metric { name: "Documents".to_string(), value: 1.0 },
            ],
        };
        Self::ok_response(req.id, &result)
    }

    // ============================================================
    // Response helpers
    // ============================================================

    fn ok_response<T: serde::Serialize>(id: u64, result: &T) -> DevtoolsResponse {
        let value = serde_json::to_value(result).unwrap_or(serde_json::Value::Null);
        DevtoolsResponse { id, result: Some(value), error: None }
    }

    fn ok_response_raw(id: u64, value: serde_json::Value) -> DevtoolsResponse {
        DevtoolsResponse { id, result: Some(value), error: None }
    }

    fn error_response(id: u64, code: i32, message: String) -> DevtoolsResponse {
        DevtoolsResponse {
            id,
            result: None,
            error: Some(DevtoolsError { code, message }),
        }
    }
}

// ============================================================
// DOM serializace helpers
// ============================================================

/// Serializuje DOM node do protocol Node typu. `depth` < 0 = unlimited,
/// 0 = jen self bez children, N = recurse N levels.
fn serialize_node(
    node: &Rc<crate::browser::dom::Node>,
    depth: i32,
    current_depth: i32,
) -> rwe_devtools_proto::dom::Node {
    use crate::browser::dom::NodeKind;
    use rwe_devtools_proto::dom::Node as ProtoNode;

    let node_id = node_id_from_ptr(node);
    let (node_type, node_name, node_value) = match &node.kind {
        NodeKind::Document => (9, "#document".to_string(), None),
        NodeKind::Element(tag) => (1, tag.to_uppercase(), None),
        NodeKind::Text(text) => (3, "#text".to_string(), Some(text.clone())),
        NodeKind::Comment(text) => (8, "#comment".to_string(), Some(text.clone())),
        NodeKind::Cdata(text) => (4, "#cdata-section".to_string(), Some(text.clone())),
        NodeKind::DocType(text) => (10, text.clone(), None),
    };

    let attrs = node.attributes.borrow();
    let mut flat = Vec::with_capacity(attrs.len() * 2);
    for (k, v) in attrs.iter() {
        flat.push(k.clone());
        flat.push(v.clone());
    }

    let children = node.children.borrow();
    let child_count = children.len() as u32;
    let serialize_children = depth < 0 || current_depth < depth;
    let children_vec = if serialize_children {
        children.iter()
            .map(|c| serialize_node(c, depth, current_depth + 1))
            .collect()
    } else {
        Vec::new()
    };

    ProtoNode {
        node_id,
        node_type,
        node_name,
        node_value,
        attributes: flat,
        children: children_vec,
        child_node_count: Some(child_count),
    }
}

/// NodeId z Rc pointer hash - stabilni dokud node zije.
/// Po reload (full DOM rebuild) id se zmeni - frontend invaliduje cache pres
/// `DOM.documentUpdated` event.
fn node_id_from_ptr(node: &Rc<crate::browser::dom::Node>) -> u64 {
    Rc::as_ptr(node) as usize as u64
}

/// Najde node v tree dle node_id (pointer hash). Walk DFS, prvni match.
fn find_node_by_id(
    root: &Rc<crate::browser::dom::Node>,
    target: u64,
) -> Option<Rc<crate::browser::dom::Node>> {
    if node_id_from_ptr(root) == target {
        return Some(Rc::clone(root));
    }
    for child in root.children.borrow().iter() {
        if let Some(found) = find_node_by_id(child, target) {
            return Some(found);
        }
    }
    None
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embed::{Engine, WebView};
    use std::sync::Arc;

    fn make_test_webview() -> WebView {
        let engine = Arc::new(Engine::new_headless());
        let mut wv = WebView::new(engine, 800, 600);
        let _ = wv.load_html(
            "<html><body><div id='a' class='x'>hello</div></body></html>",
            "",
            None,
        );
        wv
    }

    #[test]
    fn unknown_method_returns_error() {
        let mut wv = make_test_webview();
        let target = DevtoolsTarget::new();
        let req = DevtoolsRequest {
            id: 1,
            method: "Foo.bar".to_string(),
            params: serde_json::json!({}),
        };
        let resp = target.handle_request(&mut wv, req);
        assert_eq!(resp.id, 1);
        assert!(resp.result.is_none());
        let err = resp.error.expect("error expected");
        assert_eq!(err.code, error_codes::METHOD_NOT_FOUND);
    }

    #[test]
    fn dom_get_document_returns_root() {
        let mut wv = make_test_webview();
        let target = DevtoolsTarget::new();
        let req = DevtoolsRequest {
            id: 5,
            method: "DOM.getDocument".to_string(),
            params: serde_json::json!({ "depth": -1 }),
        };
        let resp = target.handle_request(&mut wv, req);
        assert_eq!(resp.id, 5);
        assert!(resp.error.is_none(), "error: {:?}", resp.error);
        let result = resp.result.expect("result expected");
        let root = result.get("root").expect("root field");
        assert_eq!(root["node_type"], 9);
        assert!(root["children"].is_array());
        assert!(!root["children"].as_array().unwrap().is_empty());
    }

    #[test]
    fn debugger_resume_emits_event() {
        let mut wv = make_test_webview();
        let target = DevtoolsTarget::new();
        let req = DevtoolsRequest {
            id: 7,
            method: "Debugger.resume".to_string(),
            params: serde_json::json!({}),
        };
        let resp = target.handle_request(&mut wv, req);
        assert_eq!(resp.id, 7);
        assert!(resp.error.is_none());
        let events = target.take_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].method, "Debugger.resumed");
        assert!(target.take_events().is_empty());
    }

    #[test]
    fn dom_set_attribute_emits_event() {
        let mut wv = make_test_webview();
        let target = DevtoolsTarget::new();
        let req = DevtoolsRequest {
            id: 1,
            method: "DOM.getDocument".to_string(),
            params: serde_json::json!({ "depth": -1 }),
        };
        let resp = target.handle_request(&mut wv, req);
        let root: rwe_devtools_proto::dom::Node = serde_json::from_value(
            resp.result.unwrap().get("root").unwrap().clone()
        ).unwrap();
        fn first_elem(n: &rwe_devtools_proto::dom::Node) -> Option<&rwe_devtools_proto::dom::Node> {
            if n.node_name == "DIV" { return Some(n); }
            for c in &n.children {
                if let Some(f) = first_elem(c) { return Some(f); }
            }
            None
        }
        let div = first_elem(&root).expect("DIV expected");

        let req = DevtoolsRequest {
            id: 2,
            method: "DOM.setAttributeValue".to_string(),
            params: serde_json::json!({
                "node_id": div.node_id,
                "name": "data-foo",
                "value": "bar",
            }),
        };
        let resp = target.handle_request(&mut wv, req);
        assert!(resp.error.is_none(), "error: {:?}", resp.error);
        let events = target.take_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].method, "DOM.attributeModified");
        assert_eq!(events[0].params["name"], "data-foo");
        assert_eq!(events[0].params["value"], "bar");
    }

    #[test]
    fn dom_get_attributes_returns_flat_list() {
        let mut wv = make_test_webview();
        let target = DevtoolsTarget::new();
        let req = DevtoolsRequest {
            id: 1,
            method: "DOM.getDocument".to_string(),
            params: serde_json::json!({ "depth": -1 }),
        };
        let resp = target.handle_request(&mut wv, req);
        let root: rwe_devtools_proto::dom::Node = serde_json::from_value(
            resp.result.unwrap().get("root").unwrap().clone()
        ).unwrap();
        fn first_elem(n: &rwe_devtools_proto::dom::Node) -> Option<&rwe_devtools_proto::dom::Node> {
            if n.node_name == "DIV" { return Some(n); }
            for c in &n.children {
                if let Some(f) = first_elem(c) { return Some(f); }
            }
            None
        }
        let div = first_elem(&root).expect("DIV");

        let req = DevtoolsRequest {
            id: 2,
            method: "DOM.getAttributes".to_string(),
            params: serde_json::json!({ "node_id": div.node_id }),
        };
        let resp = target.handle_request(&mut wv, req);
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        let attrs = result["attributes"].as_array().unwrap();
        let mut found_id = false;
        let mut found_class = false;
        let mut i = 0;
        while i + 1 < attrs.len() {
            let k = attrs[i].as_str().unwrap();
            let v = attrs[i + 1].as_str().unwrap();
            if k == "id" && v == "a" { found_id = true; }
            if k == "class" && v == "x" { found_class = true; }
            i += 2;
        }
        assert!(found_id, "id attribute missing");
        assert!(found_class, "class attribute missing");
    }

    #[test]
    fn dom_node_not_found_error() {
        let mut wv = make_test_webview();
        let target = DevtoolsTarget::new();
        let req = DevtoolsRequest {
            id: 1,
            method: "DOM.getAttributes".to_string(),
            params: serde_json::json!({ "node_id": 999999u64 }),
        };
        let resp = target.handle_request(&mut wv, req);
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, error_codes::NODE_NOT_FOUND);
    }

    #[test]
    fn invalid_params_returns_error() {
        let mut wv = make_test_webview();
        let target = DevtoolsTarget::new();
        let req = DevtoolsRequest {
            id: 1,
            method: "DOM.getAttributes".to_string(),
            params: serde_json::json!({ "wrong_field": 1 }),
        };
        let resp = target.handle_request(&mut wv, req);
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, error_codes::INVALID_PARAMS);
    }
}
