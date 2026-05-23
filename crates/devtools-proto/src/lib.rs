//! RustWebEngine DevTools Protocol (skeleton)
//!
//! Inspirace Chrome DevTools Protocol (CDP) - JSON-RPC mezi devtools
//! frontendem a backendem. Domains: DOM / CSS / Runtime / Debugger /
//! Network / Console / Performance.
//!
//! Transport: zatim mpsc channel pres `DevtoolsRequest` / `DevtoolsResponse`
//! / `DevtoolsEvent`. Pozdeji bude JSON wire format pro multi-process
//! / cross-thread variants.
//!
//! D1 (skeleton) - typy a serde derive. Real handlery v devtools_target
//! adapter modulu engine crate (D2). Frontend (D3) konzumuje typy pres
//! `window.cdp.send(method, params)` JS API.

use serde::{Deserialize, Serialize};

// ============================================================
// Top-level envelope - Request / Response / Event
// ============================================================

/// Request frontendu na backend. `id` korreluje s Response (same id).
/// `method` = "Domain.method" string (e.g. "DOM.getDocument").
/// `params` = method-specific payload (untagged enum dispatch dle method).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevtoolsRequest {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// Odpoved backendu. `id` = request id, `result` nebo `error`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevtoolsResponse {
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<DevtoolsError>,
}

/// Server-side broadcast event (no request). Frontend listener handluje
/// dle `method` string.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevtoolsEvent {
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// Error payload pri request failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevtoolsError {
    pub code: i32,
    pub message: String,
}

// ============================================================
// Domain: DOM
// ============================================================

pub mod dom {
    use serde::{Deserialize, Serialize};

    /// DOM.getDocument - vrati root document node.
    /// Method string: "DOM.getDocument"
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct GetDocumentParams {
        #[serde(default)]
        pub depth: Option<i32>,
        #[serde(default)]
        pub pierce: Option<bool>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct GetDocumentResult {
        pub root: Node,
    }

    /// DOM.querySelector - find first matching element.
    /// Method string: "DOM.querySelector"
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct QuerySelectorParams {
        pub node_id: NodeId,
        pub selector: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct QuerySelectorResult {
        pub node_id: Option<NodeId>,
    }

    /// DOM.querySelectorAll - find all matching elements.
    /// Method string: "DOM.querySelectorAll"
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct QuerySelectorAllParams {
        pub node_id: NodeId,
        pub selector: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct QuerySelectorAllResult {
        pub node_ids: Vec<NodeId>,
    }

    /// DOM.getAttributes - vrati attribute list pro node.
    /// Method string: "DOM.getAttributes"
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct GetAttributesParams {
        pub node_id: NodeId,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct GetAttributesResult {
        /// Flat list [name, value, name, value, ...] - CDP convention.
        pub attributes: Vec<String>,
    }

    /// DOM.setAttributeValue - set/update jeden atribut.
    /// Method string: "DOM.setAttributeValue"
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SetAttributeValueParams {
        pub node_id: NodeId,
        pub name: String,
        pub value: String,
    }

    /// DOM.removeAttribute - smaze atribut.
    /// Method string: "DOM.removeAttribute"
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RemoveAttributeParams {
        pub node_id: NodeId,
        pub name: String,
    }

    /// Event: DOM.documentUpdated - pri full DOM rebuild (page nav).
    /// Method string: "DOM.documentUpdated"

    /// Event: DOM.attributeModified - attribute change broadcast.
    /// Method string: "DOM.attributeModified"
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct AttributeModifiedEvent {
        pub node_id: NodeId,
        pub name: String,
        pub value: String,
    }

    /// DOM node identifier (pointer hash or sequential ID).
    pub type NodeId = u64;

    /// Serializovany DOM node. Type 1 = element, 3 = text, 8 = comment,
    /// 9 = document, 10 = doctype.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Node {
        pub node_id: NodeId,
        pub node_type: u8,
        pub node_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub node_value: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub attributes: Vec<String>, // [name, value, name, value, ...]
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub children: Vec<Node>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub child_node_count: Option<u32>,
    }
}

// ============================================================
// Domain: CSS
// ============================================================

pub mod css {
    use serde::{Deserialize, Serialize};
    use super::dom::NodeId;

    /// CSS.getMatchedStylesForNode - vrati matched rules + inline + inherited.
    /// Method string: "CSS.getMatchedStylesForNode"
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct GetMatchedStylesForNodeParams {
        pub node_id: NodeId,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct GetMatchedStylesForNodeResult {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub inline_style: Option<CSSStyle>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub matched_rules: Vec<RuleMatch>,
    }

    /// CSS.getComputedStyleForNode - vrati computed (cascaded) style.
    /// Method string: "CSS.getComputedStyleForNode"
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct GetComputedStyleForNodeParams {
        pub node_id: NodeId,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct GetComputedStyleForNodeResult {
        pub computed_style: Vec<CSSProperty>,
    }

    /// CSS.setPropertyText - edit jednu property na given rule.
    /// Method string: "CSS.setPropertyText"
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SetPropertyTextParams {
        pub node_id: NodeId,
        pub property: String,
        pub value: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RuleMatch {
        pub rule: CSSRule,
        /// Indexy selectors v rule.selector_list ktere match'nuly.
        pub matching_selectors: Vec<u32>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CSSRule {
        pub selector_list: Vec<String>,
        pub style: CSSStyle,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub origin: Option<String>, // "user-agent" / "regular" / "inline"
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CSSStyle {
        pub properties: Vec<CSSProperty>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CSSProperty {
        pub name: String,
        pub value: String,
        #[serde(default)]
        pub important: bool,
        #[serde(default)]
        pub disabled: bool,
    }
}

// ============================================================
// Domain: Runtime (JS evaluation)
// ============================================================

pub mod runtime {
    use serde::{Deserialize, Serialize};

    /// Runtime.evaluate - eval JS expression v page context.
    /// Method string: "Runtime.evaluate"
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct EvaluateParams {
        pub expression: String,
        #[serde(default)]
        pub return_by_value: bool,
        #[serde(default)]
        pub silent: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct EvaluateResult {
        pub result: RemoteObject,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub exception_details: Option<ExceptionDetails>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RemoteObject {
        /// "object" / "function" / "string" / "number" / "boolean" / "undefined" / "symbol" / "bigint"
        #[serde(rename = "type")]
        pub type_: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub value: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub description: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ExceptionDetails {
        pub text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub line_number: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub column_number: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub stack_trace: Option<String>,
    }

    /// Event: Runtime.consoleAPICalled - console.log/warn/error/info.
    /// Method string: "Runtime.consoleAPICalled"
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ConsoleAPICalledEvent {
        /// "log" / "info" / "warn" / "error" / "debug" / "trace"
        #[serde(rename = "type")]
        pub type_: String,
        pub args: Vec<RemoteObject>,
        pub timestamp: f64,
    }
}

// ============================================================
// Domain: Debugger
// ============================================================

pub mod debugger {
    use serde::{Deserialize, Serialize};

    /// Debugger.setBreakpoint - postavi breakpoint na given line.
    /// Method string: "Debugger.setBreakpoint"
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SetBreakpointParams {
        pub script_id: String,
        pub line_number: u32,
        #[serde(default)]
        pub column_number: Option<u32>,
        #[serde(default)]
        pub condition: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SetBreakpointResult {
        pub breakpoint_id: String,
        pub actual_location: Location,
    }

    /// Debugger.removeBreakpoint - smaze breakpoint dle ID.
    /// Method string: "Debugger.removeBreakpoint"
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RemoveBreakpointParams {
        pub breakpoint_id: String,
    }

    /// Debugger.resume - continue z paused stavu.
    /// Method string: "Debugger.resume"

    /// Debugger.stepOver - step na dalsi statement, neskoci do funkce.
    /// Method string: "Debugger.stepOver"

    /// Debugger.stepInto - step do call.
    /// Method string: "Debugger.stepInto"

    /// Debugger.stepOut - step pryc z aktualni funkce.
    /// Method string: "Debugger.stepOut"

    /// Debugger.pause - manual pause na nasledujici statement.
    /// Method string: "Debugger.pause"

    /// Debugger.getScriptSource - vrat source code daneho scriptId.
    /// Method string: "Debugger.getScriptSource"
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct GetScriptSourceParams {
        pub script_id: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct GetScriptSourceResult {
        pub script_source: String,
    }


    /// Event: Debugger.paused - VM hit breakpoint nebo pause.
    /// Method string: "Debugger.paused"
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PausedEvent {
        pub call_frames: Vec<CallFrame>,
        /// "instrumentation" / "exception" / "assert" / "ambiguous" / "break"
        pub reason: String,
    }

    /// Event: Debugger.resumed - po resume.
    /// Method string: "Debugger.resumed"

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Location {
        pub script_id: String,
        pub line_number: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub column_number: Option<u32>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CallFrame {
        pub call_frame_id: String,
        pub function_name: String,
        pub location: Location,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub scope_chain: Vec<Scope>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Scope {
        /// "global" / "local" / "with" / "closure" / "catch" / "block" / "script" / "eval" / "module"
        #[serde(rename = "type")]
        pub type_: String,
        pub variables: Vec<(String, String)>, // (name, value-string)
    }
}

// ============================================================
// Domain: Network
// ============================================================

pub mod network {
    use serde::{Deserialize, Serialize};

    /// Event: Network.requestWillBeSent - HTTP request initiated.
    /// Method string: "Network.requestWillBeSent"
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RequestWillBeSentEvent {
        pub request_id: String,
        pub url: String,
        pub method: String,
        pub timestamp: f64,
        /// "Document" / "Stylesheet" / "Image" / "Media" / "Font" / "Script" /
        /// "XHR" / "Fetch" / "WebSocket" / "Other"
        pub resource_type: String,
    }

    /// Event: Network.responseReceived - HTTP response headers prijato.
    /// Method string: "Network.responseReceived"
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ResponseReceivedEvent {
        pub request_id: String,
        pub status: u32,
        pub status_text: String,
        pub mime_type: String,
        pub timestamp: f64,
    }

    /// Event: Network.loadingFinished - body kompletni.
    /// Method string: "Network.loadingFinished"
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct LoadingFinishedEvent {
        pub request_id: String,
        pub encoded_data_length: u64,
        pub timestamp: f64,
    }

    /// Network.getResponseBody - fetch body pro request.
    /// Method string: "Network.getResponseBody"
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct GetResponseBodyParams {
        pub request_id: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct GetResponseBodyResult {
        pub body: String,
        #[serde(default)]
        pub base64_encoded: bool,
    }
}

// ============================================================
// Domain: Performance
// ============================================================

pub mod performance {
    use serde::{Deserialize, Serialize};

    /// Performance.getMetrics - vrati frame timing metrics.
    /// Method string: "Performance.getMetrics"
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct GetMetricsResult {
        pub metrics: Vec<Metric>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Metric {
        pub name: String,
        pub value: f64,
    }
}

// ============================================================
// Top-level method registry - dispatcher hint pro D2 (target adapter)
// ============================================================

/// Vsechny known method strings v jednom enum - pomahaji match'ovat
/// v DevtoolsTarget::handle_request. Stringify pres `to_method_str`.
///
/// NOT exhaustive - jen ty co aktualne planujeme. Frontend muze poslat
/// neznamy method -> DevtoolsError s code -32601 (method not found).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    DomGetDocument,
    DomQuerySelector,
    DomQuerySelectorAll,
    DomGetAttributes,
    DomSetAttributeValue,
    DomRemoveAttribute,
    CssGetMatchedStylesForNode,
    CssGetComputedStyleForNode,
    CssSetPropertyText,
    RuntimeEvaluate,
    DebuggerSetBreakpoint,
    DebuggerRemoveBreakpoint,
    DebuggerResume,
    DebuggerStepOver,
    DebuggerStepInto,
    DebuggerStepOut,
    DebuggerPause,
    DebuggerGetScriptSource,
    NetworkGetResponseBody,
    PerformanceGetMetrics,
    OverlayEnable,
    OverlayDisable,
    OverlayHighlightNode,
    OverlayHideHighlight,
    OverlaySetInspectMode,
    DomGetBoxModel,
    DomGetNodeForLocation,
}

impl Method {
    pub fn to_method_str(self) -> &'static str {
        match self {
            Method::DomGetDocument => "DOM.getDocument",
            Method::DomQuerySelector => "DOM.querySelector",
            Method::DomQuerySelectorAll => "DOM.querySelectorAll",
            Method::DomGetAttributes => "DOM.getAttributes",
            Method::DomSetAttributeValue => "DOM.setAttributeValue",
            Method::DomRemoveAttribute => "DOM.removeAttribute",
            Method::CssGetMatchedStylesForNode => "CSS.getMatchedStylesForNode",
            Method::CssGetComputedStyleForNode => "CSS.getComputedStyleForNode",
            Method::CssSetPropertyText => "CSS.setPropertyText",
            Method::RuntimeEvaluate => "Runtime.evaluate",
            Method::DebuggerSetBreakpoint => "Debugger.setBreakpoint",
            Method::DebuggerRemoveBreakpoint => "Debugger.removeBreakpoint",
            Method::DebuggerResume => "Debugger.resume",
            Method::DebuggerStepOver => "Debugger.stepOver",
            Method::DebuggerStepInto => "Debugger.stepInto",
            Method::DebuggerStepOut => "Debugger.stepOut",
            Method::DebuggerPause => "Debugger.pause",
            Method::DebuggerGetScriptSource => "Debugger.getScriptSource",
            Method::NetworkGetResponseBody => "Network.getResponseBody",
            Method::PerformanceGetMetrics => "Performance.getMetrics",
            Method::OverlayEnable => "Overlay.enable",
            Method::OverlayDisable => "Overlay.disable",
            Method::OverlayHighlightNode => "Overlay.highlightNode",
            Method::OverlayHideHighlight => "Overlay.hideHighlight",
            Method::OverlaySetInspectMode => "Overlay.setInspectMode",
            Method::DomGetBoxModel => "DOM.getBoxModel",
            Method::DomGetNodeForLocation => "DOM.getNodeForLocation",
        }
    }

    pub fn from_method_str(s: &str) -> Option<Self> {
        Some(match s {
            "DOM.getDocument" => Method::DomGetDocument,
            "DOM.querySelector" => Method::DomQuerySelector,
            "DOM.querySelectorAll" => Method::DomQuerySelectorAll,
            "DOM.getAttributes" => Method::DomGetAttributes,
            "DOM.setAttributeValue" => Method::DomSetAttributeValue,
            "DOM.removeAttribute" => Method::DomRemoveAttribute,
            "CSS.getMatchedStylesForNode" => Method::CssGetMatchedStylesForNode,
            "CSS.getComputedStyleForNode" => Method::CssGetComputedStyleForNode,
            "CSS.setPropertyText" => Method::CssSetPropertyText,
            "Runtime.evaluate" => Method::RuntimeEvaluate,
            "Debugger.setBreakpoint" => Method::DebuggerSetBreakpoint,
            "Debugger.removeBreakpoint" => Method::DebuggerRemoveBreakpoint,
            "Debugger.resume" => Method::DebuggerResume,
            "Debugger.stepOver" => Method::DebuggerStepOver,
            "Debugger.stepInto" => Method::DebuggerStepInto,
            "Debugger.stepOut" => Method::DebuggerStepOut,
            "Debugger.pause" => Method::DebuggerPause,
            "Debugger.getScriptSource" => Method::DebuggerGetScriptSource,
            "Network.getResponseBody" => Method::NetworkGetResponseBody,
            "Performance.getMetrics" => Method::PerformanceGetMetrics,
            "Overlay.enable" => Method::OverlayEnable,
            "Overlay.disable" => Method::OverlayDisable,
            "Overlay.highlightNode" => Method::OverlayHighlightNode,
            "Overlay.hideHighlight" => Method::OverlayHideHighlight,
            "Overlay.setInspectMode" => Method::OverlaySetInspectMode,
            "DOM.getBoxModel" => Method::DomGetBoxModel,
            "DOM.getNodeForLocation" => Method::DomGetNodeForLocation,
            _ => return None,
        })
    }
}

// ============================================================
// Error codes (JSON-RPC inspired)
// ============================================================

pub mod error_codes {
    /// Method neexistuje nebo target neumi.
    pub const METHOD_NOT_FOUND: i32 = -32601;
    /// Spatne typovane params.
    pub const INVALID_PARAMS: i32 = -32602;
    /// Internal target error (panic catch, missing webview, ...).
    pub const INTERNAL_ERROR: i32 = -32603;
    /// Node ID neexistuje (DOM/CSS metody).
    pub const NODE_NOT_FOUND: i32 = -32000;
    /// JS evaluation throw - exception_details ma detail.
    pub const EVAL_EXCEPTION: i32 = -32001;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_roundtrip() {
        let req = DevtoolsRequest {
            id: 42,
            method: "DOM.getDocument".to_string(),
            params: serde_json::json!({ "depth": -1 }),
        };
        let s = serde_json::to_string(&req).unwrap();
        let parsed: DevtoolsRequest = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed.id, 42);
        assert_eq!(parsed.method, "DOM.getDocument");
        assert_eq!(parsed.params["depth"], -1);
    }

    #[test]
    fn response_with_result() {
        let resp = DevtoolsResponse {
            id: 1,
            result: Some(serde_json::json!({ "ok": true })),
            error: None,
        };
        let s = serde_json::to_string(&resp).unwrap();
        // error not present in JSON (skip_serializing_if Option::is_none)
        assert!(!s.contains("error"));
        let parsed: DevtoolsResponse = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed.id, 1);
        assert!(parsed.result.is_some());
    }

    #[test]
    fn response_with_error() {
        let resp = DevtoolsResponse {
            id: 5,
            result: None,
            error: Some(DevtoolsError {
                code: error_codes::METHOD_NOT_FOUND,
                message: "Unknown method".to_string(),
            }),
        };
        let s = serde_json::to_string(&resp).unwrap();
        assert!(!s.contains("result"));
        assert!(s.contains("Unknown method"));
    }

    #[test]
    fn event_serialization() {
        let evt = DevtoolsEvent {
            method: "Network.requestWillBeSent".to_string(),
            params: serde_json::json!({
                "requestId": "1",
                "url": "https://example.com",
                "method": "GET",
            }),
        };
        let s = serde_json::to_string(&evt).unwrap();
        assert!(s.contains("Network.requestWillBeSent"));
        assert!(s.contains("example.com"));
    }

    #[test]
    fn method_roundtrip() {
        let m = Method::DomGetDocument;
        assert_eq!(m.to_method_str(), "DOM.getDocument");
        assert_eq!(Method::from_method_str("DOM.getDocument"), Some(m));
        assert_eq!(Method::from_method_str("Foo.bar"), None);
    }

    #[test]
    fn all_methods_roundtrip() {
        // Vsechny variants musi mit symetricky from <-> to.
        let methods = [
            Method::DomGetDocument,
            Method::DomQuerySelector,
            Method::DomQuerySelectorAll,
            Method::DomGetAttributes,
            Method::DomSetAttributeValue,
            Method::DomRemoveAttribute,
            Method::CssGetMatchedStylesForNode,
            Method::CssGetComputedStyleForNode,
            Method::CssSetPropertyText,
            Method::RuntimeEvaluate,
            Method::DebuggerSetBreakpoint,
            Method::DebuggerRemoveBreakpoint,
            Method::DebuggerResume,
            Method::DebuggerStepOver,
            Method::DebuggerStepInto,
            Method::DebuggerStepOut,
            Method::DebuggerPause,
            Method::NetworkGetResponseBody,
            Method::PerformanceGetMetrics,
        ];
        for m in methods {
            let s = m.to_method_str();
            assert_eq!(Method::from_method_str(s), Some(m), "method {:?} -> {:?} roundtrip", m, s);
        }
    }

    #[test]
    fn dom_node_roundtrip() {
        let node = dom::Node {
            node_id: 1,
            node_type: 1,
            node_name: "DIV".to_string(),
            node_value: None,
            attributes: vec!["class".to_string(), "foo".to_string()],
            children: vec![dom::Node {
                node_id: 2,
                node_type: 3,
                node_name: "#text".to_string(),
                node_value: Some("hello".to_string()),
                attributes: vec![],
                children: vec![],
                child_node_count: None,
            }],
            child_node_count: Some(1),
        };
        let s = serde_json::to_string(&node).unwrap();
        let parsed: dom::Node = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed.node_name, "DIV");
        assert_eq!(parsed.children.len(), 1);
        assert_eq!(parsed.children[0].node_value.as_deref(), Some("hello"));
    }

    #[test]
    fn evaluate_params_roundtrip() {
        let params = runtime::EvaluateParams {
            expression: "1 + 2".to_string(),
            return_by_value: true,
            silent: false,
        };
        let s = serde_json::to_string(&params).unwrap();
        let parsed: runtime::EvaluateParams = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed.expression, "1 + 2");
        assert!(parsed.return_by_value);
    }
}
