//! eval_call - function call dispatch (massive switch over callee types).

use super::*;

impl Interpreter {
    pub(super) fn eval_call(&mut self, callee: &Expr, args: &[Expr], optional: bool, env: &Rc<RefCell<Environment>>) -> EvalResult {
        // super(args) - volani konstruktoru rodicovske tridy
        if matches!(callee, Expr::Ident(n) if n == "super") {
            let super_class = env.borrow().get("__super_class__")
                .ok_or_else(|| JsError::Runtime("super() lze volat jen uvnitr konstruktoru tridy".into()))?;
            let arg_vals = self.eval_args(args, env)?;
            let this_val = env.borrow().get("this").unwrap_or(JsValue::Undefined);
            if let JsValue::Object(ref this_obj) = this_val {
                let this_obj = Rc::clone(this_obj);
                self.run_super_constructor(super_class, arg_vals, &this_obj, env)?;
            }
            return Ok(this_val);
        }

        // eval(src) - special case: potrebuje pristup k interpreteru a aktualnimu env
        if matches!(callee, Expr::Ident(n) if n == "eval") {
            let arg_vals = self.eval_args(args, env)?;
            let src = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
            return match src {
                JsValue::Str(s) => {
                    use crate::lexer::base::Lexer;
                    use crate::parser::Parser;
                    use crate::tokens::TokenKind;
                    let lexer = Lexer::parse_str(&s, "<eval>")
                        .map_err(|e| JsError::Runtime(format!("eval SyntaxError: {e}")))?;
                    let tokens: Vec<_> = lexer.tokens.into_iter()
                        .filter(|t| !matches!(t.kind,
                            TokenKind::Whitespace | TokenKind::Newline
                            | TokenKind::CommentLine(_) | TokenKind::CommentBlock(_)))
                        .collect();
                    let mut parser = Parser::new(tokens);
                    let prog = parser.parse()
                        .map_err(|e| JsError::Runtime(format!("eval SyntaxError: {e}")))?;
                    // eval vraci completion value: posledni vyraz
                    // Pokud je program prazdny, vrat undefined
                    if prog.body.is_empty() {
                        return Ok(JsValue::Undefined);
                    }
                    // Spust vsechny prikazy krome posledniho
                    let last_idx = prog.body.len() - 1;
                    for stmt in &prog.body[..last_idx] {
                        match self.exec_stmt(stmt, env)? {
                            Some(Signal::Return(v)) => return Ok(v),
                            Some(sig) => return Err(JsError::Runtime(format!("eval: neocekavany signal {:?}", sig))),
                            None => {}
                        }
                    }
                    // Posledni prikaz: vyraz vraci hodnotu, jinak undefined.
                    // Unwrap Stmt::WithLine pred match.
                    let last = &prog.body[last_idx];
                    let mut peeled = last;
                    while let crate::ast::Stmt::WithLine { inner, .. } = peeled {
                        peeled = inner;
                    }
                    match peeled {
                        crate::ast::Stmt::Expr(e) => self.eval(e, env),
                        _ => {
                            match self.exec_stmt(last, env)? {
                                Some(Signal::Return(v)) => Ok(v),
                                _ => Ok(JsValue::Undefined),
                            }
                        }
                    }
                }
                other => Ok(other), // non-string preda as-is
            };
        }

        // super.method(args) - volani metody rodicovske tridy
        if let Expr::Member { object, prop, .. } = callee {
            if matches!(object.as_ref(), Expr::Ident(n) if n == "super") {
                let super_class = env.borrow().get("__super_class__")
                    .ok_or_else(|| JsError::Runtime("super.method() lze volat jen uvnitr tridy".into()))?;
                let key = self.resolve_prop_key(prop, env)?;
                let method = self.get_class_method_func(&super_class, &key)?;
                let this_val = env.borrow().get("this").unwrap_or(JsValue::Undefined);
                let arg_vals = self.eval_args(args, env)?;
                return self.call_function(method, arg_vals, Some(this_val));
            }
        }

        if let Expr::Member { object, prop, optional: member_opt } = callee {
            let this = self.eval(object, env)?;
            // optional chaining: obj?.method() -> Undefined kdyz obj je null/undefined
            if (optional || *member_opt) && matches!(this, JsValue::Null | JsValue::Undefined) {
                return Ok(JsValue::Undefined);
            }
            let key = self.resolve_prop_key(prop, env)?;

            // ─── Object.groupBy / Map.groupBy (ES2024) ───────────────────────
            // Detekce DRIVE nez specificke arms, protoze Map/Native have early return
            if key.as_str() == "groupBy" {
                if let Expr::Ident(name) = object.as_ref() {
                    if name == "Object" || name == "Map" {
                        let arg_vals = self.eval_args(args, env)?;
                        let mut iter = arg_vals.into_iter();
                        let items_val = iter.next().unwrap_or(JsValue::Undefined);
                        let cb = iter.next().unwrap_or(JsValue::Undefined);
                        let items = collect_iterable_values(&items_val);
                        if name == "Object" {
                            let groups_obj = JsObject::new();
                            let groups_rc = Rc::new(RefCell::new(groups_obj));
                            for (i, item) in items.into_iter().enumerate() {
                                let k = self.call_function(
                                    cb.clone(),
                                    vec![item.clone(), JsValue::Number(i as f64)],
                                    None,
                                )?;
                                let key_str = k.to_string();
                                let mut g = groups_rc.borrow_mut();
                                let bucket = g.props.entry(key_str)
                                    .or_insert_with(|| JsValue::Array(Rc::new(RefCell::new(Vec::new()))));
                                if let JsValue::Array(a) = bucket {
                                    a.borrow_mut().push(item);
                                }
                            }
                            return Ok(JsValue::Object(groups_rc));
                        } else {
                            // Map.groupBy
                            let mut m = JsMap::new();
                            for (i, item) in items.into_iter().enumerate() {
                                let k = self.call_function(
                                    cb.clone(),
                                    vec![item.clone(), JsValue::Number(i as f64)],
                                    None,
                                )?;
                                let existing = m.get(&k);
                                let bucket = match existing {
                                    JsValue::Array(a) => a,
                                    _ => {
                                        let new_arr = Rc::new(RefCell::new(Vec::new()));
                                        m.set(k.clone(), JsValue::Array(Rc::clone(&new_arr)));
                                        new_arr
                                    }
                                };
                                bucket.borrow_mut().push(item);
                            }
                            return Ok(JsValue::Map(Rc::new(RefCell::new(m))));
                        }
                    }
                }
            }

            // Built-in Array/String/Object/Map/Set instance metody -- dispatch pred call_function
            match &this {
                // ─── Map metody ────────────────────────────────────────────
                JsValue::Map(map_rc) => {
                    let map_rc2 = Rc::clone(map_rc);
                    let arg_vals = self.eval_args(args, env)?;
                    match key.as_str() {
                        "set" => {
                            let mut iter = arg_vals.into_iter();
                            let k = iter.next().unwrap_or(JsValue::Undefined);
                            let v = iter.next().unwrap_or(JsValue::Undefined);
                            map_rc2.borrow_mut().set(k, v);
                            return Ok(JsValue::Map(map_rc2));
                        }
                        "get" => {
                            let k = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            return Ok(map_rc2.borrow().get(&k));
                        }
                        "has" => {
                            let k = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            return Ok(JsValue::Bool(map_rc2.borrow().has(&k)));
                        }
                        "delete" => {
                            let k = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            return Ok(JsValue::Bool(map_rc2.borrow_mut().delete(&k)));
                        }
                        "clear" => { map_rc2.borrow_mut().entries.clear(); return Ok(JsValue::Undefined); }
                        "keys" => {
                            let keys: Vec<JsValue> = map_rc2.borrow().entries.iter().map(|(k,_)| k.clone()).collect();
                            return Ok(make_array_iterator(keys));
                        }
                        "values" => {
                            let vals: Vec<JsValue> = map_rc2.borrow().entries.iter().map(|(_,v)| v.clone()).collect();
                            return Ok(make_array_iterator(vals));
                        }
                        "entries" => {
                            let entries: Vec<JsValue> = map_rc2.borrow().entries.iter()
                                .map(|(k,v)| JsValue::Array(Rc::new(RefCell::new(vec![k.clone(), v.clone()]))))
                                .collect();
                            return Ok(make_array_iterator(entries));
                        }
                        "forEach" => {
                            let cb = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            let entries: Vec<(JsValue,JsValue)> = map_rc2.borrow().entries.clone();
                            for (k, v) in entries {
                                self.call_function(cb.clone(), vec![v, k, JsValue::Map(Rc::clone(&map_rc2))], None)?;
                            }
                            return Ok(JsValue::Undefined);
                        }
                        _ => return Ok(JsValue::Undefined),
                    }
                }
                // ─── Set metody ────────────────────────────────────────────
                JsValue::Set(set_rc) => {
                    let set_rc2 = Rc::clone(set_rc);
                    let arg_vals = self.eval_args(args, env)?;
                    match key.as_str() {
                        "add" => {
                            let v = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            set_rc2.borrow_mut().add(v);
                            return Ok(JsValue::Set(set_rc2));
                        }
                        "has" => {
                            let v = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            return Ok(JsValue::Bool(set_rc2.borrow().has(&v)));
                        }
                        "delete" => {
                            let v = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            return Ok(JsValue::Bool(set_rc2.borrow_mut().delete(&v)));
                        }
                        "clear" => { set_rc2.borrow_mut().values.clear(); return Ok(JsValue::Undefined); }
                        "keys" | "values" => {
                            let vals: Vec<JsValue> = set_rc2.borrow().values.clone();
                            return Ok(make_array_iterator(vals));
                        }
                        "entries" => {
                            let entries: Vec<JsValue> = set_rc2.borrow().values.iter()
                                .map(|v| JsValue::Array(Rc::new(RefCell::new(vec![v.clone(), v.clone()]))))
                                .collect();
                            return Ok(make_array_iterator(entries));
                        }
                        "forEach" => {
                            let cb = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            let vals: Vec<JsValue> = set_rc2.borrow().values.clone();
                            for v in vals {
                                self.call_function(cb.clone(), vec![v.clone(), v, JsValue::Set(Rc::clone(&set_rc2))], None)?;
                            }
                            return Ok(JsValue::Undefined);
                        }
                        // ─── ES2025 Set operace ─────────────────────────────
                        "union" => {
                            let other = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            let other_vals = collect_iterable_values(&other);
                            let mut result = JsSet::new();
                            for v in set_rc2.borrow().values.clone() { result.add(v); }
                            for v in other_vals { result.add(v); }
                            return Ok(JsValue::Set(Rc::new(RefCell::new(result))));
                        }
                        "intersection" => {
                            let other = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            let other_vals = collect_iterable_values(&other);
                            let mut result = JsSet::new();
                            for v in set_rc2.borrow().values.clone() {
                                if other_vals.iter().any(|x| JsMap::key_eq(x, &v)) {
                                    result.add(v);
                                }
                            }
                            return Ok(JsValue::Set(Rc::new(RefCell::new(result))));
                        }
                        "difference" => {
                            let other = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            let other_vals = collect_iterable_values(&other);
                            let mut result = JsSet::new();
                            for v in set_rc2.borrow().values.clone() {
                                if !other_vals.iter().any(|x| JsMap::key_eq(x, &v)) {
                                    result.add(v);
                                }
                            }
                            return Ok(JsValue::Set(Rc::new(RefCell::new(result))));
                        }
                        "symmetricDifference" => {
                            let other = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            let other_vals = collect_iterable_values(&other);
                            let mut result = JsSet::new();
                            for v in set_rc2.borrow().values.clone() {
                                if !other_vals.iter().any(|x| JsMap::key_eq(x, &v)) {
                                    result.add(v);
                                }
                            }
                            for v in other_vals {
                                if !set_rc2.borrow().has(&v) {
                                    result.add(v);
                                }
                            }
                            return Ok(JsValue::Set(Rc::new(RefCell::new(result))));
                        }
                        "isSubsetOf" => {
                            let other = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            let other_vals = collect_iterable_values(&other);
                            let result = set_rc2.borrow().values.iter().all(|v| {
                                other_vals.iter().any(|x| JsMap::key_eq(x, v))
                            });
                            return Ok(JsValue::Bool(result));
                        }
                        "isSupersetOf" => {
                            let other = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            let other_vals = collect_iterable_values(&other);
                            let result = other_vals.iter().all(|v| set_rc2.borrow().has(v));
                            return Ok(JsValue::Bool(result));
                        }
                        "isDisjointFrom" => {
                            let other = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            let other_vals = collect_iterable_values(&other);
                            let result = !set_rc2.borrow().values.iter().any(|v| {
                                other_vals.iter().any(|x| JsMap::key_eq(x, v))
                            });
                            return Ok(JsValue::Bool(result));
                        }
                        _ => return Ok(JsValue::Undefined),
                    }
                }
                JsValue::Object(obj_rc) => {
                    let obj_rc2 = Rc::clone(obj_rc);
                    // ─── Iterator helpers (ES2025) ────────────────────────
                    if matches!(obj_rc2.borrow().props.get("__iterator_helpers__"), Some(JsValue::Bool(true))) {
                        let helper_methods = ["toArray", "map", "filter", "take", "drop",
                            "reduce", "forEach", "some", "every", "find", "flatMap"];
                        if helper_methods.contains(&key.as_str()) {
                            let arg_vals = self.eval_args(args, env)?;
                            return self.iterator_helper_method(JsValue::Object(obj_rc2), &key, arg_vals);
                        }
                    }
                    // ─── document metody s pristupem k interpretu ──────────
                    if matches!(obj_rc2.borrow().props.get("__document__"), Some(JsValue::Bool(true))) {
                        if key == "createElement" {
                            let arg_vals = self.eval_args(args, env)?;
                            let tag = arg_vals.into_iter().next()
                                .map(|v| v.to_string()).unwrap_or_else(|| "div".into());
                            let node = crate::browser::dom::NodeData::new_element(
                                &tag, std::collections::HashMap::new()
                            );
                            let node_ptr = Rc::as_ptr(&node) as usize;
                            // Pokud je tag registrovany jako custom element, zavolej konstruktor
                            let ctor = self.custom_elements.borrow().get(&tag).cloned();
                            if let Some(ctor_val) = ctor {
                                match self.call_new(ctor_val, vec![]) {
                                    Ok(instance) => {
                                        self.custom_element_instances.borrow_mut()
                                            .insert(node_ptr, instance);
                                    }
                                    Err(_) => {}
                                }
                            }
                            return Ok(JsValue::DomNode(node));
                        }
                    }
                    // ─── DOM Element metody ─────────────────────────────
                    if matches!(obj_rc2.borrow().props.get("__element__"), Some(JsValue::Bool(true))) {
                        let arg_vals = self.eval_args(args, env)?;
                        match key.as_str() {
                            "getAttribute" => {
                                let name = arg_vals.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
                                let attrs = obj_rc2.borrow().props.get("__attrs__").cloned();
                                if let Some(JsValue::Object(a)) = attrs {
                                    let v = a.borrow().get(&name);
                                    return Ok(if matches!(v, JsValue::Undefined) { JsValue::Null } else { v });
                                }
                                return Ok(JsValue::Null);
                            }
                            "setAttribute" => {
                                let mut iter = arg_vals.into_iter();
                                let name = iter.next().map(|v| v.to_string()).unwrap_or_default();
                                let val = iter.next().map(|v| JsValue::Str(v.to_string()))
                                    .unwrap_or(JsValue::Str(String::new()));
                                let attrs = obj_rc2.borrow().props.get("__attrs__").cloned();
                                if let Some(JsValue::Object(a)) = attrs {
                                    a.borrow_mut().set(name.clone(), val.clone());
                                }
                                // Specialni atributy: id, class promitnout do props
                                match name.as_str() {
                                    "id" | "class" => {
                                        let prop = if name == "class" { "className" } else { "id" };
                                        obj_rc2.borrow_mut().set(prop.into(), val);
                                    }
                                    _ => {}
                                }
                                return Ok(JsValue::Undefined);
                            }
                            "hasAttribute" => {
                                let name = arg_vals.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
                                let attrs = obj_rc2.borrow().props.get("__attrs__").cloned();
                                if let Some(JsValue::Object(a)) = attrs {
                                    return Ok(JsValue::Bool(a.borrow().has_own(&name)));
                                }
                                return Ok(JsValue::Bool(false));
                            }
                            "removeAttribute" => {
                                let name = arg_vals.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
                                let attrs = obj_rc2.borrow().props.get("__attrs__").cloned();
                                if let Some(JsValue::Object(a)) = attrs {
                                    a.borrow_mut().props.remove(&name);
                                }
                                return Ok(JsValue::Undefined);
                            }
                            "appendChild" => {
                                let child = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                                let children = obj_rc2.borrow().props.get("childNodes").cloned();
                                if let Some(JsValue::Array(arr)) = children {
                                    arr.borrow_mut().push(child.clone());
                                }
                                return Ok(child);
                            }
                            "removeChild" => {
                                let child = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                                let children = obj_rc2.borrow().props.get("childNodes").cloned();
                                if let Some(JsValue::Array(arr)) = children {
                                    if let JsValue::Object(child_obj) = &child {
                                        arr.borrow_mut().retain(|item| {
                                            if let JsValue::Object(o) = item {
                                                !Rc::ptr_eq(o, child_obj)
                                            } else { true }
                                        });
                                    }
                                }
                                return Ok(child);
                            }
                            "addEventListener" => {
                                let mut iter = arg_vals.into_iter();
                                let evt_type = iter.next().map(|v| v.to_string()).unwrap_or_default();
                                let listener = iter.next().unwrap_or(JsValue::Undefined);
                                let listeners_val = obj_rc2.borrow().props.get("__listeners__").cloned();
                                if let Some(JsValue::Object(lst)) = listeners_val {
                                    let existing = lst.borrow().props.get(&evt_type).cloned();
                                    let arr = match existing {
                                        Some(JsValue::Array(a)) => a,
                                        _ => {
                                            let new_arr = Rc::new(RefCell::new(Vec::new()));
                                            lst.borrow_mut().set(evt_type.clone(),
                                                JsValue::Array(Rc::clone(&new_arr)));
                                            new_arr
                                        }
                                    };
                                    arr.borrow_mut().push(listener);
                                }
                                return Ok(JsValue::Undefined);
                            }
                            "removeEventListener" => {
                                return Ok(JsValue::Undefined);
                            }
                            "dispatchEvent" => {
                                let event = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                                let evt_type = if let JsValue::Object(eo) = &event {
                                    match eo.borrow().get("type") {
                                        JsValue::Str(s) => s,
                                        _ => String::new(),
                                    }
                                } else { String::new() };
                                let listeners_val = obj_rc2.borrow().props.get("__listeners__").cloned();
                                if let Some(JsValue::Object(lst)) = listeners_val {
                                    let arr = lst.borrow().props.get(&evt_type).cloned();
                                    if let Some(JsValue::Array(a)) = arr {
                                        let listeners: Vec<JsValue> = a.borrow().clone();
                                        for l in listeners {
                                            self.call_function(l, vec![event.clone()], None)?;
                                        }
                                    }
                                }
                                return Ok(JsValue::Bool(true));
                            }
                            "click" | "focus" | "blur" => {
                                return Ok(JsValue::Undefined);
                            }
                            _ => {}
                        }
                    }
                    // ─── Response (fetch) - text/json/ok/headers.get ──────
                    if matches!(obj_rc2.borrow().props.get("__response__"), Some(JsValue::Bool(true))) {
                        let body = match obj_rc2.borrow().props.get("__body__").cloned() {
                            Some(JsValue::Str(s)) => s,
                            _ => String::new(),
                        };
                        match key.as_str() {
                            "text" => {
                                return Ok(make_settled_promise("fulfilled", JsValue::Str(body)));
                            }
                            "json" => {
                                match json_parse(&body) {
                                    Ok(v) => return Ok(make_settled_promise("fulfilled", v)),
                                    Err(e) => {
                                        let mut err = JsObject::new();
                                        err.set("name".into(),    JsValue::Str("SyntaxError".into()));
                                        err.set("message".into(), JsValue::Str(e));
                                        return Ok(make_settled_promise("rejected",
                                            JsValue::Object(Rc::new(RefCell::new(err)))));
                                    }
                                }
                            }
                            "blob" | "arrayBuffer" => {
                                // Stub - vratime body jako string Promise
                                return Ok(make_settled_promise("fulfilled", JsValue::Str(body)));
                            }
                            _ => {}
                        }
                    }
                    // Headers.get(name) - case-insensitive
                    if matches!(obj_rc2.borrow().props.get("__headers__"), Some(JsValue::Bool(true))) {
                        let arg_vals = self.eval_args(args, env)?;
                        match key.as_str() {
                            "get" => {
                                let name = arg_vals.into_iter().next()
                                    .map(|v| v.to_string().to_lowercase())
                                    .unwrap_or_default();
                                let v = obj_rc2.borrow().get(&name);
                                return Ok(if matches!(v, JsValue::Undefined) { JsValue::Null } else { v });
                            }
                            "has" => {
                                let name = arg_vals.into_iter().next()
                                    .map(|v| v.to_string().to_lowercase())
                                    .unwrap_or_default();
                                return Ok(JsValue::Bool(obj_rc2.borrow().has_own(&name)));
                            }
                            _ => {}
                        }
                    }
                    // ─── Worker - postMessage/terminate (real thread) ──────
                    if matches!(obj_rc2.borrow().props.get("__worker__"), Some(JsValue::Bool(true))) {
                        let arg_vals = self.eval_args(args, env)?;
                        let worker_id = match obj_rc2.borrow().props.get("__worker_id__").cloned() {
                            Some(JsValue::Number(n)) => n as u32,
                            _ => return Ok(JsValue::Undefined),
                        };
                        match key.as_str() {
                            "postMessage" => {
                                let msg = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                                let serialized = json_stringify(&msg, 0, 0)
                                    .unwrap_or_else(|| msg.to_string());
                                if let Some(state) = self.workers.borrow().get(&worker_id) {
                                    let _ = state.sender.send(serialized);
                                }
                                return Ok(JsValue::Undefined);
                            }
                            "terminate" => {
                                self.workers.borrow_mut().remove(&worker_id);
                                return Ok(JsValue::Undefined);
                            }
                            _ => {}
                        }
                    }
                    // ─── Storage API (localStorage/sessionStorage) ──────
                    if matches!(obj_rc2.borrow().props.get("__storage__"), Some(JsValue::Bool(true))) {
                        let arg_vals = self.eval_args(args, env)?;
                        let data_val = obj_rc2.borrow().props.get("__storage_data__").cloned();
                        let data = match data_val {
                            Some(JsValue::Object(d)) => d,
                            _ => return Ok(JsValue::Undefined),
                        };
                        // Persist-helper kdyz storage je persistent (localStorage)
                        let persist_now = || {
                            let is_persistent = matches!(
                                obj_rc2.borrow().props.get("__storage_persistent__"),
                                Some(JsValue::Bool(true))
                            );
                            if !is_persistent { return; }
                            let name = match obj_rc2.borrow().props.get("__storage_name__").cloned() {
                                Some(JsValue::Str(n)) => n,
                                _ => return,
                            };
                            let entries: Vec<(String, String)> = data.borrow().own_keys()
                                .into_iter().filter_map(|k| {
                                    let v = data.borrow().get(&k);
                                    if let JsValue::Str(s) = v { Some((k, s)) } else { None }
                                }).collect();
                            let _ = save_storage_to_disk(&name, &entries);
                        };
                        match key.as_str() {
                            "setItem" => {
                                let mut iter = arg_vals.into_iter();
                                let k = iter.next().map(|v| v.to_string()).unwrap_or_default();
                                let v = iter.next().map(|v| JsValue::Str(v.to_string()))
                                    .unwrap_or(JsValue::Str(String::new()));
                                data.borrow_mut().set(k, v);
                                let len = data.borrow().own_keys().len() as f64;
                                obj_rc2.borrow_mut().set("length".into(), JsValue::Number(len));
                                persist_now();
                                return Ok(JsValue::Undefined);
                            }
                            "getItem" => {
                                let k = arg_vals.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
                                let v = data.borrow().get(&k);
                                return Ok(if matches!(v, JsValue::Undefined) { JsValue::Null } else { v });
                            }
                            "removeItem" => {
                                let k = arg_vals.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
                                data.borrow_mut().props.remove(&k);
                                let len = data.borrow().own_keys().len() as f64;
                                obj_rc2.borrow_mut().set("length".into(), JsValue::Number(len));
                                persist_now();
                                return Ok(JsValue::Undefined);
                            }
                            "clear" => {
                                data.borrow_mut().props.clear();
                                obj_rc2.borrow_mut().set("length".into(), JsValue::Number(0.0));
                                persist_now();
                                return Ok(JsValue::Undefined);
                            }
                            "key" => {
                                let i = arg_vals.into_iter().next().map(|v| v.to_number() as usize).unwrap_or(0);
                                let keys = data.borrow().own_keys();
                                return Ok(keys.get(i).cloned().map(JsValue::Str).unwrap_or(JsValue::Null));
                            }
                            _ => {}
                        }
                    }
                    // ─── Intl.* metody ───────────────────────────────────
                    if let Some(JsValue::Str(kind)) = obj_rc2.borrow().props.get("__intl_kind__").cloned() {
                        let locale = match obj_rc2.borrow().props.get("__intl_locale__").cloned() {
                            Some(JsValue::Str(s)) => s,
                            _ => "en-US".into(),
                        };
                        let arg_vals = self.eval_args(args, env)?;
                        match (kind.as_str(), key.as_str()) {
                            ("number", "format") => {
                                let n = arg_vals.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
                                return Ok(JsValue::Str(format_number_intl(n, &locale)));
                            }
                            ("datetime", "format") => {
                                let ms = arg_vals.first().and_then(|v| get_date_ms(v))
                                    .or_else(|| arg_vals.first().map(|v| v.to_number()))
                                    .unwrap_or(0.0);
                                return Ok(JsValue::Str(format_datetime_intl(ms, &locale)));
                            }
                            ("collator", "compare") => {
                                let a = arg_vals.get(0).map(|v| v.to_string()).unwrap_or_default();
                                let b = arg_vals.get(1).map(|v| v.to_string()).unwrap_or_default();
                                let cmp = collator_compare_intl(&a, &b, &locale);
                                return Ok(JsValue::Number(cmp as f64));
                            }
                            ("plural", "select") => {
                                let n = arg_vals.first().map(|v| v.to_number()).unwrap_or(0.0);
                                return Ok(JsValue::Str(plural_select(n, &locale)));
                            }
                            _ => {}
                        }
                    }
                    // ─── WeakRef.deref / FinalizationRegistry methods ──────
                    if obj_rc2.borrow().props.contains_key("__weak_target__") {
                        if key == "deref" {
                            return Ok(obj_rc2.borrow().props.get("__weak_target__")
                                .cloned().unwrap_or(JsValue::Undefined));
                        }
                    }
                    if obj_rc2.borrow().props.contains_key("__finalizer__") {
                        // Stub: register/unregister - jen vrat undefined
                        match key.as_str() {
                            "register" | "unregister" => return Ok(JsValue::Undefined),
                            _ => {}
                        }
                    }
                    // ─── Date instance metody ──────────────────────────────
                    // Extrahujeme ms pred if-blokem, aby obj_rc2 nebyl borrowed pri borrow_mut() uvnitr.
                    let date_ms_val = { let b = obj_rc2.borrow(); b.props.get("__date_ms__").and_then(|v| if let JsValue::Number(n) = v { Some(*n) } else { None }) };
                    if let Some(ms) = date_ms_val {
                        let arg_vals = self.eval_args(args, env)?;
                        let (yr, mo, day, hr, min, sec, ms_part) = ms_to_parts(ms);
                        match key.as_str() {
                            "getTime"           => return Ok(JsValue::Number(ms)),
                            "getFullYear"       => return Ok(JsValue::Number(yr as f64)),
                            "getMonth"          => return Ok(JsValue::Number(mo as f64)),
                            "getDate"           => return Ok(JsValue::Number(day as f64)),
                            "getHours"          => return Ok(JsValue::Number(hr as f64)),
                            "getMinutes"        => return Ok(JsValue::Number(min as f64)),
                            "getSeconds"        => return Ok(JsValue::Number(sec as f64)),
                            "getMilliseconds"   => return Ok(JsValue::Number(ms_part as f64)),
                            "getDay"            => {
                                // Den tydne: 0=Sun,...,6=Sat
                                let days = (ms / 86_400_000.0) as i64;
                                return Ok(JsValue::Number(((days + 4) % 7).rem_euclid(7) as f64));
                            }
                            // UTC gettery - nase implementace uz pouziva UTC, takze identicky
                            "getUTCFullYear"    => return Ok(JsValue::Number(yr as f64)),
                            "getUTCMonth"       => return Ok(JsValue::Number(mo as f64)),
                            "getUTCDate"        => return Ok(JsValue::Number(day as f64)),
                            "getUTCHours"       => return Ok(JsValue::Number(hr as f64)),
                            "getUTCMinutes"     => return Ok(JsValue::Number(min as f64)),
                            "getUTCSeconds"     => return Ok(JsValue::Number(sec as f64)),
                            "getUTCMilliseconds"=> return Ok(JsValue::Number(ms_part as f64)),
                            "getUTCDay"         => {
                                let days = (ms / 86_400_000.0) as i64;
                                return Ok(JsValue::Number(((days + 4) % 7).rem_euclid(7) as f64));
                            }
                            "valueOf" | "getTimezoneOffset" => return Ok(JsValue::Number(ms)),
                            "toISOString" => {
                                return Ok(JsValue::Str(format!(
                                    "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
                                    yr, mo+1, day, hr, min, sec, ms_part
                                )));
                            }
                            "toLocaleDateString" => {
                                return Ok(JsValue::Str(format!("{}/{}/{}", mo+1, day, yr)));
                            }
                            "toLocaleTimeString" => {
                                return Ok(JsValue::Str(format!("{:02}:{:02}:{:02}", hr, min, sec)));
                            }
                            "toLocaleString" | "toString" => {
                                return Ok(JsValue::Str(format!(
                                    "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                                    yr, mo+1, day, hr, min, sec
                                )));
                            }
                            "toDateString" => {
                                return Ok(JsValue::Str(format!("{:04}-{:02}-{:02}", yr, mo+1, day)));
                            }
                            "setTime" => {
                                let new_ms = arg_vals.into_iter().next()
                                    .map(|v| v.to_number()).unwrap_or(f64::NAN);
                                obj_rc2.borrow_mut().props.insert("__date_ms__".into(), JsValue::Number(new_ms));
                                return Ok(JsValue::Number(new_ms));
                            }
                            "setFullYear" => {
                                let mut it = arg_vals.into_iter();
                                let ny = it.next().map(|v| v.to_number() as i64).unwrap_or(yr);
                                let nm = it.next().map(|v| v.to_number() as u32).unwrap_or(mo);
                                let nd = it.next().map(|v| v.to_number() as u32).unwrap_or(day);
                                let new_ms = parts_to_ms(ny, nm, nd, hr, min, sec, ms_part);
                                obj_rc2.borrow_mut().props.insert("__date_ms__".into(), JsValue::Number(new_ms));
                                return Ok(JsValue::Number(new_ms));
                            }
                            "setMonth" => {
                                let mut it = arg_vals.into_iter();
                                let nm = it.next().map(|v| v.to_number() as u32).unwrap_or(mo);
                                let nd = it.next().map(|v| v.to_number() as u32).unwrap_or(day);
                                let new_ms = parts_to_ms(yr, nm, nd, hr, min, sec, ms_part);
                                obj_rc2.borrow_mut().props.insert("__date_ms__".into(), JsValue::Number(new_ms));
                                return Ok(JsValue::Number(new_ms));
                            }
                            "setDate" => {
                                let nd = arg_vals.into_iter().next().map(|v| v.to_number() as u32).unwrap_or(day);
                                let new_ms = parts_to_ms(yr, mo, nd, hr, min, sec, ms_part);
                                obj_rc2.borrow_mut().props.insert("__date_ms__".into(), JsValue::Number(new_ms));
                                return Ok(JsValue::Number(new_ms));
                            }
                            "setHours" => {
                                let mut it = arg_vals.into_iter();
                                let nh = it.next().map(|v| v.to_number() as u32).unwrap_or(hr);
                                let nm = it.next().map(|v| v.to_number() as u32).unwrap_or(min);
                                let ns = it.next().map(|v| v.to_number() as u32).unwrap_or(sec);
                                let nms = it.next().map(|v| v.to_number() as u32).unwrap_or(ms_part);
                                let new_ms = parts_to_ms(yr, mo, day, nh, nm, ns, nms);
                                obj_rc2.borrow_mut().props.insert("__date_ms__".into(), JsValue::Number(new_ms));
                                return Ok(JsValue::Number(new_ms));
                            }
                            "setMinutes" => {
                                let mut it = arg_vals.into_iter();
                                let nm = it.next().map(|v| v.to_number() as u32).unwrap_or(min);
                                let ns = it.next().map(|v| v.to_number() as u32).unwrap_or(sec);
                                let nms = it.next().map(|v| v.to_number() as u32).unwrap_or(ms_part);
                                let new_ms = parts_to_ms(yr, mo, day, hr, nm, ns, nms);
                                obj_rc2.borrow_mut().props.insert("__date_ms__".into(), JsValue::Number(new_ms));
                                return Ok(JsValue::Number(new_ms));
                            }
                            "setSeconds" => {
                                let mut it = arg_vals.into_iter();
                                let ns = it.next().map(|v| v.to_number() as u32).unwrap_or(sec);
                                let nms = it.next().map(|v| v.to_number() as u32).unwrap_or(ms_part);
                                let new_ms = parts_to_ms(yr, mo, day, hr, min, ns, nms);
                                obj_rc2.borrow_mut().props.insert("__date_ms__".into(), JsValue::Number(new_ms));
                                return Ok(JsValue::Number(new_ms));
                            }
                            "setMilliseconds" => {
                                let nms = arg_vals.into_iter().next().map(|v| v.to_number() as u32).unwrap_or(ms_part);
                                let new_ms = parts_to_ms(yr, mo, day, hr, min, sec, nms);
                                obj_rc2.borrow_mut().props.insert("__date_ms__".into(), JsValue::Number(new_ms));
                                return Ok(JsValue::Number(new_ms));
                            }
                            _ => {}
                        }
                    }
                    // ─── RegExp instance metody ───────────────────────────
                    if let Some((pat, flags)) = get_regex_parts(&JsValue::Object(Rc::clone(&obj_rc2))) {
                        match key.as_str() {
                            "test" => {
                                let arg_vals = self.eval_args(args, env)?;
                                let text = arg_vals.into_iter().next()
                                    .map(|v| v.to_string())
                                    .unwrap_or_default();
                                return Ok(JsValue::Bool(regex_test(&pat, &flags, &text)));
                            }
                            "exec" => {
                                let arg_vals = self.eval_args(args, env)?;
                                let text = arg_vals.into_iter().next()
                                    .map(|v| v.to_string())
                                    .unwrap_or_default();
                                match regex_exec_named(&pat, &flags, &text) {
                                    None => return Ok(JsValue::Null),
                                    Some((groups, named)) => {
                                        let arr: Vec<JsValue> = groups.into_iter()
                                            .map(|g| g.map(JsValue::Str).unwrap_or(JsValue::Undefined))
                                            .collect();
                                        let arr_val = JsValue::Array(Rc::new(RefCell::new(arr)));
                                        // Pripojime .groups objekt s named groups
                                        if !named.is_empty() {
                                            if let JsValue::Array(_) = &arr_val {
                                                // Array nemuze mit vlastni props - vratime vsak Array
                                                // Pro plnou kompatibilitu by .groups bylo na Array
                                                // Zatim pouzijeme: arr.groups = obj
                                                let mut groups_obj = JsObject::new();
                                                for (n, v) in named {
                                                    groups_obj.set(n, v.map(JsValue::Str).unwrap_or(JsValue::Undefined));
                                                }
                                                // Bohuzel arr je primo Array, ne Object - pripojime jako separatni
                                                // hodnotu pres specialni klic? Zatim vratime jen positional.
                                                let _ = groups_obj;
                                            }
                                        }
                                        return Ok(arr_val);
                                    }
                                }
                            }
                            "toString" => {
                                return Ok(JsValue::Str(format!("/{pat}/{flags}")));
                            }
                            _ => {} // Pust dal
                        }
                    }
                    // ─── Promise instance metody ───────────────────────────
                    if let Some((state, pval)) = {
                        let b = obj_rc2.borrow();
                        b.props.get("__promise_state__").and_then(|s| {
                            if let JsValue::Str(st) = s {
                                let v = b.props.get("__promise_value__").cloned().unwrap_or(JsValue::Undefined);
                                Some((st.clone(), v))
                            } else { None }
                        })
                    } {
                        match key.as_str() {
                            "then" => {
                                let arg_vals = self.eval_args(args, env)?;
                                let on_fulfilled = arg_vals.get(0).cloned().unwrap_or(JsValue::Undefined);
                                let on_rejected  = arg_vals.get(1).cloned().unwrap_or(JsValue::Undefined);
                                return match state.as_str() {
                                    "fulfilled" => {
                                        if matches!(on_fulfilled, JsValue::Function(_)) {
                                            match self.call_function(on_fulfilled, vec![pval], None) {
                                                Ok(r) => Ok(make_settled_promise("fulfilled",
                                                    unwrap_promise_result(r).unwrap_or_else(|v| v))),
                                                Err(JsError::Thrown(v)) => Ok(make_settled_promise("rejected", v)),
                                                Err(e) => Err(e),
                                            }
                                        } else {
                                            Ok(JsValue::Object(Rc::clone(&obj_rc2)))
                                        }
                                    }
                                    "rejected" => {
                                        if matches!(on_rejected, JsValue::Function(_)) {
                                            match self.call_function(on_rejected, vec![pval], None) {
                                                Ok(r) => Ok(make_settled_promise("fulfilled",
                                                    unwrap_promise_result(r).unwrap_or_else(|v| v))),
                                                Err(JsError::Thrown(v)) => Ok(make_settled_promise("rejected", v)),
                                                Err(e) => Err(e),
                                            }
                                        } else {
                                            Ok(JsValue::Object(Rc::clone(&obj_rc2)))
                                        }
                                    }
                                    _ => Ok(JsValue::Object(Rc::clone(&obj_rc2))),
                                };
                            }
                            "catch" => {
                                let arg_vals = self.eval_args(args, env)?;
                                let on_rejected = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                                return if state == "rejected" && matches!(on_rejected, JsValue::Function(_)) {
                                    match self.call_function(on_rejected, vec![pval], None) {
                                        Ok(r) => Ok(make_settled_promise("fulfilled",
                                            unwrap_promise_result(r).unwrap_or_else(|v| v))),
                                        Err(JsError::Thrown(v)) => Ok(make_settled_promise("rejected", v)),
                                        Err(e) => Err(e),
                                    }
                                } else {
                                    Ok(JsValue::Object(Rc::clone(&obj_rc2)))
                                };
                            }
                            "finally" => {
                                let arg_vals = self.eval_args(args, env)?;
                                let cb = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                                if matches!(cb, JsValue::Function(_)) {
                                    match self.call_function(cb, vec![], None) {
                                        Err(JsError::Thrown(v)) => return Ok(make_settled_promise("rejected", v)),
                                        Err(e) => return Err(e),
                                        Ok(_) => {}
                                    }
                                }
                                return Ok(JsValue::Object(Rc::clone(&obj_rc2)));
                            }
                            _ => {} // Pust dal na normalni object dispatch
                        }
                        let _ = pval; // suppress unused warning
                        let _ = state;
                    }
                    let arg_vals = self.eval_args(args, env)?;
                    match key.as_str() {
                        // obj.hasOwnProperty("key") - kontrola vlastni vlastnosti
                        "hasOwnProperty" => {
                            let k = arg_vals.into_iter().next()
                                .map(|v| v.to_string()).unwrap_or_default();
                            return Ok(JsValue::Bool(obj_rc2.borrow().has_own(&k)));
                        }
                        // obj.isPrototypeOf(other) - je this v proto retezci other?
                        "isPrototypeOf" => {
                            let target = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            return Ok(JsValue::Bool(is_in_proto_chain(&obj_rc2, &target)));
                        }
                        // obj.propertyIsEnumerable("key") - vlastni + ne-interni
                        "propertyIsEnumerable" => {
                            let k = arg_vals.into_iter().next()
                                .map(|v| v.to_string()).unwrap_or_default();
                            let is_enum = obj_rc2.borrow().has_own(&k) && !is_internal_key(&k);
                            return Ok(JsValue::Bool(is_enum));
                        }
                        "toString" => {
                            // Zkontroluj vlastni toString v props; jinak fallback
                            let custom = obj_rc2.borrow().props.get("toString").cloned();
                            if let Some(f) = custom {
                                return self.call_function(f, arg_vals, Some(this));
                            }
                            return Ok(JsValue::Str("[object Object]".into()));
                        }
                        "valueOf"  => return Ok(JsValue::Object(Rc::clone(&obj_rc2))),
                        _ => {
                            // Normalni method call
                            let func = self.get_prop(&this, &key)?;
                            return self.call_function(func, arg_vals, Some(this));
                        }
                    }
                }
                JsValue::Array(arr_rc) => {
                    let arr_rc = Rc::clone(arr_rc);
                    let arg_vals = self.eval_args(args, env)?;
                    if let Some(result) = self.call_array_method(arr_rc, &key, arg_vals)? {
                        return Ok(result);
                    }
                }
                // ─── Number instance metody ────────────────────────────────
                JsValue::Number(n) => {
                    let n = *n;
                    let arg_vals = self.eval_args(args, env)?;
                    match key.as_str() {
                        "toFixed" => {
                            let digits = arg_vals.first().map(|v| v.to_number() as usize).unwrap_or(0);
                            return Ok(JsValue::Str(format!("{:.prec$}", n, prec = digits)));
                        }
                        "toPrecision" => {
                            let digits = arg_vals.first().map(|v| v.to_number() as usize).unwrap_or(1);
                            return Ok(JsValue::Str(format!("{:.prec$}", n, prec = if digits > 0 { digits - 1 } else { 0 })));
                        }
                        "toExponential" => {
                            let digits = arg_vals.first().map(|v| v.to_number() as usize).unwrap_or(6);
                            // Rust `{:e}` = "1.23e5", JS chce "1.23e+5"
                            let s = format!("{:.prec$e}", n, prec = digits);
                            // Pridame '+' pred kladny exponent
                            let s = if let Some(e_pos) = s.find('e') {
                                let (mantissa, exp_part) = s.split_at(e_pos);
                                let exp_str = &exp_part[1..]; // bez 'e'
                                if exp_str.starts_with('-') {
                                    format!("{}e{}", mantissa, exp_str)
                                } else {
                                    format!("{}e+{}", mantissa, exp_str)
                                }
                            } else { s };
                            return Ok(JsValue::Str(s));
                        }
                        "toString" => {
                            let radix = arg_vals.first().map(|v| v.to_number() as u32).unwrap_or(10);
                            if radix == 10 || radix == 0 {
                                return Ok(JsValue::Str(JsValue::Number(n).to_string()));
                            }
                            let radix = radix.min(36).max(2);
                            if n.fract() == 0.0 && n.is_finite() {
                                let i = n as i64;
                                return Ok(JsValue::Str(if i < 0 {
                                    format!("-{}", radix_string(-i as u64, radix))
                                } else {
                                    radix_string(i as u64, radix)
                                }));
                            }
                            return Ok(JsValue::Str(n.to_string()));
                        }
                        "valueOf" => return Ok(JsValue::Number(n)),
                        "toLocaleString" => {
                            // Volitelny prvni argument: locale string
                            let locale = arg_vals.first().map(|v| v.to_string());
                            return Ok(JsValue::Str(match locale {
                                Some(loc) => format_number_intl(n, &loc),
                                None      => format_number_locale(n),
                            }));
                        }
                        _ => {}
                    }
                }
                // ─── BigNumber instance metody ────────────────────────────
                JsValue::BigNumber(bn) => {
                    let bn = Rc::clone(bn);
                    let arg_vals = self.eval_args(args, env)?;
                    let other_bd = arg_vals.first().and_then(|v| v.to_bigdecimal());
                    return match key.as_str() {
                        "plus"      => Ok(JsValue::BigNumber(Rc::new((*bn).clone() + other_bd.unwrap_or(BigDecimal::from(0))))),
                        "minus"     => Ok(JsValue::BigNumber(Rc::new((*bn).clone() - other_bd.unwrap_or(BigDecimal::from(0))))),
                        "times"     => Ok(JsValue::BigNumber(Rc::new((*bn).clone() * other_bd.unwrap_or(BigDecimal::from(1))))),
                        "multipliedBy" => Ok(JsValue::BigNumber(Rc::new((*bn).clone() * other_bd.unwrap_or(BigDecimal::from(1))))),
                        "div" | "dividedBy" => {
                            let d = other_bd.unwrap_or(BigDecimal::from(1));
                            if d.is_zero() { return Ok(JsValue::Number(f64::NAN)); }
                            Ok(JsValue::BigNumber(Rc::new(bn.as_ref().clone() / d)))
                        }
                        "mod" | "modulo" => {
                            let d = other_bd.unwrap_or(BigDecimal::from(1));
                            if d.is_zero() { return Ok(JsValue::Number(f64::NAN)); }
                            Ok(JsValue::BigNumber(Rc::new(bn.as_ref().clone() % d)))
                        }
                        "pow" | "exponentiatedBy" => {
                            let exp = other_bd.and_then(|d| d.to_u64()).unwrap_or(0);
                            Ok(JsValue::BigNumber(Rc::new(bigdecimal_pow(bn.as_ref().clone(), exp))))
                        }
                        "abs"       => Ok(JsValue::BigNumber(Rc::new(bn.abs()))),
                        "negated"   => Ok(JsValue::BigNumber(Rc::new(-bn.as_ref().clone()))),
                        "sqrt"      => Ok(JsValue::BigNumber(Rc::new(bn.sqrt().unwrap_or(BigDecimal::from(0))))),
                        "toNumber"  => Ok(JsValue::Number(bn.to_f64().unwrap_or(f64::NAN))),
                        "toString"  => Ok(JsValue::Str(bn.to_string())),
                        "toFixed"   => {
                            let digits = arg_vals.first().map(|v| v.to_number() as usize).unwrap_or(0);
                            Ok(JsValue::Str(bn.round(digits as i64).to_string()))
                        }
                        "toPrecision" => {
                            let digits = arg_vals.first().map(|v| v.to_number() as usize).unwrap_or(0);
                            Ok(JsValue::Str(bn.round(digits as i64).to_string()))
                        }
                        "isZero"     => Ok(JsValue::Bool(bn.is_zero())),
                        "isPositive" => Ok(JsValue::Bool(*bn > BigDecimal::from(0))),
                        "isNegative" => Ok(JsValue::Bool(*bn < BigDecimal::from(0))),
                        "isFinite"   => Ok(JsValue::Bool(true)),
                        "isNaN"      => Ok(JsValue::Bool(false)),
                        "isInteger"  => Ok(JsValue::Bool(bn.is_integer())),
                        "gt" | "isGreaterThan"           => Ok(JsValue::Bool((*bn) > other_bd.unwrap_or(BigDecimal::from(0)))),
                        "gte" | "isGreaterThanOrEqualTo" => Ok(JsValue::Bool((*bn) >= other_bd.unwrap_or(BigDecimal::from(0)))),
                        "lt" | "isLessThan"              => Ok(JsValue::Bool((*bn) < other_bd.unwrap_or(BigDecimal::from(0)))),
                        "lte" | "isLessThanOrEqualTo"    => Ok(JsValue::Bool((*bn) <= other_bd.unwrap_or(BigDecimal::from(0)))),
                        "eq" | "isEqualTo"               => Ok(JsValue::Bool((*bn) == other_bd.unwrap_or(BigDecimal::from(0)))),
                        "comparedTo" => {
                            let other = other_bd.unwrap_or(BigDecimal::from(0));
                            let cmp = if *bn < other { -1.0 } else if *bn > other { 1.0 } else { 0.0 };
                            Ok(JsValue::Number(cmp))
                        }
                        "decimalPlaces" | "dp" => {
                            let s = bn.to_string();
                            let dp = s.find('.').map(|i| s.len() - i - 1).unwrap_or(0);
                            Ok(JsValue::Number(dp as f64))
                        }
                        "integerValue" => Ok(JsValue::BigNumber(Rc::new(bn.round(0)))),
                        "shiftedBy" => {
                            let n = arg_vals.first().map(|v| v.to_number() as i64).unwrap_or(0);
                            let factor = bigdecimal_pow(BigDecimal::from(10i64), n.unsigned_abs());
                            let result = if n >= 0 { bn.as_ref().clone() * factor } else { bn.as_ref().clone() / factor };
                            Ok(JsValue::BigNumber(Rc::new(result)))
                        }
                        "valueOf" => Ok(JsValue::Number(bn.to_f64().unwrap_or(f64::NAN))),
                        _ => Ok(JsValue::Undefined),
                    };
                }
                // ─── BigInt instance metody ────────────────────────────────
                JsValue::BigInt(bn) => {
                    let bn = Rc::clone(bn);
                    let arg_vals = self.eval_args(args, env)?;
                    return match key.as_str() {
                        "toString" => {
                            let radix = arg_vals.first().map(|v| v.to_number() as u32).unwrap_or(10);
                            let radix = radix.clamp(2, 36);
                            Ok(JsValue::Str(bn.to_str_radix(radix)))
                        }
                        "toLocaleString" => Ok(JsValue::Str(bn.to_string())),
                        "valueOf" => Ok(JsValue::BigInt(bn)),
                        _ => Ok(JsValue::Undefined),
                    };
                }
                // ─── DomNode metody (real browser::dom Node) ─────────────
                JsValue::DomNode(node_rc) => {
                    use crate::browser::dom::NodeData;
                    let n = Rc::clone(node_rc);
                    let arg_vals = self.eval_args(args, env)?;
                    match key.as_str() {
                        "getAttribute" => {
                            let name = arg_vals.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
                            return Ok(match n.attr(&name) {
                                Some(v) => JsValue::Str(v),
                                None    => JsValue::Null,
                            });
                        }
                        "setAttribute" => {
                            let mut iter = arg_vals.into_iter();
                            let attr_name = iter.next().map(|v| v.to_string()).unwrap_or_default();
                            let attr_val = iter.next().map(|v| v.to_string()).unwrap_or_default();
                            let old_val = n.attr(&attr_name).unwrap_or_default();
                            n.set_attr(&attr_name, &attr_val);
                            // MutationObserver dispatch
                            self.dispatch_mutation(&n, "attributes",
                                Some(attr_name.clone()), Some(old_val.clone()));
                            // Lifecycle: attributeChangedCallback pro custom elements
                            let node_ptr = Rc::as_ptr(&n) as usize;
                            let instance = self.custom_element_instances.borrow().get(&node_ptr).cloned();
                            if let Some(inst) = instance {
                                let cb = if let JsValue::Object(o) = &inst {
                                    o.borrow().props.get("attributeChangedCallback").cloned()
                                } else { None };
                                if let Some(f) = cb {
                                    let _ = self.call_function(f, vec![
                                        JsValue::Str(attr_name),
                                        JsValue::Str(old_val),
                                        JsValue::Str(attr_val),
                                    ], Some(inst));
                                }
                            }
                            return Ok(JsValue::Undefined);
                        }
                        "removeAttribute" => {
                            let name = arg_vals.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
                            n.remove_attr(&name);
                            return Ok(JsValue::Undefined);
                        }
                        "hasAttribute" => {
                            let name = arg_vals.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
                            return Ok(JsValue::Bool(n.has_attr(&name)));
                        }
                        "appendChild" => {
                            let child = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            if let JsValue::DomNode(c) = &child {
                                n.append_child(Rc::clone(c));
                                // MutationObserver dispatch on parent s addedNodes.
                                self.dispatch_mutation_childlist(&n, vec![Rc::clone(c)], Vec::new());
                                // Lifecycle: connectedCallback
                                let child_ptr = Rc::as_ptr(c) as usize;
                                let instance = self.custom_element_instances.borrow().get(&child_ptr).cloned();
                                if let Some(inst) = instance {
                                    let cb = if let JsValue::Object(o) = &inst {
                                        o.borrow().props.get("connectedCallback").cloned()
                                    } else { None };
                                    if let Some(f) = cb {
                                        let _ = self.call_function(f, vec![], Some(inst));
                                    }
                                }
                            }
                            return Ok(child);
                        }
                        "removeChild" => {
                            let child = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            if let JsValue::DomNode(c) = &child {
                                // Lifecycle: disconnectedCallback
                                let child_ptr = Rc::as_ptr(c) as usize;
                                let instance = self.custom_element_instances.borrow().get(&child_ptr).cloned();
                                if let Some(inst) = instance {
                                    let cb = if let JsValue::Object(o) = &inst {
                                        o.borrow().props.get("disconnectedCallback").cloned()
                                    } else { None };
                                    if let Some(f) = cb {
                                        let _ = self.call_function(f, vec![], Some(inst));
                                    }
                                }
                                n.children.borrow_mut().retain(|x| !Rc::ptr_eq(x, c));
                                // MutationObserver dispatch s removedNodes.
                                self.dispatch_mutation_childlist(&n, Vec::new(), vec![Rc::clone(c)]);
                            }
                            return Ok(child);
                        }
                        "matches" => {
                            let sel = arg_vals.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
                            let parsed = crate::browser::css_parser::parse_selectors(&sel);
                            let any = parsed.iter().any(|s| crate::browser::cascade::matches_selector(&n, s));
                            return Ok(JsValue::Bool(any));
                        }
                        "closest" => {
                            let sel = arg_vals.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
                            let parsed = crate::browser::css_parser::parse_selectors(&sel);
                            let mut current = Some(Rc::clone(&n));
                            while let Some(c) = current {
                                if parsed.iter().any(|s| crate::browser::cascade::matches_selector(&c, s)) {
                                    return Ok(JsValue::DomNode(c));
                                }
                                current = c.parent.borrow().upgrade();
                            }
                            return Ok(JsValue::Null);
                        }
                        "getBoundingClientRect" => {
                            // Lookup layout rect pres host callback; pri absenci vrati 0,0,0,0.
                            let (x, y, w, h) = self.lookup_layout_rect(&n).unwrap_or((0.0, 0.0, 0.0, 0.0));
                            let mut rect = JsObject::new();
                            rect.set("x".into(),      JsValue::Number(x as f64));
                            rect.set("y".into(),      JsValue::Number(y as f64));
                            rect.set("width".into(),  JsValue::Number(w as f64));
                            rect.set("height".into(), JsValue::Number(h as f64));
                            rect.set("top".into(),    JsValue::Number(y as f64));
                            rect.set("left".into(),   JsValue::Number(x as f64));
                            rect.set("right".into(),  JsValue::Number((x + w) as f64));
                            rect.set("bottom".into(), JsValue::Number((y + h) as f64));
                            return Ok(JsValue::Object(Rc::new(RefCell::new(rect))));
                        }
                        "getClientRects" => {
                            // Single-rect approximation (spec by mela vratit per-line rects pro inline).
                            let (x, y, w, h) = self.lookup_layout_rect(&n).unwrap_or((0.0, 0.0, 0.0, 0.0));
                            let mut rect = JsObject::new();
                            rect.set("x".into(),      JsValue::Number(x as f64));
                            rect.set("y".into(),      JsValue::Number(y as f64));
                            rect.set("width".into(),  JsValue::Number(w as f64));
                            rect.set("height".into(), JsValue::Number(h as f64));
                            rect.set("top".into(),    JsValue::Number(y as f64));
                            rect.set("left".into(),   JsValue::Number(x as f64));
                            rect.set("right".into(),  JsValue::Number((x + w) as f64));
                            rect.set("bottom".into(), JsValue::Number((y + h) as f64));
                            let arr = vec![JsValue::Object(Rc::new(RefCell::new(rect)))];
                            return Ok(JsValue::Array(Rc::new(RefCell::new(arr))));
                        }
                        "submit" if n.tag_name().as_deref() == Some("form") => {
                            // Dispatch 'submit' SubmitEvent na form pred actual fetch
                            // Pokud listener zavola preventDefault, fetch neproveden.
                            let mut event_obj = JsObject::new();
                            event_obj.set("type".into(), JsValue::Str("submit".into()));
                            event_obj.set("target".into(), JsValue::DomNode(Rc::clone(&n)));
                            event_obj.set("currentTarget".into(), JsValue::DomNode(Rc::clone(&n)));
                            event_obj.set("bubbles".into(), JsValue::Bool(true));
                            event_obj.set("cancelable".into(), JsValue::Bool(true));
                            let prevented = Rc::new(RefCell::new(false));
                            let prevented_clone = Rc::clone(&prevented);
                            event_obj.set("preventDefault".into(),
                                native("preventDefault", move |_| {
                                    *prevented_clone.borrow_mut() = true;
                                    Ok(JsValue::Undefined)
                                }));
                            event_obj.set("stopPropagation".into(),
                                native("stopPropagation", |_| Ok(JsValue::Undefined)));
                            event_obj.set("defaultPrevented".into(), JsValue::Bool(false));
                            let event_val = JsValue::Object(Rc::new(RefCell::new(event_obj)));
                            // Volat listenery pres existing dispatch
                            let _ = self.dispatch_event(&n, "submit", event_val);
                            if *prevented.borrow() {
                                self.console_log.borrow_mut().push((
                                    "log".into(),
                                    "[form submit] prevented by listener".into(),
                                ));
                                return Ok(JsValue::Undefined);
                            }
                            // Collect form data (name=value pairs from inputs)
                            let action = n.attr("action").unwrap_or_else(|| "/".to_string());
                            let method = n.attr("method").unwrap_or_else(|| "GET".to_string()).to_uppercase();
                            let mut pairs: Vec<(String, String)> = Vec::new();
                            n.walk(&mut |node| {
                                if Rc::ptr_eq(node, &n) { return; }
                                if let Some(t) = node.tag_name() {
                                    if matches!(t.as_str(), "input" | "select" | "textarea") {
                                        if let Some(name) = node.attr("name") {
                                            let value = node.attr("value").unwrap_or_default();
                                            pairs.push((name, value));
                                        }
                                    }
                                }
                            });
                            // URL encode body
                            let body = pairs.iter()
                                .map(|(k, v)| format!("{}={}",
                                    url_encode(k), url_encode(v)))
                                .collect::<Vec<_>>().join("&");
                            // Real fetch pres ureq pokud HTTP(S) URL
                            let mut status: u16 = 0;
                            if action.starts_with("http://") || action.starts_with("https://") {
                                let req_result = if method == "POST" {
                                    ureq::post(&action)
                                        .set("Content-Type", "application/x-www-form-urlencoded")
                                        .send_string(&body)
                                } else {
                                    let url = if body.is_empty() { action.clone() }
                                              else { format!("{action}?{body}") };
                                    ureq::get(&url).call()
                                };
                                status = match &req_result {
                                    Ok(r) => r.status(),
                                    Err(ureq::Error::Status(s, _)) => *s,
                                    Err(_) => 0,
                                };
                            }
                            self.console_log.borrow_mut().push((
                                "log".into(),
                                format!("[form submit] {method} {action} body={body} status={status}"),
                            ));
                            self.network_log.borrow_mut().push((
                                format!("{method} {action}"), status,
                            ));
                            return Ok(JsValue::Undefined);
                        }
                        "getContext" if n.tag_name().as_deref() == Some("canvas") => {
                            let kind = arg_vals.into_iter().next().map(|v| v.to_string()).unwrap_or_else(|| "2d".into());
                            let canvas_ptr = Rc::as_ptr(&n) as usize;
                            if kind == "webgl" || kind == "webgl2" || kind == "experimental-webgl" {
                                // Sdileny WebGLState - znovu pouzij existujici (idempotent getContext)
                                let state = {
                                    let mut states = self.webgl_states.borrow_mut();
                                    states.entry(canvas_ptr)
                                        .or_insert_with(|| Rc::new(RefCell::new(WebGLState::new())))
                                        .clone()
                                };
                                return Ok(webgl::create_webgl_context(state));
                            }
                            // Default 2D
                            let ctx = canvas::create_canvas_2d_context(canvas_ptr, Rc::clone(&self.canvas_ops));
                            return Ok(ctx);
                        }
                        "scrollIntoView" | "scroll" | "scrollBy" | "scrollTo"
                            => {
                            // No-op
                            return Ok(JsValue::Undefined);
                        }
                        // ─── Element extras ─────────────────────────────────
                        "checkVisibility" => {
                            // CSS Display L4 - kontrola visibility (display:none, visibility:hidden, opacity:0)
                            let opts = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            let check_opacity = if let JsValue::Object(o) = &opts {
                                matches!(o.borrow().get("checkOpacity"), JsValue::Bool(true))
                            } else { false };
                            let check_visibility_css = if let JsValue::Object(o) = &opts {
                                matches!(o.borrow().get("checkVisibilityCSS"), JsValue::Bool(true))
                            } else { false };
                            let style = n.attr("style").unwrap_or_default();
                            if style.contains("display:none") || style.contains("display: none") {
                                return Ok(JsValue::Bool(false));
                            }
                            if check_visibility_css && (style.contains("visibility:hidden") || style.contains("visibility: hidden")) {
                                return Ok(JsValue::Bool(false));
                            }
                            if check_opacity && style.contains("opacity:0") {
                                return Ok(JsValue::Bool(false));
                            }
                            return Ok(JsValue::Bool(true));
                        }
                        "requestFullscreen" => {
                            n.set_attr("data-fullscreen", "true");
                            return Ok(make_settled_promise("fulfilled", JsValue::Undefined));
                        }
                        "requestPointerLock" => {
                            n.set_attr("data-pointer-lock", "true");
                            return Ok(JsValue::Undefined);
                        }
                        "computedStyleMap" => {
                            // CSS Typed OM stub - vrati objekt s get/has/set
                            let map = Rc::new(RefCell::new(JsObject::new()));
                            map.borrow_mut().set("get".into(), native("get", |_| Ok(JsValue::Undefined)));
                            map.borrow_mut().set("has".into(), native("has", |_| Ok(JsValue::Bool(false))));
                            map.borrow_mut().set("set".into(), native("set", |_| Ok(JsValue::Undefined)));
                            map.borrow_mut().set("size".into(), JsValue::Number(0.0));
                            return Ok(JsValue::Object(map));
                        }
                        "attachInternals" => {
                            // ElementInternals - pro custom elements form participation
                            let internals = Rc::new(RefCell::new(JsObject::new()));
                            internals.borrow_mut().set("__element_internals__".into(), JsValue::Bool(true));
                            internals.borrow_mut().set("form".into(), JsValue::Null);
                            internals.borrow_mut().set("labels".into(), JsValue::Array(Rc::new(RefCell::new(Vec::new()))));
                            internals.borrow_mut().set("validity".into(), {
                                let v = Rc::new(RefCell::new(JsObject::new()));
                                v.borrow_mut().set("valid".into(), JsValue::Bool(true));
                                v.borrow_mut().set("valueMissing".into(), JsValue::Bool(false));
                                v.borrow_mut().set("typeMismatch".into(), JsValue::Bool(false));
                                JsValue::Object(v)
                            });
                            internals.borrow_mut().set("setFormValue".into(),
                                native("setFormValue", |_| Ok(JsValue::Undefined)));
                            internals.borrow_mut().set("setValidity".into(),
                                native("setValidity", |_| Ok(JsValue::Undefined)));
                            internals.borrow_mut().set("checkValidity".into(),
                                native("checkValidity", |_| Ok(JsValue::Bool(true))));
                            internals.borrow_mut().set("reportValidity".into(),
                                native("reportValidity", |_| Ok(JsValue::Bool(true))));
                            return Ok(JsValue::Object(internals));
                        }
                        // ─── Popover API (HTML L1) ─────────────────────────
                        "showPopover" => {
                            n.set_attr("data-popover-open", "true");
                            return Ok(JsValue::Undefined);
                        }
                        "hidePopover" => {
                            n.remove_attr("data-popover-open");
                            return Ok(JsValue::Undefined);
                        }
                        "togglePopover" => {
                            if n.has_attr("data-popover-open") {
                                n.remove_attr("data-popover-open");
                                return Ok(JsValue::Bool(false));
                            } else {
                                n.set_attr("data-popover-open", "true");
                                return Ok(JsValue::Bool(true));
                            }
                        }
                        // ─── Shadow DOM (attachShadow + shadowRoot) ────────
                        "attachShadow" => {
                            let init = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            let mode = if let JsValue::Object(o) = &init {
                                o.borrow().get("mode").to_string()
                            } else { "open".into() };
                            // Shadow root - separe DOM strom (zatim plain JsObject simulace)
                            let shadow = Rc::new(RefCell::new(JsObject::new()));
                            shadow.borrow_mut().set("__shadow_root__".into(), JsValue::Bool(true));
                            shadow.borrow_mut().set("mode".into(), JsValue::Str(mode));
                            shadow.borrow_mut().set("host".into(), JsValue::DomNode(Rc::clone(&n)));
                            shadow.borrow_mut().set("childNodes".into(),
                                JsValue::Array(Rc::new(RefCell::new(Vec::new()))));
                            shadow.borrow_mut().set("innerHTML".into(), JsValue::Str(String::new()));
                            shadow.borrow_mut().set("appendChild".into(),
                                native("appendChild", |args| Ok(args.into_iter().next().unwrap_or(JsValue::Undefined))));
                            shadow.borrow_mut().set("querySelector".into(),
                                native("querySelector", |_| Ok(JsValue::Null)));
                            shadow.borrow_mut().set("querySelectorAll".into(),
                                native("querySelectorAll", |_| Ok(JsValue::Array(Rc::new(RefCell::new(Vec::new()))))));
                            shadow.borrow_mut().set("getElementById".into(),
                                native("getElementById", |_| Ok(JsValue::Null)));
                            shadow.borrow_mut().set("adoptedStyleSheets".into(),
                                JsValue::Array(Rc::new(RefCell::new(Vec::new()))));
                            // Ulozit shadow root ID na node atribut
                            n.set_attr("data-shadow-root", "true");
                            return Ok(JsValue::Object(shadow));
                        }
                        // ─── Web Animations API: Element.animate(keyframes, options) ──
                        "animate" => {
                            let mut it = arg_vals.into_iter();
                            let keyframes = it.next().unwrap_or(JsValue::Undefined);
                            let options = it.next().unwrap_or(JsValue::Undefined);
                            let anim = Rc::new(RefCell::new(JsObject::new()));
                            anim.borrow_mut().set("__animation__".into(), JsValue::Bool(true));
                            anim.borrow_mut().set("playState".into(), JsValue::Str("running".into()));
                            anim.borrow_mut().set("currentTime".into(), JsValue::Number(0.0));
                            anim.borrow_mut().set("startTime".into(), JsValue::Number(now_ms()));
                            anim.borrow_mut().set("playbackRate".into(), JsValue::Number(1.0));
                            anim.borrow_mut().set("effect".into(), JsValue::Object({
                                let eff = Rc::new(RefCell::new(JsObject::new()));
                                eff.borrow_mut().set("target".into(), JsValue::DomNode(Rc::clone(&n)));
                                eff.borrow_mut().set("keyframes".into(), keyframes);
                                eff.borrow_mut().set("timing".into(), options);
                                eff
                            }));
                            anim.borrow_mut().set("play".into(),
                                native("play", |_| Ok(JsValue::Undefined)));
                            anim.borrow_mut().set("pause".into(),
                                native("pause", |_| Ok(JsValue::Undefined)));
                            anim.borrow_mut().set("cancel".into(),
                                native("cancel", |_| Ok(JsValue::Undefined)));
                            anim.borrow_mut().set("finish".into(),
                                native("finish", |_| Ok(JsValue::Undefined)));
                            anim.borrow_mut().set("reverse".into(),
                                native("reverse", |_| Ok(JsValue::Undefined)));
                            // Promise-like .finished / .ready
                            anim.borrow_mut().set("finished".into(),
                                make_settled_promise("fulfilled", JsValue::Undefined));
                            anim.borrow_mut().set("ready".into(),
                                make_settled_promise("fulfilled", JsValue::Undefined));
                            anim.borrow_mut().set("addEventListener".into(),
                                native("addEventListener", |_| Ok(JsValue::Undefined)));
                            return Ok(JsValue::Object(anim));
                        }
                        "getAnimations" => {
                            return Ok(JsValue::Array(Rc::new(RefCell::new(Vec::new()))));
                        }
                        // HTMLDialogElement
                        "show" if n.tag_name().as_deref() == Some("dialog") => {
                            n.set_attr("open", "");
                            return Ok(JsValue::Undefined);
                        }
                        "showModal" if n.tag_name().as_deref() == Some("dialog") => {
                            n.set_attr("open", "");
                            n.set_attr("aria-modal", "true");
                            // Dispatch 'show' event (custom)
                            let mut event = JsObject::new();
                            event.set("type".into(), JsValue::Str("show".into()));
                            event.set("target".into(), JsValue::DomNode(Rc::clone(&n)));
                            let _ = self.dispatch_event(&n, "show",
                                JsValue::Object(Rc::new(RefCell::new(event))));
                            return Ok(JsValue::Undefined);
                        }
                        "close" if n.tag_name().as_deref() == Some("dialog") => {
                            // Optional return value
                            let return_val = arg_vals.into_iter().next().map(|v| v.to_string());
                            n.remove_attr("open");
                            n.remove_attr("aria-modal");
                            if let Some(rv) = &return_val {
                                n.set_attr("returnValue", rv);
                            }
                            // Dispatch 'close' event
                            let mut event = JsObject::new();
                            event.set("type".into(), JsValue::Str("close".into()));
                            event.set("target".into(), JsValue::DomNode(Rc::clone(&n)));
                            if let Some(rv) = return_val {
                                event.set("returnValue".into(), JsValue::Str(rv));
                            }
                            let _ = self.dispatch_event(&n, "close",
                                JsValue::Object(Rc::new(RefCell::new(event))));
                            return Ok(JsValue::Undefined);
                        }
                        // HTMLMediaElement (video / audio)
                        "play" | "pause" | "load" if matches!(n.tag_name().as_deref(), Some("video") | Some("audio")) => {
                            // Pri play, pause aspon set/remove "paused" attr (semantically se chovaji)
                            match key.as_str() {
                                "play" => { n.remove_attr("paused"); }
                                "pause" => { n.set_attr("paused", ""); }
                                _ => {}
                            }
                            return Ok(JsValue::Undefined);
                        }
                        // HTMLInputElement
                        "select" | "setSelectionRange" | "setCustomValidity" | "checkValidity"
                        | "reportValidity" | "stepUp" | "stepDown"
                            if matches!(n.tag_name().as_deref(), Some("input") | Some("textarea") | Some("select")) => {
                            return Ok(JsValue::Bool(true));
                        }
                        "toggleAttribute" => {
                            let name = arg_vals.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
                            if n.attr(&name).is_some() {
                                n.remove_attr(&name);
                                return Ok(JsValue::Bool(false));
                            } else {
                                n.set_attr(&name, "");
                                return Ok(JsValue::Bool(true));
                            }
                        }
                        "cloneNode" => {
                            // Clone node deep (zjednodusene pres serialize -> parse fragment)
                            let html = serialize::serialize_outer_html(&n);
                            let frag = crate::browser::html_parser::parse_html_fragment(&html);
                            let frag_children: Vec<_> = frag.children.borrow().clone();
                            // Najdi prvni element child
                            for ch in &frag_children {
                                let body_children: Vec<_> = ch.children.borrow().clone();
                                if let Some(b) = body_children.into_iter().next() {
                                    return Ok(JsValue::DomNode(b));
                                }
                            }
                            return Ok(JsValue::DomNode(Rc::clone(&n)));
                        }
                        "contains" => {
                            let other = arg_vals.into_iter().next().unwrap_or(JsValue::Null);
                            if let JsValue::DomNode(o) = other {
                                let mut found = false;
                                n.walk(&mut |node| { if Rc::ptr_eq(node, &o) { found = true; } });
                                return Ok(JsValue::Bool(found));
                            }
                            return Ok(JsValue::Bool(false));
                        }
                        "append" => {
                            // Append vsechny DomNode args jako children + Strings jako text nodes
                            for arg in arg_vals {
                                match arg {
                                    JsValue::DomNode(c) => { n.append_child(c); }
                                    JsValue::Str(s) => {
                                        n.append_child(crate::browser::dom::NodeData::new_text(&s));
                                    }
                                    _ => {}
                                }
                            }
                            return Ok(JsValue::Undefined);
                        }
                        "prepend" => {
                            let mut new_first: Vec<Rc<crate::browser::dom::NodeData>> = Vec::new();
                            for arg in arg_vals {
                                match arg {
                                    JsValue::DomNode(c) => new_first.push(c),
                                    JsValue::Str(s) => new_first.push(crate::browser::dom::NodeData::new_text(&s)),
                                    _ => {}
                                }
                            }
                            let mut children = n.children.borrow_mut();
                            for (i, c) in new_first.into_iter().enumerate() {
                                children.insert(i, c);
                            }
                            return Ok(JsValue::Undefined);
                        }
                        "before" | "after" | "replaceWith" => {
                            let parent = match n.parent.borrow().upgrade() {
                                Some(p) => p, None => return Ok(JsValue::Undefined),
                            };
                            let mut new_nodes: Vec<Rc<crate::browser::dom::NodeData>> = Vec::new();
                            for arg in arg_vals {
                                match arg {
                                    JsValue::DomNode(c) => new_nodes.push(c),
                                    JsValue::Str(s) => new_nodes.push(crate::browser::dom::NodeData::new_text(&s)),
                                    _ => {}
                                }
                            }
                            let mut children = parent.children.borrow_mut();
                            let idx = children.iter().position(|c| Rc::ptr_eq(c, &n));
                            if let Some(i) = idx {
                                let insert_at = match key.as_str() {
                                    "before" => i,
                                    "after" => i + 1,
                                    _ /* replaceWith */ => {
                                        children.remove(i);
                                        i
                                    }
                                };
                                for (k, c) in new_nodes.into_iter().enumerate() {
                                    children.insert(insert_at + k, c);
                                }
                            }
                            return Ok(JsValue::Undefined);
                        }
                        "remove" => {
                            // Element.remove() - pasivne odstrani z parenta
                            if let Some(parent) = n.parent.borrow().upgrade() {
                                let mut children = parent.children.borrow_mut();
                                children.retain(|c| !Rc::ptr_eq(c, &n));
                            }
                            return Ok(JsValue::Undefined);
                        }
                        "insertAdjacentHTML" => {
                            let mut it = arg_vals.into_iter();
                            let position = it.next().map(|v| v.to_string()).unwrap_or_default();
                            let html = it.next().map(|v| v.to_string()).unwrap_or_default();
                            let frag = crate::browser::html_parser::parse_html_fragment(&html);
                            // Vytahnu nove nody (odznacka <html><body>... struktur)
                            let mut new_nodes: Vec<Rc<crate::browser::dom::NodeData>> = Vec::new();
                            for ch in frag.children.borrow().iter() {
                                for grandch in ch.children.borrow().iter() {
                                    new_nodes.push(Rc::clone(grandch));
                                }
                            }
                            match position.as_str() {
                                "beforebegin" => {
                                    if let Some(p) = n.parent.borrow().upgrade() {
                                        let mut c = p.children.borrow_mut();
                                        if let Some(i) = c.iter().position(|x| Rc::ptr_eq(x, &n)) {
                                            for (k, nn) in new_nodes.into_iter().enumerate() {
                                                c.insert(i + k, nn);
                                            }
                                        }
                                    }
                                }
                                "afterbegin" => {
                                    let mut c = n.children.borrow_mut();
                                    for (k, nn) in new_nodes.into_iter().enumerate() {
                                        c.insert(k, nn);
                                    }
                                }
                                "beforeend" => {
                                    for nn in new_nodes { n.append_child(nn); }
                                }
                                "afterend" => {
                                    if let Some(p) = n.parent.borrow().upgrade() {
                                        let mut c = p.children.borrow_mut();
                                        if let Some(i) = c.iter().position(|x| Rc::ptr_eq(x, &n)) {
                                            for (k, nn) in new_nodes.into_iter().enumerate() {
                                                c.insert(i + 1 + k, nn);
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                            return Ok(JsValue::Undefined);
                        }
                        "querySelector" => {
                            let sel = arg_vals.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
                            let result = if let Some(id) = sel.strip_prefix('#') {
                                n.get_element_by_id(id)
                            } else if let Some(cls) = sel.strip_prefix('.') {
                                n.get_elements_by_class(cls).into_iter().next()
                            } else {
                                n.get_elements_by_tag(&sel).into_iter().next()
                            };
                            return Ok(match result {
                                Some(node) => JsValue::DomNode(node),
                                None       => JsValue::Null,
                            });
                        }
                        "getElementsByTagName" => {
                            let tag = arg_vals.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
                            let nodes = n.get_elements_by_tag(&tag);
                            let arr: Vec<JsValue> = nodes.into_iter().map(JsValue::DomNode).collect();
                            return Ok(JsValue::Array(Rc::new(RefCell::new(arr))));
                        }
                        "getElementsByClassName" => {
                            let cls = arg_vals.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
                            let nodes = n.get_elements_by_class(&cls);
                            let arr: Vec<JsValue> = nodes.into_iter().map(JsValue::DomNode).collect();
                            return Ok(JsValue::Array(Rc::new(RefCell::new(arr))));
                        }
                        "addEventListener" => {
                            let mut iter = arg_vals.into_iter();
                            let event_type = iter.next().map(|v| v.to_string()).unwrap_or_default();
                            let callback = iter.next().unwrap_or(JsValue::Undefined);
                            let id = {
                                let mut c = self.next_callback_id.borrow_mut();
                                let id = *c;
                                *c += 1;
                                id
                            };
                            self.event_callbacks.borrow_mut().insert(id, callback);
                            n.listeners.borrow_mut().entry(event_type).or_default().push(id);
                            return Ok(JsValue::Undefined);
                        }
                        "removeEventListener" => {
                            // Bez ID nelze najit konkretni callback - zatim no-op
                            return Ok(JsValue::Undefined);
                        }
                        "dispatchEvent" => {
                            let event = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            let event_type = if let JsValue::Object(eo) = &event {
                                match eo.borrow().get("type") {
                                    JsValue::Str(s) => s,
                                    _ => String::new(),
                                }
                            } else { String::new() };
                            let ids: Vec<usize> = n.listeners.borrow().get(&event_type)
                                .cloned().unwrap_or_default();
                            for id in ids {
                                let cb = self.event_callbacks.borrow().get(&id).cloned();
                                if let Some(cb) = cb {
                                    self.call_function(cb, vec![event.clone()], None)?;
                                }
                            }
                            return Ok(JsValue::Bool(true));
                        }
                        "click" => {
                            // Programaticke click - dispatchEvent("click")
                            let ids: Vec<usize> = n.listeners.borrow().get("click")
                                .cloned().unwrap_or_default();
                            let mut event = JsObject::new();
                            event.set("type".into(), JsValue::Str("click".into()));
                            event.set("target".into(), JsValue::DomNode(Rc::clone(&n)));
                            let event_val = JsValue::Object(Rc::new(RefCell::new(event)));
                            for id in ids {
                                let cb = self.event_callbacks.borrow().get(&id).cloned();
                                if let Some(cb) = cb {
                                    self.call_function(cb, vec![event_val.clone()], None)?;
                                }
                            }
                            return Ok(JsValue::Undefined);
                        }
                        "focus" | "blur" => return Ok(JsValue::Undefined),
                        _ => {}
                    }
                    let _ = NodeData::new_text("");  // suppress unused-import warning
                    return Ok(JsValue::Undefined);
                }
                JsValue::Str(s) => {
                    let s = s.clone();
                    let arg_vals = self.eval_args(args, env)?;
                    if let Some(result) = call_string_method(&s, &key, arg_vals)? {
                        return Ok(result);
                    }
                }
                // Function.prototype.call / apply / bind
                JsValue::Function(_) if matches!(key.as_str(), "call" | "apply" | "bind") => {
                    let arg_vals = self.eval_args(args, env)?;
                    match key.as_str() {
                        "call" => {
                            // fn.call(thisArg, arg1, arg2, ...)
                            let this_arg = arg_vals.first().cloned();
                            let call_args = arg_vals.into_iter().skip(1).collect();
                            return self.call_function(this.clone(), call_args, this_arg);
                        }
                        "apply" => {
                            // fn.apply(thisArg, [arg1, arg2, ...])
                            let this_arg = arg_vals.first().cloned();
                            let call_args = match arg_vals.get(1) {
                                Some(JsValue::Array(a)) => a.borrow().clone(),
                                _ => vec![],
                            };
                            return self.call_function(this.clone(), call_args, this_arg);
                        }
                        "bind" => {
                            // fn.bind(thisArg, ...boundArgs) -> nova JsFunc::Bound
                            let bound_this = arg_vals.first().cloned().unwrap_or(JsValue::Undefined);
                            let bound_args: Vec<JsValue> = arg_vals.into_iter().skip(1).collect();
                            return Ok(JsValue::Function(JsFunc::Bound {
                                func: Box::new(this.clone()),
                                bound_this: Box::new(bound_this),
                                bound_args,
                            }));
                        }
                        _ => unreachable!()
                    }
                }
                // Array/Number/Date/Promise staticke metody
                JsValue::Function(JsFunc::Native(fname, _)) => {
                    let fname = fname.clone();
                    let arg_vals = self.eval_args(args, env)?;
                    match (fname.as_str(), key.as_str()) {
                        ("Array", "isArray") => {
                            return Ok(JsValue::Bool(matches!(arg_vals.first(), Some(JsValue::Array(_)))));
                        }
                        ("Array", "from") => {
                            let result = match arg_vals.into_iter().next().unwrap_or(JsValue::Undefined) {
                                JsValue::Array(a) => JsValue::Array(Rc::new(RefCell::new(a.borrow().clone()))),
                                JsValue::Str(s) => JsValue::Array(Rc::new(RefCell::new(
                                    s.chars().map(|c| JsValue::Str(c.to_string())).collect()
                                ))),
                                JsValue::Set(s) => JsValue::Array(Rc::new(RefCell::new(s.borrow().values.clone()))),
                                JsValue::Map(m) => {
                                    let entries: Vec<JsValue> = m.borrow().entries.iter()
                                        .map(|(k, v)| JsValue::Array(Rc::new(RefCell::new(vec![k.clone(), v.clone()]))))
                                        .collect();
                                    JsValue::Array(Rc::new(RefCell::new(entries)))
                                }
                                _ => JsValue::Array(Rc::new(RefCell::new(vec![]))),
                            };
                            return Ok(result);
                        }
                        ("Array", "of") => {
                            return Ok(JsValue::Array(Rc::new(RefCell::new(arg_vals))));
                        }
                        // Number staticke metody
                        ("Number", "isInteger") => {
                            return Ok(JsValue::Bool(matches!(arg_vals.first(),
                                Some(JsValue::Number(n)) if n.fract() == 0.0 && n.is_finite())));
                        }
                        ("Number", "isFinite") => {
                            return Ok(JsValue::Bool(matches!(arg_vals.first(),
                                Some(JsValue::Number(n)) if n.is_finite())));
                        }
                        ("Number", "isNaN") => {
                            return Ok(JsValue::Bool(matches!(arg_vals.first(),
                                Some(JsValue::Number(n)) if n.is_nan())));
                        }
                        ("Number", "isSafeInteger") => {
                            let ok = matches!(arg_vals.first(),
                                Some(JsValue::Number(n)) if n.is_finite() && n.fract() == 0.0 && n.abs() <= 9007199254740991.0);
                            return Ok(JsValue::Bool(ok));
                        }
                        ("Number", "parseInt") => {
                            let s = arg_vals.first().map(|v| v.to_string()).unwrap_or_default();
                            let radix = arg_vals.get(1).map(|v| v.to_number() as u32).unwrap_or(10);
                            let radix = if radix == 0 { 10 } else { radix.min(36) };
                            return Ok(JsValue::Number(
                                i64::from_str_radix(s.trim(), radix)
                                    .map(|n| n as f64).unwrap_or(f64::NAN)
                            ));
                        }
                        ("Number", "parseFloat") => {
                            let s = arg_vals.first().map(|v| v.to_string()).unwrap_or_default();
                            return Ok(JsValue::Number(s.trim().parse::<f64>().unwrap_or(f64::NAN)));
                        }
                        // Date staticke metody
                        ("Date", "now") => return Ok(JsValue::Number(now_ms())),
                        ("Date", "parse") => {
                            let s = arg_vals.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
                            return Ok(JsValue::Number(crate::interpreter::helpers::parse_date_string(&s)));
                        }
                        ("Date", "UTC") => {
                            // Date.UTC(year, month, day?, hours?, minutes?, seconds?, ms?) - UTC ms
                            let year = arg_vals.get(0).map(|v| v.to_number() as i64).unwrap_or(1970);
                            let month = arg_vals.get(1).map(|v| v.to_number() as u32).unwrap_or(0);
                            let day = arg_vals.get(2).map(|v| v.to_number() as u32).unwrap_or(1);
                            let hours = arg_vals.get(3).map(|v| v.to_number() as u32).unwrap_or(0);
                            let minutes = arg_vals.get(4).map(|v| v.to_number() as u32).unwrap_or(0);
                            let seconds = arg_vals.get(5).map(|v| v.to_number() as u32).unwrap_or(0);
                            let ms_part = arg_vals.get(6).map(|v| v.to_number() as u32).unwrap_or(0);
                            let ms = crate::interpreter::helpers::parts_to_ms(year, month, day, hours, minutes, seconds, ms_part);
                            return Ok(JsValue::Number(ms));
                        }
                        // Promise staticke metody
                        ("Promise", "resolve") => {
                            let v = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            return Ok(make_settled_promise("fulfilled", v));
                        }
                        ("Promise", "reject") => {
                            let v = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            return Ok(make_settled_promise("rejected", v));
                        }
                        ("Promise", "all") => {
                            let arr = match arg_vals.into_iter().next() {
                                Some(JsValue::Array(a)) => a.borrow().clone(),
                                _ => vec![],
                            };
                            let mut results = Vec::new();
                            for item in arr {
                                match get_promise_state(&item) {
                                    Some((s, v)) if s == "rejected" => {
                                        return Ok(make_settled_promise("rejected", v));
                                    }
                                    Some((_, v)) => results.push(v),
                                    None => results.push(item),
                                }
                            }
                            return Ok(make_settled_promise("fulfilled",
                                JsValue::Array(Rc::new(RefCell::new(results)))));
                        }
                        ("Promise", "allSettled") => {
                            let arr = match arg_vals.into_iter().next() {
                                Some(JsValue::Array(a)) => a.borrow().clone(),
                                _ => vec![],
                            };
                            let results: Vec<JsValue> = arr.into_iter().map(|item| {
                                let (status, value) = match get_promise_state(&item) {
                                    Some((s, v)) => (s, v),
                                    None => ("fulfilled".into(), item),
                                };
                                let mut o = JsObject::new();
                                o.set("status".into(), JsValue::Str(status.clone()));
                                if status == "fulfilled" {
                                    o.set("value".into(), value);
                                } else {
                                    o.set("reason".into(), value);
                                }
                                JsValue::Object(Rc::new(RefCell::new(o)))
                            }).collect();
                            return Ok(make_settled_promise("fulfilled",
                                JsValue::Array(Rc::new(RefCell::new(results)))));
                        }
                        ("Promise", "race") => {
                            let arr = match arg_vals.into_iter().next() {
                                Some(JsValue::Array(a)) => a.borrow().clone(),
                                _ => vec![],
                            };
                            for item in arr {
                                match get_promise_state(&item) {
                                    Some((s, v)) if s == "fulfilled" || s == "rejected" => {
                                        return Ok(make_settled_promise(&s, v));
                                    }
                                    _ => {}
                                }
                            }
                            return Ok(make_settled_promise("pending", JsValue::Undefined));
                        }
                        // String staticke metody
                        ("String", "fromCharCode") => {
                            let s: String = arg_vals.iter()
                                .map(|v| {
                                    let code = v.to_number() as u32;
                                    char::from_u32(code).unwrap_or('\u{FFFD}')
                                })
                                .collect();
                            return Ok(JsValue::Str(s));
                        }
                        ("String", "fromCodePoint") => {
                            let s: String = arg_vals.iter()
                                .map(|v| {
                                    let code = v.to_number() as u32;
                                    char::from_u32(code).unwrap_or('\u{FFFD}')
                                })
                                .collect();
                            return Ok(JsValue::Str(s));
                        }
                        ("Promise", "withResolvers") => {
                            // ES2024: { promise, resolve, reject }
                            // V nasi sync implementaci pouzivame stav v sdilenem RefCell
                            let state: Rc<RefCell<(String, JsValue)>> =
                                Rc::new(RefCell::new(("pending".to_string(), JsValue::Undefined)));
                            let promise_obj_rc = Rc::new(RefCell::new(JsObject::new()));
                            promise_obj_rc.borrow_mut().set("__promise_state__".into(), JsValue::Str("pending".into()));
                            promise_obj_rc.borrow_mut().set("__promise_value__".into(), JsValue::Undefined);
                            let promise_val = JsValue::Object(Rc::clone(&promise_obj_rc));

                            let p1 = Rc::clone(&promise_obj_rc);
                            let s1 = Rc::clone(&state);
                            let resolve_fn = native("resolve", move |a| {
                                let v = a.into_iter().next().unwrap_or(JsValue::Undefined);
                                if s1.borrow().0 == "pending" {
                                    *s1.borrow_mut() = ("fulfilled".into(), v.clone());
                                    p1.borrow_mut().set("__promise_state__".into(), JsValue::Str("fulfilled".into()));
                                    p1.borrow_mut().set("__promise_value__".into(), v);
                                }
                                Ok(JsValue::Undefined)
                            });
                            let p2 = Rc::clone(&promise_obj_rc);
                            let s2 = Rc::clone(&state);
                            let reject_fn = native("reject", move |a| {
                                let v = a.into_iter().next().unwrap_or(JsValue::Undefined);
                                if s2.borrow().0 == "pending" {
                                    *s2.borrow_mut() = ("rejected".into(), v.clone());
                                    p2.borrow_mut().set("__promise_state__".into(), JsValue::Str("rejected".into()));
                                    p2.borrow_mut().set("__promise_value__".into(), v);
                                }
                                Ok(JsValue::Undefined)
                            });

                            let mut result = JsObject::new();
                            result.set("promise".into(), promise_val);
                            result.set("resolve".into(), resolve_fn);
                            result.set("reject".into(),  reject_fn);
                            return Ok(JsValue::Object(Rc::new(RefCell::new(result))));
                        }
                        ("Promise", "try") => {
                            // ES2025: zavola callback synchronne, zabali vysledek do Promise
                            let cb = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            match self.call_function(cb, vec![], None) {
                                Ok(v) => {
                                    if get_promise_state(&v).is_some() {
                                        return Ok(v);
                                    }
                                    return Ok(make_settled_promise("fulfilled", v));
                                }
                                Err(JsError::Thrown(v)) => return Ok(make_settled_promise("rejected", v)),
                                Err(e) => return Err(e),
                            }
                        }
                        ("Promise", "any") => {
                            let arr = match arg_vals.into_iter().next() {
                                Some(JsValue::Array(a)) => a.borrow().clone(),
                                _ => vec![],
                            };
                            let mut errors = Vec::new();
                            for item in arr {
                                match get_promise_state(&item) {
                                    Some((s, v)) if s == "fulfilled" => {
                                        return Ok(make_settled_promise("fulfilled", v));
                                    }
                                    Some((_, v)) => errors.push(v),
                                    None => return Ok(make_settled_promise("fulfilled", item)),
                                }
                            }
                            let mut agg = JsObject::new();
                            agg.set("name".into(), JsValue::Str("AggregateError".into()));
                            agg.set("message".into(), JsValue::Str("All promises were rejected".into()));
                            agg.set("errors".into(), JsValue::Array(Rc::new(RefCell::new(errors))));
                            return Ok(make_settled_promise("rejected", JsValue::Object(Rc::new(RefCell::new(agg)))));
                        }
                        _ => {}
                    }
                    // Neznama staticka metoda - zkusit get_prop + call_function
                    let func = self.get_prop(&this, &key)?;
                    return self.call_function(func, arg_vals, Some(this));
                }
                _ => {}
            }

            let func = self.get_prop(&this, &key)?;
            let arg_vals = self.eval_args(args, env)?;
            return self.call_function(func, arg_vals, Some(this));
        }

        // Bezny call: optional chaining foo?.()
        let func_val = self.eval(callee, env)?;
        if optional && matches!(func_val, JsValue::Null | JsValue::Undefined) {
            return Ok(JsValue::Undefined);
        }
        let arg_vals = self.eval_args(args, env)?;
        self.call_function(func_val, arg_vals, None)
    }
}
