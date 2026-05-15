//! eval_member + get_prop - member access (obj.prop, obj[key]) evaluation.

use super::*;

impl Interpreter {
    pub(super) fn eval_member(&mut self, object: &Expr, prop: &MemberProp, optional: bool, env: &Rc<RefCell<Environment>>) -> EvalResult {
        let obj = self.eval(object, env)?;
        if optional && matches!(obj, JsValue::Null | JsValue::Undefined) {
            return Ok(JsValue::Undefined);
        }
        let key = self.resolve_prop_key(prop, env)?;

        // Proxy trap 'get': handler.get(target, key, receiver)
        if let JsValue::Object(ref o) = obj {
            let has_handler = o.borrow().props.contains_key("__proxy_handler__");
            if has_handler && !key.starts_with("__") {
                let handler = o.borrow().props.get("__proxy_handler__").cloned()
                    .unwrap_or(JsValue::Undefined);
                let target = o.borrow().props.get("__proxy_target__").cloned()
                    .unwrap_or(JsValue::Undefined);
                if let JsValue::Object(h) = &handler {
                    let trap = h.borrow().props.get("get").cloned();
                    if let Some(trap_fn) = trap {
                        return self.call_function(
                            trap_fn,
                            vec![target, JsValue::Str(key.clone()), obj.clone()],
                            None,
                        );
                    }
                }
            }
        }

        // Getter podpora: kdyz objekt ma `__get_key__` vlastnost (funkci), zavolej ji
        if let JsValue::Object(ref o) = obj {
            let getter_key = format!("__get_{key}__");
            let getter_fn = o.borrow().props.get(&getter_key).cloned();
            if let Some(getter) = getter_fn {
                return self.call_function(getter, vec![], Some(obj.clone()));
            }
        }

        self.get_prop(&obj, &key)
    }

    pub(super) fn get_prop(&self, obj: &JsValue, key: &str) -> EvalResult {
        match obj {
            // Staticke metody tridy: ClassName.staticMethod()
            JsValue::Function(JsFunc::Class { statics, getters, env, super_val, .. }) => {
                for s in statics {
                    if s.name == key {
                        let senv = Environment::new_child(env);
                        if let Some(sv) = super_val {
                            senv.borrow_mut().define("__super_class__", (**sv).clone());
                        }
                        return Ok(JsValue::Function(JsFunc::User {
                            name: Some(s.name.clone()),
                            params: s.params.clone(),
                            body: FuncBody::Stmts(s.body.clone()),
                            env: Rc::clone(&senv),
                        }));
                    }
                }
                // Getters jako vlastnosti tridy (ne bezne)
                for g in getters {
                    if g.name == key {
                        return Ok(JsValue::Function(JsFunc::User {
                            name: Some(g.name.clone()),
                            params: g.params.clone(),
                            body: FuncBody::Stmts(g.body.clone()),
                            env: Rc::clone(env),
                        }));
                    }
                }
                Ok(JsValue::Undefined)
            }
            JsValue::Object(o) => {
                // Proxy: delegovani na target (bez full handler traps)
                // Pokud key je interni, vrat primo; jinak deleguj
                if !key.starts_with("__") {
                    let proxy_target = o.borrow().props.get("__proxy_target__").cloned();
                    if let Some(target) = proxy_target {
                        return self.get_prop(&target, key);
                    }
                }
                // Specialni klic __proto__ vraci prototyp objektu
                if key == "__proto__" {
                    return Ok(match o.borrow().proto.clone() {
                        Some(p) => JsValue::Object(p),
                        None    => JsValue::Null,
                    });
                }
                Ok(o.borrow().get(key))
            }
            JsValue::Array(a)  => {
                if key == "length" { return Ok(JsValue::Number(a.borrow().len() as f64)); }
                if let Ok(i) = key.parse::<usize>() {
                    return Ok(a.borrow().get(i).cloned().unwrap_or(JsValue::Undefined));
                }
                Ok(JsValue::Undefined)
            }
            JsValue::Str(s) => {
                if key == "length" { return Ok(JsValue::Number(s.chars().count() as f64)); }
                if let Ok(i) = key.parse::<usize>() {
                    return Ok(s.chars().nth(i).map(|c| JsValue::Str(c.to_string())).unwrap_or(JsValue::Undefined));
                }
                Ok(JsValue::Undefined)
            }
            // Map vlastnosti: size (read-only)
            JsValue::Map(m) => {
                if key == "size" { return Ok(JsValue::Number(m.borrow().entries.len() as f64)); }
                Ok(JsValue::Undefined)
            }
            // Set vlastnosti: size (read-only)
            JsValue::Set(s) => {
                if key == "size" { return Ok(JsValue::Number(s.borrow().values.len() as f64)); }
                Ok(JsValue::Undefined)
            }
            // BigNumber vlastnosti (read-only)
            JsValue::BigNumber(bn) => {
                match key {
                    "s" | "sign" => return Ok(JsValue::Number(if bn.as_ref() < &BigDecimal::from(0) { -1.0 } else { 1.0 })),
                    _ => {}
                }
                Ok(JsValue::Undefined)
            }
            // DOM node properties: tagName, textContent, children, parentNode, ...
            JsValue::DomNode(n) => {
                use crate::browser::dom::NodeKind;
                match key {
                    "tagName" | "nodeName" => {
                        return Ok(match n.tag_name() {
                            Some(t) => JsValue::Str(t.to_uppercase()),
                            None    => JsValue::Str(String::new()),
                        });
                    }
                    "nodeType" => {
                        let nt = match &n.kind {
                            NodeKind::Element { .. }     => 1.0,
                            NodeKind::Text(_)            => 3.0,
                            NodeKind::Comment(_)         => 8.0,
                            NodeKind::Document           => 9.0,
                            NodeKind::DocType(_)         => 10.0,
                            NodeKind::Cdata(_)           => 4.0,
                            NodeKind::DocumentFragment   => 11.0,
                        };
                        return Ok(JsValue::Number(nt));
                    }
                    "textContent" | "innerText" => {
                        return Ok(JsValue::Str(n.text_content()));
                    }
                    "innerHTML" => {
                        return Ok(JsValue::Str(serialize::serialize_inner_html(&n)));
                    }
                    "outerHTML" => {
                        return Ok(JsValue::Str(serialize::serialize_outer_html(n)));
                    }
                    "id" => {
                        return Ok(JsValue::Str(n.attr("id").unwrap_or_default()));
                    }
                    "className" => {
                        return Ok(JsValue::Str(n.attr("class").unwrap_or_default()));
                    }
                    "value" if !matches!(n.tag_name().as_deref(), Some("progress") | Some("meter")) => {
                        // Form input value (progress/meter handled below)
                        return Ok(JsValue::Str(n.attr("value").unwrap_or_default()));
                    }
                    "shadowRoot" => {
                        // Bez attachShadow vraci null. Po attachShadow ulozeno v atributu.
                        if n.has_attr("data-shadow-root") {
                            // Vraci empty shadow root prepointer (state se nedrzi mezi calls)
                            let sr = Rc::new(RefCell::new(JsObject::new()));
                            sr.borrow_mut().set("__shadow_root__".into(), JsValue::Bool(true));
                            sr.borrow_mut().set("mode".into(), JsValue::Str("open".into()));
                            sr.borrow_mut().set("host".into(), JsValue::DomNode(Rc::clone(&n)));
                            return Ok(JsValue::Object(sr));
                        }
                        return Ok(JsValue::Null);
                    }
                    "ariaHidden" | "ariaLabel" | "ariaDescribedBy" | "ariaLabelledBy" => {
                        let attr = match key {
                            "ariaHidden" => "aria-hidden",
                            "ariaLabel" => "aria-label",
                            "ariaDescribedBy" => "aria-describedby",
                            "ariaLabelledBy" => "aria-labelledby",
                            _ => "",
                        };
                        return Ok(match n.attr(attr) {
                            Some(v) => JsValue::Str(v),
                            None => JsValue::Null,
                        });
                    }
                    "checked" => {
                        return Ok(JsValue::Bool(n.has_attr("checked")));
                    }
                    // HTMLProgressElement
                    "value" if n.tag_name().as_deref() == Some("progress")
                            || n.tag_name().as_deref() == Some("meter") => {
                        let v = n.attr("value").and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                        return Ok(JsValue::Number(v));
                    }
                    "max" if n.tag_name().as_deref() == Some("progress")
                            || n.tag_name().as_deref() == Some("meter") => {
                        let m = n.attr("max").and_then(|s| s.parse::<f64>().ok()).unwrap_or(1.0);
                        return Ok(JsValue::Number(m));
                    }
                    "min" if n.tag_name().as_deref() == Some("meter") => {
                        let m = n.attr("min").and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                        return Ok(JsValue::Number(m));
                    }
                    "low" | "high" | "optimum" if n.tag_name().as_deref() == Some("meter") => {
                        let v = n.attr(key).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                        return Ok(JsValue::Number(v));
                    }
                    "position" if n.tag_name().as_deref() == Some("progress") => {
                        // Indeterminate -> -1, jinak value/max
                        if !n.has_attr("value") { return Ok(JsValue::Number(-1.0)); }
                        let v = n.attr("value").and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                        let m = n.attr("max").and_then(|s| s.parse::<f64>().ok()).unwrap_or(1.0);
                        return Ok(JsValue::Number(if m > 0.0 { v / m } else { 0.0 }));
                    }
                    // HTMLDataListElement.options - <option> children
                    "options" if n.tag_name().as_deref() == Some("datalist")
                            || n.tag_name().as_deref() == Some("select") => {
                        let mut opts: Vec<JsValue> = Vec::new();
                        n.walk(&mut |node| {
                            if node.tag_name().as_deref() == Some("option") {
                                opts.push(JsValue::DomNode(Rc::clone(node)));
                            }
                        });
                        return Ok(JsValue::Array(Rc::new(RefCell::new(opts))));
                    }
                    "selectedIndex" if n.tag_name().as_deref() == Some("select") => {
                        let mut idx = -1i64;
                        let mut i = 0i64;
                        n.walk(&mut |node| {
                            if node.tag_name().as_deref() == Some("option") {
                                if node.has_attr("selected") && idx == -1 { idx = i; }
                                i += 1;
                            }
                        });
                        return Ok(JsValue::Number(idx as f64));
                    }
                    // HTMLAnchorElement extras - relList
                    "relList" if matches!(n.tag_name().as_deref(), Some("a") | Some("area") | Some("link")) => {
                        let rels = n.attr("rel").unwrap_or_default();
                        let arr: Vec<JsValue> = rels.split_whitespace()
                            .map(|s| JsValue::Str(s.to_string())).collect();
                        return Ok(JsValue::Array(Rc::new(RefCell::new(arr))));
                    }
                    // Element popover state
                    "popover" => {
                        return Ok(match n.attr("popover") {
                            Some(v) => JsValue::Str(v),
                            None => JsValue::Null,
                        });
                    }
                    "files" if n.tag_name().as_deref() == Some("input")
                            && n.attr("type").as_deref() == Some("file") => {
                        // FileList - empty array-like ze atributu data-files (test seed)
                        let mut obj = JsObject::new();
                        obj.set("__filelist__".into(), JsValue::Bool(true));
                        obj.set("length".into(), JsValue::Number(0.0));
                        obj.set("item".into(), native("FileList.item", |_| Ok(JsValue::Null)));
                        return Ok(JsValue::Object(Rc::new(RefCell::new(obj))));
                    }
                    // HTMLFormElement.elements - kolekce form controls
                    "elements" if n.tag_name().as_deref() == Some("form") => {
                        let mut elements: Vec<JsValue> = Vec::new();
                        n.walk(&mut |node| {
                            if Rc::ptr_eq(node, &n) { return; }
                            if let Some(t) = node.tag_name() {
                                if matches!(t.as_str(), "input" | "select" | "textarea" | "button" | "fieldset") {
                                    elements.push(JsValue::DomNode(Rc::clone(node)));
                                }
                            }
                        });
                        return Ok(JsValue::Array(Rc::new(RefCell::new(elements))));
                    }
                    "length" if n.tag_name().as_deref() == Some("form") => {
                        let mut count = 0;
                        n.walk(&mut |node| {
                            if Rc::ptr_eq(node, &n) { return; }
                            if let Some(t) = node.tag_name() {
                                if matches!(t.as_str(), "input" | "select" | "textarea" | "button") {
                                    count += 1;
                                }
                            }
                        });
                        return Ok(JsValue::Number(count as f64));
                    }
                    // HTMLImageElement / canvas - rozmery
                    "naturalWidth" if n.tag_name().as_deref() == Some("img") => {
                        let w = n.attr("width").and_then(|w| w.parse::<f64>().ok()).unwrap_or(0.0);
                        return Ok(JsValue::Number(w));
                    }
                    "naturalHeight" if n.tag_name().as_deref() == Some("img") => {
                        let h = n.attr("height").and_then(|h| h.parse::<f64>().ok()).unwrap_or(0.0);
                        return Ok(JsValue::Number(h));
                    }
                    "complete" if n.tag_name().as_deref() == Some("img") => {
                        return Ok(JsValue::Bool(n.attr("src").is_some()));
                    }
                    "width" if matches!(n.tag_name().as_deref(), Some("img") | Some("canvas") | Some("svg")) => {
                        let w = n.attr("width").and_then(|w| w.parse::<f64>().ok()).unwrap_or(0.0);
                        return Ok(JsValue::Number(w));
                    }
                    "height" if matches!(n.tag_name().as_deref(), Some("img") | Some("canvas") | Some("svg")) => {
                        let h = n.attr("height").and_then(|h| h.parse::<f64>().ok()).unwrap_or(0.0);
                        return Ok(JsValue::Number(h));
                    }
                    // Stare stuby pro offset/client/scroll - nahrazeny nizez s
                    // layout_lookup-aware impl (Tier 1 Item 4).
                    // Hidden / contentEditable / draggable
                    "hidden" => {
                        return Ok(JsValue::Bool(n.has_attr("hidden")));
                    }
                    "contentEditable" => {
                        return Ok(JsValue::Str(n.attr("contenteditable").unwrap_or_else(|| "inherit".to_string())));
                    }
                    "draggable" => {
                        return Ok(JsValue::Bool(n.attr("draggable").as_deref() == Some("true")));
                    }
                    "tabIndex" => {
                        return Ok(JsValue::Number(
                            n.attr("tabindex").and_then(|t| t.parse::<f64>().ok()).unwrap_or(0.0)
                        ));
                    }
                    // HTMLTemplateElement.content - vraci self (template node-content uz drzi children)
                    "content" if n.tag_name().as_deref() == Some("template") => {
                        return Ok(JsValue::DomNode(Rc::clone(&n)));
                    }
                    // Element.namespaceURI / localName / prefix
                    "namespaceURI" => {
                        let ns = match n.tag_name().as_deref() {
                            Some("svg") => "http://www.w3.org/2000/svg",
                            Some("math") => "http://www.w3.org/1998/Math/MathML",
                            _ => "http://www.w3.org/1999/xhtml",
                        };
                        return Ok(JsValue::Str(ns.into()));
                    }
                    "localName" => {
                        return Ok(JsValue::Str(n.tag_name().unwrap_or_default()));
                    }
                    "prefix" => {
                        return Ok(JsValue::Null);
                    }
                    // ChildNode.previousElementSibling / nextElementSibling
                    "previousElementSibling" | "nextElementSibling" => {
                        let parent = match n.parent.borrow().upgrade() { Some(p) => p, None => return Ok(JsValue::Null) };
                        let children = parent.children.borrow();
                        let idx = match children.iter().position(|c| Rc::ptr_eq(c, &n)) { Some(i) => i, None => return Ok(JsValue::Null) };
                        let key_str: &str = key;
                        let target = if key_str == "previousElementSibling" {
                            (0..idx).rev().find(|&i| matches!(children[i].kind, crate::browser::dom::NodeKind::Element { .. }))
                        } else {
                            (idx + 1..children.len()).find(|&i| matches!(children[i].kind, crate::browser::dom::NodeKind::Element { .. }))
                        };
                        return Ok(target.map(|i| JsValue::DomNode(Rc::clone(&children[i])))
                            .unwrap_or(JsValue::Null));
                    }
                    "previousSibling" | "nextSibling" => {
                        let parent = match n.parent.borrow().upgrade() { Some(p) => p, None => return Ok(JsValue::Null) };
                        let children = parent.children.borrow();
                        let idx = match children.iter().position(|c| Rc::ptr_eq(c, &n)) { Some(i) => i, None => return Ok(JsValue::Null) };
                        let key_str: &str = key;
                        let target_idx = if key_str == "previousSibling" {
                            if idx == 0 { return Ok(JsValue::Null); }
                            idx - 1
                        } else {
                            if idx + 1 >= children.len() { return Ok(JsValue::Null); }
                            idx + 1
                        };
                        return Ok(JsValue::DomNode(Rc::clone(&children[target_idx])));
                    }
                    "childElementCount" => {
                        let count = n.children.borrow().iter()
                            .filter(|c| matches!(c.kind, crate::browser::dom::NodeKind::Element { .. }))
                            .count();
                        return Ok(JsValue::Number(count as f64));
                    }
                    "firstElementChild" => {
                        return Ok(n.children.borrow().iter()
                            .find(|c| matches!(c.kind, crate::browser::dom::NodeKind::Element { .. }))
                            .map(|c| JsValue::DomNode(Rc::clone(c)))
                            .unwrap_or(JsValue::Null));
                    }
                    "lastElementChild" => {
                        return Ok(n.children.borrow().iter().rev()
                            .find(|c| matches!(c.kind, crate::browser::dom::NodeKind::Element { .. }))
                            .map(|c| JsValue::DomNode(Rc::clone(c)))
                            .unwrap_or(JsValue::Null));
                    }
                    "isConnected" => {
                        // Walk parents na document
                        let mut cur = Some(Rc::clone(&n));
                        while let Some(c) = cur {
                            if matches!(c.kind, crate::browser::dom::NodeKind::Document) {
                                return Ok(JsValue::Bool(true));
                            }
                            cur = c.parent.borrow().upgrade();
                        }
                        return Ok(JsValue::Bool(false));
                    }
                    "ownerDocument" => {
                        // Return DomNode of the document root
                        return Ok(JsValue::DomNode(Rc::clone(&self.document.borrow().root)));
                    }
                    "open" if matches!(n.tag_name().as_deref(), Some("dialog") | Some("details")) => {
                        return Ok(JsValue::Bool(n.attr("open").is_some()));
                    }
                    "disabled" => {
                        return Ok(JsValue::Bool(n.attr("disabled").is_some()));
                    }
                    "readOnly" | "readonly" => {
                        return Ok(JsValue::Bool(n.attr("readonly").is_some()));
                    }
                    "multiple" => {
                        return Ok(JsValue::Bool(n.attr("multiple").is_some()));
                    }
                    "selected" => {
                        return Ok(JsValue::Bool(n.attr("selected").is_some()));
                    }
                    "options" if n.tag_name().as_deref() == Some("select") => {
                        let opts: Vec<JsValue> = n.get_elements_by_tag("option")
                            .into_iter().map(JsValue::DomNode).collect();
                        return Ok(JsValue::Array(Rc::new(RefCell::new(opts))));
                    }
                    "selectedIndex" if n.tag_name().as_deref() == Some("select") => {
                        let opts = n.get_elements_by_tag("option");
                        let idx = opts.iter().position(|o| o.attr("selected").is_some());
                        return Ok(JsValue::Number(idx.map(|i| i as f64).unwrap_or(-1.0)));
                    }
                    "selectedOptions" if n.tag_name().as_deref() == Some("select") => {
                        let opts: Vec<JsValue> = n.get_elements_by_tag("option").into_iter()
                            .filter(|o| o.attr("selected").is_some())
                            .map(JsValue::DomNode).collect();
                        return Ok(JsValue::Array(Rc::new(RefCell::new(opts))));
                    }
                    "form" if matches!(n.tag_name().as_deref(),
                        Some("input") | Some("select") | Some("textarea") | Some("button")) => {
                        // Najdi nejblizsi form ancestor
                        let mut cur = n.parent.borrow().upgrade();
                        while let Some(p) = cur {
                            if p.tag_name().as_deref() == Some("form") {
                                return Ok(JsValue::DomNode(p));
                            }
                            cur = p.parent.borrow().upgrade();
                        }
                        return Ok(JsValue::Null);
                    }
                    "labels" if matches!(n.tag_name().as_deref(),
                        Some("input") | Some("select") | Some("textarea") | Some("button")) => {
                        // Vrati vsechny label elementy s for=id ukazujici na tento element
                        let id = n.attr("id").unwrap_or_default();
                        if id.is_empty() {
                            return Ok(JsValue::Array(Rc::new(RefCell::new(Vec::new()))));
                        }
                        let doc_root = self.document.borrow().root.clone();
                        let labels = doc_root.get_elements_by_tag("label").into_iter()
                            .filter(|l| l.attr("for").as_deref() == Some(id.as_str()))
                            .map(JsValue::DomNode)
                            .collect();
                        return Ok(JsValue::Array(Rc::new(RefCell::new(labels))));
                    }
                    "src" | "href" | "alt" | "title" | "placeholder" | "lang" | "dir" => {
                        return Ok(JsValue::Str(n.attr(key).unwrap_or_default()));
                    }
                    // HTMLAnchorElement / HTMLAreaElement parts (rozkladaji href URL)
                    "protocol" | "host" | "hostname" | "port" | "pathname"
                    | "search" | "hash" | "origin"
                    if matches!(n.tag_name().as_deref(), Some("a") | Some("area")) => {
                        let href = n.attr("href").unwrap_or_default();
                        let parts = dom_props::parse_url_parts(&href);
                        let key_str: &str = key;
                        let v: String = match key_str {
                            "protocol" => parts.protocol,
                            "host"     => parts.host,
                            "hostname" => parts.hostname,
                            "port"     => parts.port,
                            "pathname" => parts.pathname,
                            "search"   => parts.search,
                            "hash"     => parts.hash,
                            "origin"   => parts.origin,
                            _ => parts.host,
                        };
                        return Ok(JsValue::Str(v));
                    }
                    // HTMLLabelElement.control / .htmlFor
                    "control" if n.tag_name().as_deref() == Some("label") => {
                        // Vrati cilovy form element (z for attributu)
                        if let Some(id) = n.attr("for") {
                            let doc = self.document.borrow().root.clone();
                            if let Some(target) = doc.get_element_by_id(&id) {
                                return Ok(JsValue::DomNode(target));
                            }
                        }
                        return Ok(JsValue::Null);
                    }
                    "htmlFor" if n.tag_name().as_deref() == Some("label") => {
                        return Ok(JsValue::Str(n.attr("for").unwrap_or_default()));
                    }
                    // HTMLOptionElement
                    "text" if n.tag_name().as_deref() == Some("option") => {
                        return Ok(JsValue::Str(n.text_content()));
                    }
                    "label" if n.tag_name().as_deref() == Some("option") => {
                        return Ok(JsValue::Str(n.attr("label")
                            .unwrap_or_else(|| n.text_content())));
                    }
                    "defaultSelected" if n.tag_name().as_deref() == Some("option") => {
                        return Ok(JsValue::Bool(n.attr("selected").is_some()));
                    }
                    // HTMLTableElement / Row / Cell
                    "rows" if matches!(n.tag_name().as_deref(),
                        Some("table") | Some("thead") | Some("tbody") | Some("tfoot")) => {
                        let rows: Vec<JsValue> = n.get_elements_by_tag("tr")
                            .into_iter().map(JsValue::DomNode).collect();
                        return Ok(JsValue::Array(Rc::new(RefCell::new(rows))));
                    }
                    "cells" if n.tag_name().as_deref() == Some("tr") => {
                        let mut cells: Vec<JsValue> = Vec::new();
                        for c in n.children.borrow().iter() {
                            if matches!(c.tag_name().as_deref(), Some("td") | Some("th")) {
                                cells.push(JsValue::DomNode(Rc::clone(c)));
                            }
                        }
                        return Ok(JsValue::Array(Rc::new(RefCell::new(cells))));
                    }
                    "currentTime" if matches!(n.tag_name().as_deref(), Some("video") | Some("audio")) => {
                        return Ok(JsValue::Number(0.0));
                    }
                    "duration" if matches!(n.tag_name().as_deref(), Some("video") | Some("audio")) => {
                        return Ok(JsValue::Number(0.0));
                    }
                    "paused" if matches!(n.tag_name().as_deref(), Some("video") | Some("audio")) => {
                        return Ok(JsValue::Bool(n.attr("paused").is_some()));
                    }
                    "muted" => {
                        return Ok(JsValue::Bool(n.attr("muted").is_some()));
                    }
                    "volume" => {
                        return Ok(JsValue::Number(
                            n.attr("volume").and_then(|v| v.parse::<f64>().ok()).unwrap_or(1.0)
                        ));
                    }
                    "type" | "name" => {
                        return Ok(JsValue::Str(n.attr(key).unwrap_or_default()));
                    }
                    // classList - vraci JsObject s methods (add/remove/toggle/contains)
                    "classList" => {
                        return Ok(dom_props::create_class_list(Rc::clone(&n)));
                    }
                    // dataset - vraci JsObject se vsemi data-* atributy
                    "dataset" => {
                        return Ok(dom_props::create_dataset(&n));
                    }
                    // style - CSSStyleDeclaration object (cached, persistuje pri setteru)
                    "style" => {
                        return Ok(dom_props::get_or_create_style_object(
                            &self.style_cache, Rc::clone(&n),
                        ));
                    }
                    // offsetWidth/Height/Left/Top - rect z layout_lookup (rounded).
                    // Pro zacatek offset == client == rect (bez nezavisleho rozliseni border).
                    "offsetWidth" => {
                        let (_, _, w, _) = self.lookup_layout_rect(&n).unwrap_or((0.0, 0.0, 0.0, 0.0));
                        return Ok(JsValue::Number(w.round() as f64));
                    }
                    "offsetHeight" => {
                        let (_, _, _, h) = self.lookup_layout_rect(&n).unwrap_or((0.0, 0.0, 0.0, 0.0));
                        return Ok(JsValue::Number(h.round() as f64));
                    }
                    "offsetLeft" => {
                        let (x, _, _, _) = self.lookup_layout_rect(&n).unwrap_or((0.0, 0.0, 0.0, 0.0));
                        return Ok(JsValue::Number(x.round() as f64));
                    }
                    "offsetTop" => {
                        let (_, y, _, _) = self.lookup_layout_rect(&n).unwrap_or((0.0, 0.0, 0.0, 0.0));
                        return Ok(JsValue::Number(y.round() as f64));
                    }
                    "offsetParent" => {
                        // Zjednodusene: parent v DOM tree.
                        return Ok(match n.parent.borrow().upgrade() {
                            Some(p) => JsValue::DomNode(p),
                            None    => JsValue::Null,
                        });
                    }
                    // clientWidth/Height - content + padding (bez border). Zatim == rect.
                    "clientWidth" => {
                        let (_, _, w, _) = self.lookup_layout_rect(&n).unwrap_or((0.0, 0.0, 0.0, 0.0));
                        return Ok(JsValue::Number(w.round() as f64));
                    }
                    "clientHeight" => {
                        let (_, _, _, h) = self.lookup_layout_rect(&n).unwrap_or((0.0, 0.0, 0.0, 0.0));
                        return Ok(JsValue::Number(h.round() as f64));
                    }
                    "clientLeft" | "clientTop" => {
                        // Border width (zatim 0).
                        return Ok(JsValue::Number(0.0));
                    }
                    // scrollWidth/Height - content size, scrollTop/Left - scroll position.
                    "scrollWidth" => {
                        let (_, _, w, _) = self.lookup_layout_rect(&n).unwrap_or((0.0, 0.0, 0.0, 0.0));
                        return Ok(JsValue::Number(w.round() as f64));
                    }
                    "scrollHeight" => {
                        let (_, _, _, h) = self.lookup_layout_rect(&n).unwrap_or((0.0, 0.0, 0.0, 0.0));
                        return Ok(JsValue::Number(h.round() as f64));
                    }
                    "scrollTop" | "scrollLeft" => {
                        // Aktualne ulozeno jako attribute (default 0).
                        return Ok(JsValue::Number(
                            n.attr(key).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0)
                        ));
                    }
                    // HTMLFormElement properties
                    "action" if n.tag_name().as_deref() == Some("form") => {
                        return Ok(JsValue::Str(n.attr("action").unwrap_or_default()));
                    }
                    "method" if n.tag_name().as_deref() == Some("form") => {
                        return Ok(JsValue::Str(n.attr("method").unwrap_or_else(|| "GET".to_string())));
                    }
                    // form.elements - vsechny input/select/textarea uvnitr formu
                    "elements" if n.tag_name().as_deref() == Some("form") => {
                        let mut elems: Vec<Rc<crate::browser::dom::NodeData>> = Vec::new();
                        n.walk(&mut |node| {
                            if Rc::ptr_eq(node, n) { return; } // skip self
                            if let Some(t) = node.tag_name() {
                                if matches!(t.as_str(), "input" | "select" | "textarea" | "button") {
                                    elems.push(Rc::clone(node));
                                }
                            }
                        });
                        let arr: Vec<JsValue> = elems.into_iter().map(JsValue::DomNode).collect();
                        return Ok(JsValue::Array(Rc::new(RefCell::new(arr))));
                    }
                    "children" | "childNodes" => {
                        let arr: Vec<JsValue> = n.children.borrow().iter()
                            .map(|c| JsValue::DomNode(Rc::clone(c))).collect();
                        return Ok(JsValue::Array(Rc::new(RefCell::new(arr))));
                    }
                    "firstChild" => {
                        return Ok(match n.children.borrow().first() {
                            Some(c) => JsValue::DomNode(Rc::clone(c)),
                            None    => JsValue::Null,
                        });
                    }
                    "lastChild" => {
                        return Ok(match n.children.borrow().last() {
                            Some(c) => JsValue::DomNode(Rc::clone(c)),
                            None    => JsValue::Null,
                        });
                    }
                    "parentNode" | "parentElement" => {
                        return Ok(match n.parent.borrow().upgrade() {
                            Some(p) => JsValue::DomNode(p),
                            None    => JsValue::Null,
                        });
                    }
                    _ => {}
                }
                Ok(JsValue::Undefined)
            }
            // BigInt vlastnosti (read-only)
            JsValue::BigInt(bn) => {
                match key {
                    "sign" => return Ok(JsValue::Number(match bn.sign() {
                        Sign::Minus => -1.0,
                        Sign::NoSign => 0.0,
                        Sign::Plus => 1.0,
                    })),
                    _ => {}
                }
                Ok(JsValue::Undefined)
            }
            // Native funkce: Number.XXX konstanty + Array.isArray atd.
            JsValue::Function(JsFunc::Native(fname, _)) => {
                match (fname.as_str(), key) {
                    ("Number", "MAX_VALUE")         => return Ok(JsValue::Number(f64::MAX)),
                    ("Number", "MIN_VALUE")         => return Ok(JsValue::Number(f64::MIN_POSITIVE)),
                    ("Number", "MAX_SAFE_INTEGER")  => return Ok(JsValue::Number(9007199254740991.0)),
                    ("Number", "MIN_SAFE_INTEGER")  => return Ok(JsValue::Number(-9007199254740991.0)),
                    ("Number", "POSITIVE_INFINITY") => return Ok(JsValue::Number(f64::INFINITY)),
                    ("Number", "NEGATIVE_INFINITY") => return Ok(JsValue::Number(f64::NEG_INFINITY)),
                    ("Number", "NaN")               => return Ok(JsValue::Number(f64::NAN)),
                    ("Number", "EPSILON")           => return Ok(JsValue::Number(f64::EPSILON)),
                    _ => {}
                }
                Ok(JsValue::Undefined)
            }
            _ => Ok(JsValue::Undefined),
        }
    }
}
