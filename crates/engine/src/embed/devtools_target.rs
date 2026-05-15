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

    fn handle_dom_query_selector(&self, webview: &WebView, req: DevtoolsRequest) -> DevtoolsResponse {
        use rwe_devtools_proto::dom::{QuerySelectorParams, QuerySelectorResult};
        let params: QuerySelectorParams = match serde_json::from_value(req.params.clone()) {
            Ok(p) => p,
            Err(e) => return Self::error_response(req.id, error_codes::INVALID_PARAMS,
                format!("Invalid params: {e}")),
        };
        let doc = match webview.document() {
            Some(d) => d,
            None => return Self::error_response(req.id, error_codes::INTERNAL_ERROR,
                "No document loaded".to_string()),
        };
        let root = match find_node_by_id(&doc.root, params.node_id) {
            Some(n) => n,
            None => return Self::error_response(req.id, error_codes::NODE_NOT_FOUND,
                format!("Node {} not found", params.node_id)),
        };
        let selectors = crate::browser::css_parser::parse_selectors(&params.selector);
        let mut found: Option<u64> = None;
        walk_dfs(&root, &mut |node| {
            if found.is_some() { return; }
            for sel in &selectors {
                if crate::browser::cascade::matches_selector(node, sel) {
                    found = Some(node_id_from_ptr(node));
                    return;
                }
            }
        });
        let result = QuerySelectorResult { node_id: found };
        Self::ok_response(req.id, &result)
    }

    fn handle_dom_query_selector_all(&self, webview: &WebView, req: DevtoolsRequest) -> DevtoolsResponse {
        use rwe_devtools_proto::dom::{QuerySelectorAllParams, QuerySelectorAllResult};
        let params: QuerySelectorAllParams = match serde_json::from_value(req.params.clone()) {
            Ok(p) => p,
            Err(e) => return Self::error_response(req.id, error_codes::INVALID_PARAMS,
                format!("Invalid params: {e}")),
        };
        let doc = match webview.document() {
            Some(d) => d,
            None => return Self::error_response(req.id, error_codes::INTERNAL_ERROR,
                "No document loaded".to_string()),
        };
        let root = match find_node_by_id(&doc.root, params.node_id) {
            Some(n) => n,
            None => return Self::error_response(req.id, error_codes::NODE_NOT_FOUND,
                format!("Node {} not found", params.node_id)),
        };
        let selectors = crate::browser::css_parser::parse_selectors(&params.selector);
        let mut ids: Vec<u64> = Vec::new();
        walk_dfs(&root, &mut |node| {
            for sel in &selectors {
                if crate::browser::cascade::matches_selector(node, sel) {
                    ids.push(node_id_from_ptr(node));
                    return;
                }
            }
        });
        let result = QuerySelectorAllResult { node_ids: ids };
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

    fn handle_css_get_matched_styles(&self, webview: &WebView, req: DevtoolsRequest) -> DevtoolsResponse {
        use rwe_devtools_proto::css::{
            CSSProperty, CSSRule, CSSStyle, GetMatchedStylesForNodeParams,
            GetMatchedStylesForNodeResult, RuleMatch,
        };
        let params: GetMatchedStylesForNodeParams = match serde_json::from_value(req.params.clone()) {
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

        // Inline style atribut -> CSSStyle.
        let inline_style = node.attr("style").and_then(|s| {
            if s.is_empty() { return None; }
            let mut props = Vec::new();
            for pair in s.split(';') {
                if let Some(idx) = pair.find(':') {
                    let name = pair[..idx].trim().to_string();
                    let value = pair[idx+1..].trim().trim_end_matches("!important").trim().to_string();
                    let important = pair[idx+1..].contains("!important");
                    if !name.is_empty() {
                        props.push(CSSProperty { name, value, important, disabled: false });
                    }
                }
            }
            if props.is_empty() { None } else { Some(CSSStyle { properties: props }) }
        });

        // Walk stylesheets + match selectors proti tomuto node.
        let mut matched_rules: Vec<RuleMatch> = Vec::new();
        for sheet in webview.stylesheets() {
            for rule in &sheet.rules {
                let mut matching_indices: Vec<u32> = Vec::new();
                let mut selectors_str: Vec<String> = Vec::with_capacity(rule.selectors.len());
                for (i, sel) in rule.selectors.iter().enumerate() {
                    selectors_str.push(format_selector(sel));
                    if crate::browser::cascade::matches_selector(&node, sel) {
                        matching_indices.push(i as u32);
                    }
                }
                if matching_indices.is_empty() { continue; }
                let props: Vec<CSSProperty> = rule.declarations.iter().map(|d| CSSProperty {
                    name: d.property.clone(),
                    value: d.value.clone(),
                    important: d.important,
                    disabled: false,
                }).collect();
                matched_rules.push(RuleMatch {
                    rule: CSSRule {
                        selector_list: selectors_str,
                        style: CSSStyle { properties: props },
                        origin: Some("regular".to_string()),
                    },
                    matching_selectors: matching_indices,
                });
            }
        }

        let result = GetMatchedStylesForNodeResult {
            inline_style,
            matched_rules,
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

    fn handle_runtime_evaluate(&self, webview: &mut WebView, req: DevtoolsRequest) -> DevtoolsResponse {
        use rwe_devtools_proto::runtime::{EvaluateParams, EvaluateResult, ExceptionDetails, RemoteObject};
        let params: EvaluateParams = match serde_json::from_value(req.params.clone()) {
            Ok(p) => p,
            Err(e) => return Self::error_response(req.id, error_codes::INVALID_PARAMS,
                format!("Invalid params: {e}")),
        };
        let expr_src = params.expression;
        let interp = match webview.interpreter_mut() {
            Some(i) => i,
            None => return Self::error_response(req.id, error_codes::INTERNAL_ERROR,
                "No interpreter".to_string()),
        };
        let lexer = match crate::lexer::base::Lexer::parse_str(&expr_src, "<devtools-eval>") {
            Ok(l) => l,
            Err(e) => return Self::ok_response(req.id, &EvaluateResult {
                result: RemoteObject { type_: "object".into(), value: None,
                    description: Some(format!("SyntaxError: {e}")) },
                exception_details: Some(ExceptionDetails {
                    text: format!("SyntaxError: {e}"),
                    line_number: None, column_number: None, stack_trace: None,
                }),
            }),
        };
        let tokens: Vec<_> = lexer.tokens.iter().filter(|t| !matches!(t.kind,
            crate::tokens::TokenKind::Whitespace | crate::tokens::TokenKind::Newline
            | crate::tokens::TokenKind::CommentLine(_) | crate::tokens::TokenKind::CommentBlock(_)
        )).cloned().collect();
        let mut parser = crate::parser::Parser::new(tokens);
        let program = match parser.parse() {
            Ok(p) => p,
            Err(e) => return Self::ok_response(req.id, &EvaluateResult {
                result: RemoteObject { type_: "object".into(), value: None,
                    description: Some(format!("ParseError: {:?}", e)) },
                exception_details: Some(ExceptionDetails {
                    text: format!("ParseError: {:?}", e),
                    line_number: None, column_number: None, stack_trace: None,
                }),
            }),
        };
        let env = Rc::clone(&interp.global);
        let mut last_val = crate::interpreter::JsValue::Undefined;
        for stmt in &program.body {
            if let crate::ast::Stmt::Expr(e) = stmt {
                match interp.eval(e, &env) {
                    Ok(v) => last_val = v,
                    Err(err) => return Self::ok_response(req.id, &EvaluateResult {
                        result: RemoteObject { type_: "object".into(), value: None,
                            description: Some(format!("RuntimeError: {:?}", err)) },
                        exception_details: Some(ExceptionDetails {
                            text: format!("RuntimeError: {:?}", err),
                            line_number: None, column_number: None, stack_trace: None,
                        }),
                    }),
                }
            }
        }
        let (type_, value, description) = js_value_to_remote(&last_val);
        let result = EvaluateResult {
            result: RemoteObject { type_, value, description: Some(description) },
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

/// Selector -> string (cosmetic - vrati to v CDP rule.selector_list).
/// Naive format: join SimpleSelectors s descendant " ".
fn format_selector(sel: &crate::browser::css_parser::Selector) -> String {
    sel.parts.iter().map(|s| {
        let mut out = String::new();
        if let Some(t) = &s.tag { out.push_str(t); }
        if let Some(id) = &s.id { out.push('#'); out.push_str(id); }
        for c in &s.classes { out.push('.'); out.push_str(c); }
        for pc in &s.pseudo_classes { out.push(':'); out.push_str(pc); }
        if let Some(pe) = &s.pseudo_element { out.push_str("::"); out.push_str(pe); }
        if out.is_empty() { out.push('*'); }
        out
    }).collect::<Vec<_>>().join(" ")
}

/// DFS walk subtree + apply visitor pres kazdy element node.
/// Visitor mutate moze hold state nebo early-exit (check `found`).
fn walk_dfs<F: FnMut(&Rc<crate::browser::dom::Node>)>(
    root: &Rc<crate::browser::dom::Node>,
    visitor: &mut F,
) {
    use crate::browser::dom::NodeKind;
    if matches!(root.kind, NodeKind::Element(_)) {
        visitor(root);
    }
    for child in root.children.borrow().iter() {
        walk_dfs(child, visitor);
    }
}

/// JsValue -> (type, value, description) pro CDP Runtime.evaluate RemoteObject.
fn js_value_to_remote(val: &crate::interpreter::JsValue) -> (String, Option<serde_json::Value>, String) {
    use crate::interpreter::JsValue;
    match val {
        JsValue::Undefined => ("undefined".into(), None, "undefined".into()),
        JsValue::Null => ("object".into(), Some(serde_json::Value::Null), "null".into()),
        JsValue::Bool(b) => ("boolean".into(), Some(serde_json::Value::Bool(*b)), b.to_string()),
        JsValue::Number(n) => {
            let v = serde_json::Number::from_f64(*n)
                .map(serde_json::Value::Number).unwrap_or(serde_json::Value::Null);
            ("number".into(), Some(v), n.to_string())
        }
        JsValue::Str(s) => ("string".into(),
            Some(serde_json::Value::String(s.clone())), s.clone()),
        JsValue::BigInt(b) => ("bigint".into(), None, format!("{}n", b)),
        JsValue::Function(_) => ("function".into(), None, "[Function]".into()),
        JsValue::Object(_) => ("object".into(), None, "[Object]".into()),
        JsValue::Array(a) => ("object".into(), None, format!("Array({})", a.borrow().len())),
        JsValue::DomNode(n) => {
            let desc = match &n.kind {
                crate::browser::dom::NodeKind::Element(tag) => format!("<{}>", tag),
                _ => format!("[{:?}]", n.kind),
            };
            ("object".into(), None, desc)
        }
        _ => ("object".into(), None, format!("{:?}", val)),
    }
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
    fn dom_query_selector_returns_match() {
        let engine = Arc::new(Engine::new_headless());
        let mut wv = WebView::new(engine, 800, 600);
        let _ = wv.load_html(
            "<html><body><div id='a'></div><p class='x'></p><p class='x'></p></body></html>",
            "", None);
        let target = DevtoolsTarget::new();
        // First get document for root node_id.
        let resp = target.handle_request(&mut wv, DevtoolsRequest {
            id: 1, method: "DOM.getDocument".into(),
            params: serde_json::json!({ "depth": -1 }),
        });
        let root_id = resp.result.unwrap()["root"]["node_id"].as_u64().unwrap();

        // querySelector finds first .x.
        let resp = target.handle_request(&mut wv, DevtoolsRequest {
            id: 2, method: "DOM.querySelector".into(),
            params: serde_json::json!({ "node_id": root_id, "selector": ".x" }),
        });
        assert!(resp.error.is_none(), "qs error: {:?}", resp.error);
        let node_id = resp.result.unwrap()["node_id"].as_u64();
        assert!(node_id.is_some(), "querySelector should match first .x");

        // querySelectorAll vraci 2 matches.
        let resp = target.handle_request(&mut wv, DevtoolsRequest {
            id: 3, method: "DOM.querySelectorAll".into(),
            params: serde_json::json!({ "node_id": root_id, "selector": ".x" }),
        });
        assert!(resp.error.is_none());
        let ids = resp.result.unwrap()["node_ids"].as_array().unwrap().len();
        assert_eq!(ids, 2, "querySelectorAll should return 2");
    }

    #[test]
    fn css_get_matched_styles_real_walk() {
        let engine = Arc::new(Engine::new_headless());
        let mut wv = WebView::new(engine, 800, 600);
        let _ = wv.load_html(
            "<html><body><div class='box' style='padding: 5px'></div></body></html>",
            ".box { color: red; background: blue; }", None);
        let target = DevtoolsTarget::new();
        // Find div node_id.
        let resp = target.handle_request(&mut wv, DevtoolsRequest {
            id: 1, method: "DOM.getDocument".into(),
            params: serde_json::json!({ "depth": -1 }),
        });
        let root: rwe_devtools_proto::dom::Node = serde_json::from_value(
            resp.result.unwrap()["root"].clone()
        ).unwrap();
        fn find_div(n: &rwe_devtools_proto::dom::Node) -> Option<&rwe_devtools_proto::dom::Node> {
            if n.node_name == "DIV" { return Some(n); }
            for c in &n.children { if let Some(f) = find_div(c) { return Some(f); } }
            None
        }
        let div = find_div(&root).expect("DIV");

        let resp = target.handle_request(&mut wv, DevtoolsRequest {
            id: 2, method: "CSS.getMatchedStylesForNode".into(),
            params: serde_json::json!({ "node_id": div.node_id }),
        });
        assert!(resp.error.is_none(), "css err: {:?}", resp.error);
        let result = resp.result.unwrap();

        // Inline style (padding: 5px).
        let inline = result.get("inline_style").expect("inline_style key");
        assert!(!inline.is_null(), "inline_style should be set (padding)");
        let inline_props = inline["properties"].as_array().unwrap();
        assert!(inline_props.iter().any(|p| p["name"] == "padding"),
            "inline must have padding");

        // Matched rules - .box { color, background }.
        let rules = result["matched_rules"].as_array().unwrap();
        assert!(!rules.is_empty(), "matched_rules should not be empty");
        let first_rule = &rules[0];
        let props = first_rule["rule"]["style"]["properties"].as_array().unwrap();
        let prop_names: Vec<&str> = props.iter().map(|p| p["name"].as_str().unwrap()).collect();
        assert!(prop_names.contains(&"color"), "expected color in matched rule");
        assert!(prop_names.contains(&"background"), "expected background in matched rule");
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
