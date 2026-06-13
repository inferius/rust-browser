//! call_function dispatch + new constructor (Map/Set/Date/Error/Promise) + generators.

use super::*;

impl Interpreter {
    pub fn call_function(&mut self, func: JsValue, args: Vec<JsValue>, this: Option<JsValue>) -> EvalResult {
        match func {
            // Tridu nelze zavolat bez `new`
            JsValue::Function(JsFunc::Class { name, .. }) => {
                Err(JsError::Runtime(format!(
                    "TypeError: trida '{}' musi byt volana s 'new'",
                    name.as_deref().unwrap_or("(anonymous)")
                )))
            }
            JsValue::Function(JsFunc::Native(_, f)) => {
                f(args).map_err(JsError::Runtime)
            }
            JsValue::Function(JsFunc::User { params, body, env, .. }) => {
                let call_env = Environment::new_function_child(&env);
                let params = params.clone();
                let body = body.clone();
                self.bind_params(&params, args.clone(), &call_env)?;
                if let Some(t) = this { call_env.borrow_mut().define("this", t); }
                let args_arr = JsValue::Array(Rc::new(RefCell::new(args)));
                call_env.borrow_mut().define("arguments", args_arr);
                let body = body;

                match &body {
                    FuncBody::Stmts(stmts) => {
                        let stmts = stmts.clone();
                        Ok(match self.exec_stmts(&stmts, &call_env)? {
                            Some(Signal::Return(v)) => v,
                            _ => JsValue::Undefined,
                        })
                    }
                    FuncBody::Expr(e) => {
                        let e = e.clone();
                        self.eval(&e, &call_env)
                    }
                }
            }
            // Volani generator funkce vraci iterator objekt
            JsValue::Function(JsFunc::Generator { params, body, env, .. }) => {
                self.call_generator(params, body, args, env)
            }
            // Async funkce: spust synchronne, zabal vysledek do Promise
            JsValue::Function(JsFunc::Async { params, body, env, .. }) => {
                let call_env = Environment::new_function_child(&env);
                self.bind_params(&params, args.clone(), &call_env)?;
                if let Some(t) = this { call_env.borrow_mut().define("this", t); }
                let args_arr = JsValue::Array(Rc::new(RefCell::new(args)));
                call_env.borrow_mut().define("arguments", args_arr);
                match match &body {
                    FuncBody::Stmts(stmts) => {
                        let stmts = stmts.clone();
                        self.exec_stmts(&stmts, &call_env)
                            .map(|s| match s { Some(Signal::Return(v)) => v, _ => JsValue::Undefined })
                    }
                    FuncBody::Expr(e) => {
                        let e = e.clone();
                        self.eval(&e, &call_env)
                    }
                } {
                    Ok(v) => {
                        // Pokud return value je uz Promise, vrat ho primo
                        if get_promise_state(&v).is_some() {
                            Ok(v)
                        } else {
                            Ok(make_settled_promise("fulfilled", v))
                        }
                    }
                    Err(JsError::Thrown(v)) => Ok(make_settled_promise("rejected", v)),
                    Err(e) => Err(e),
                }
            }
            // Bound funkce: prepend bound_args, pouzij bound_this
            JsValue::Function(JsFunc::Bound { func, bound_this, bound_args }) => {
                let mut all_args = bound_args.clone();
                all_args.extend(args);
                let effective_this = this.or(Some(*bound_this));
                self.call_function(*func, all_args, effective_this)
            }
            _ => {
                // Diag: pres `__rwe_call_debug` env, dump arg count + types.
                if std::env::var("RWE_CALL_DEBUG").is_ok() {
                    eprintln!("[call non-fn] target={:?} args={} line={}",
                        func, args.len(), self.current_line);
                }
                Err(JsError::Runtime(format!("{func} není funkce (line {})", self.current_line)))
            }
        }
    }

    pub(super) fn call_new(&mut self, func: JsValue, args: Vec<JsValue>) -> EvalResult {
        // `new ClassName()` pro tridy - speciálni logika
        if matches!(&func, JsValue::Function(JsFunc::Class { .. })) {
            return self.construct_class(func, args);
        }
        // Vestavene konstruktory: Map, Set, ...
        if let JsValue::Function(JsFunc::Native(name, _)) = &func {
            match name.as_str() {
                "Map" | "WeakMap" => return self.construct_map(args),
                "Set" | "WeakSet" => return self.construct_set(args),
                "WeakRef" => {
                    // new WeakRef(target) -> objekt s __weak_target__
                    let target = args.into_iter().next().unwrap_or(JsValue::Undefined);
                    let mut obj = JsObject::new();
                    obj.set("__weak_target__".into(), target);
                    return Ok(JsValue::Object(Rc::new(RefCell::new(obj))));
                }
                "Proxy" => {
                    // new Proxy(target, handler) - wraps target with trap calls
                    let mut iter = args.into_iter();
                    let target = iter.next().unwrap_or(JsValue::Undefined);
                    let handler = iter.next().unwrap_or(JsValue::Undefined);
                    let mut obj = JsObject::new();
                    obj.set("__proxy_target__".into(), target);
                    obj.set("__proxy_handler__".into(), handler);
                    return Ok(JsValue::Object(Rc::new(RefCell::new(obj))));
                }
                "FinalizationRegistry" => {
                    // new FinalizationRegistry(cb) -> objekt s __finalizer__
                    let cb = args.into_iter().next().unwrap_or(JsValue::Undefined);
                    let mut obj = JsObject::new();
                    obj.set("__finalizer__".into(), cb);
                    return Ok(JsValue::Object(Rc::new(RefCell::new(obj))));
                }
                "Date"            => return self.construct_date(args),
                "Promise"         => return self.construct_promise(args),
                "RegExp"          => {
                    let pat = args.get(0).map(|v| v.to_string()).unwrap_or_default();
                    let flags = args.get(1).map(|v| v.to_string()).unwrap_or_default();
                    return Ok(make_regex_object(&pat, &flags));
                }
                "BigNumber"       => {
                    let s = args.into_iter().next().map(|v| match v {
                        JsValue::BigNumber(n) => n.to_string(),
                        other => other.to_string(),
                    }).unwrap_or_else(|| "0".into());
                    return BigDecimal::from_str(s.trim())
                        .map(|bd| JsValue::BigNumber(Rc::new(bd)))
                        .map_err(|_| JsError::Runtime(format!("BigNumber: neplatna hodnota '{s}'")));
                }
                "Error" | "TypeError" | "RangeError" | "SyntaxError"
                | "ReferenceError" | "URIError" | "EvalError" => {
                    return self.construct_error(name.clone(), args);
                }
                _     => {}
            }
        }
        // `new FunctionConstructor()` - stary styl
        // Pro Native funkce: kdyz vrati Object/DomNode/Array, pouzij jeho return value
        // (umoznuje natnivnim konstruktorum vratit objekt vlastniho typu)
        let is_native = matches!(&func, JsValue::Function(JsFunc::Native(_, _)));
        let obj = JsValue::Object(Rc::new(RefCell::new(JsObject::new())));
        let result = self.call_function(func, args, Some(obj.clone()))?;
        if is_native && matches!(&result,
            JsValue::Object(_) | JsValue::DomNode(_) | JsValue::Array(_)
            | JsValue::Map(_) | JsValue::Set(_)) {
            return Ok(result);
        }
        Ok(obj)
    }

    /// Konstruktor `new Map([[k,v], ...])` nebo `new Map()`.
    pub(super) fn construct_map(&mut self, args: Vec<JsValue>) -> EvalResult {
        let mut m = JsMap::new();
        if let Some(JsValue::Array(entries)) = args.into_iter().next() {
            for entry in entries.borrow().clone() {
                if let JsValue::Array(pair) = entry {
                    let pair = pair.borrow();
                    let k = pair.get(0).cloned().unwrap_or(JsValue::Undefined);
                    let v = pair.get(1).cloned().unwrap_or(JsValue::Undefined);
                    m.set(k, v);
                }
            }
        }
        Ok(JsValue::Map(Rc::new(RefCell::new(m))))
    }

    /// Konstruktor `new Set([val, ...])` nebo `new Set()`.
    pub(super) fn construct_set(&mut self, args: Vec<JsValue>) -> EvalResult {
        let mut s = JsSet::new();
        if let Some(iterable) = args.into_iter().next() {
            let items = self.collect_iterable(iterable).unwrap_or_default();
            for v in items { s.add(v); }
        }
        Ok(JsValue::Set(Rc::new(RefCell::new(s))))
    }

    /// Konstruktor `new Date()`, `new Date(ms)`, `new Date("iso-string")`,
    /// `new Date(year, month, day?, hours?, minutes?, seconds?, ms?)`.
    pub(super) fn construct_date(&mut self, args: Vec<JsValue>) -> EvalResult {
        if args.len() >= 2 {
            // Multi-arg form: year, month, day, hours, minutes, seconds, ms
            let year = args[0].to_number() as i64;
            let month = args[1].to_number() as u32;
            let day = args.get(2).map(|v| v.to_number() as u32).unwrap_or(1);
            let hours = args.get(3).map(|v| v.to_number() as u32).unwrap_or(0);
            let minutes = args.get(4).map(|v| v.to_number() as u32).unwrap_or(0);
            let seconds = args.get(5).map(|v| v.to_number() as u32).unwrap_or(0);
            let ms_part = args.get(6).map(|v| v.to_number() as u32).unwrap_or(0);
            let ms = crate::interpreter::helpers::parts_to_ms(year, month, day, hours, minutes, seconds, ms_part);
            return Ok(make_date_object(ms));
        }
        let ms = match args.into_iter().next() {
            None                       => now_ms(),
            Some(JsValue::Number(n))   => n,
            Some(JsValue::Str(s))      => crate::interpreter::helpers::parse_date_string(&s),
            Some(JsValue::Undefined)   => now_ms(),
            Some(other) => {
                // Date kopirovani: new Date(other_date) -> uses valueOf
                let n = other.to_number();
                if n.is_nan() { now_ms() } else { n }
            }
        };
        Ok(make_date_object(ms))
    }

    /// Konstruktor `new Error("msg")`, `new TypeError("msg")`, atd.
    /// ES2022: druhy argument je options objekt s `cause`.
    pub(super) fn construct_error(&mut self, name: String, args: Vec<JsValue>) -> EvalResult {
        let mut iter = args.into_iter();
        let msg = iter.next().map(|v| v.to_string()).unwrap_or_default();
        let options = iter.next();
        let mut obj = JsObject::new();
        obj.set("name".into(),    JsValue::Str(name.clone()));
        obj.set("message".into(), JsValue::Str(msg.clone()));
        obj.set("stack".into(),   JsValue::Str(format!("{name}: {msg}")));
        // ES2022 Error.cause: pokud options.cause existuje, uloz
        if let Some(JsValue::Object(opts)) = options {
            let cause = opts.borrow().props.get("cause").cloned();
            if let Some(c) = cause {
                obj.set("cause".into(), c);
            }
        }
        Ok(JsValue::Object(Rc::new(RefCell::new(obj))))
    }

    /// Konstruktor `new Promise(executor)` - synchronni rozliseni.
    ///
    /// Executor je volan okamzite se dvema argumenty:
    /// - `resolve(value)` - splni promise
    /// - `reject(reason)` - odmitne promise
    pub(super) fn construct_promise(&mut self, args: Vec<JsValue>) -> EvalResult {
        let mut obj = JsObject::new();
        obj.set("__promise_state__".into(), JsValue::Str("pending".into()));
        obj.set("__promise_value__".into(), JsValue::Undefined);
        // Pending callbacks: array s entries [(on_fulfilled, on_rejected, child_promise), ...]
        // Pri then() pending: push entry. Pri resolve/reject: drain + call.
        obj.set("__pending_callbacks__".into(),
            JsValue::Array(Rc::new(RefCell::new(Vec::new()))));
        let obj_rc = Rc::new(RefCell::new(obj));

        let executor = args.into_iter().next().unwrap_or(JsValue::Undefined);
        if matches!(executor, JsValue::Function(_)) {
            // Vytvor resolve/reject closures ktere zachyti Rc<RefCell<JsObject>>
            // Resolve/reject mutuje state + value + DRAIN pending callbacks.
            // (Pending callbacks scheduling = task_queue push, real call az
            // event loop. Pro nasi sync interpreter scheduling = call inline.)
            let obj_rc_r = Rc::clone(&obj_rc);
            let tq = Rc::clone(&self.task_queue);
            let id_ctr = Rc::clone(&self.next_timer_id);
            let resolve = native("resolve", move |a| {
                let val = a.into_iter().next().unwrap_or(JsValue::Undefined);
                let pending = {
                    let mut o = obj_rc_r.borrow_mut();
                    if !matches!(o.props.get("__promise_state__"), Some(JsValue::Str(s)) if s == "pending") {
                        return Ok(JsValue::Undefined);
                    }
                    o.set("__promise_state__".into(), JsValue::Str("fulfilled".into()));
                    o.set("__promise_value__".into(), val.clone());
                    match o.props.get("__pending_callbacks__").cloned() {
                        Some(JsValue::Array(arr)) => arr,
                        _ => Rc::new(RefCell::new(Vec::new())),
                    }
                };
                // Drain pending callbacks: kazdy [on_fulfilled, _, _] schedule
                // pres task_queue (microtask emulation - bezi pres drain_timers).
                let cbs: Vec<JsValue> = pending.borrow().clone();
                for entry in cbs {
                    if let JsValue::Array(a3) = entry {
                        let triple = a3.borrow().clone();
                        if let Some(on_f) = triple.first().cloned() {
                            if matches!(on_f, JsValue::Function(_)) {
                                let id = {
                                    let mut ctr = id_ctr.borrow_mut();
                                    let id = *ctr; *ctr += 1; id
                                };
                                tq.borrow_mut().push((id, std::time::Instant::now(), on_f, vec![val.clone()]));
                            }
                        }
                    }
                }
                Ok(JsValue::Undefined)
            });
            let obj_rc_j = Rc::clone(&obj_rc);
            let tq2 = Rc::clone(&self.task_queue);
            let id_ctr2 = Rc::clone(&self.next_timer_id);
            let reject = native("reject", move |a| {
                let val = a.into_iter().next().unwrap_or(JsValue::Undefined);
                let pending = {
                    let mut o = obj_rc_j.borrow_mut();
                    if !matches!(o.props.get("__promise_state__"), Some(JsValue::Str(s)) if s == "pending") {
                        return Ok(JsValue::Undefined);
                    }
                    o.set("__promise_state__".into(), JsValue::Str("rejected".into()));
                    o.set("__promise_value__".into(), val.clone());
                    match o.props.get("__pending_callbacks__").cloned() {
                        Some(JsValue::Array(arr)) => arr,
                        _ => Rc::new(RefCell::new(Vec::new())),
                    }
                };
                let cbs: Vec<JsValue> = pending.borrow().clone();
                for entry in cbs {
                    if let JsValue::Array(a3) = entry {
                        let triple = a3.borrow().clone();
                        if let Some(on_r) = triple.get(1).cloned() {
                            if matches!(on_r, JsValue::Function(_)) {
                                let id = {
                                    let mut ctr = id_ctr2.borrow_mut();
                                    let id = *ctr; *ctr += 1; id
                                };
                                tq2.borrow_mut().push((id, std::time::Instant::now(), on_r, vec![val.clone()]));
                            }
                        }
                    }
                }
                Ok(JsValue::Undefined)
            });

            // Spust executor - chyba = reject
            match self.call_function(executor, vec![resolve, reject], None) {
                Ok(_) => {}
                Err(JsError::Thrown(v)) => {
                    let mut o = obj_rc.borrow_mut();
                    if matches!(o.props.get("__promise_state__"), Some(JsValue::Str(s)) if s == "pending") {
                        o.set("__promise_state__".into(), JsValue::Str("rejected".into()));
                        o.set("__promise_value__".into(), v);
                    }
                }
                Err(e) => return Err(e),
            }
        }
        Ok(JsValue::Object(obj_rc))
    }

    // ─── Generator + iterator protokol ───────────────────────────────────────

    /// Zavola generator funkci a vrati iterator objekt.
    ///
    /// Implementace: spusti cely body v generator rezimu (yield_buffer = Some(vec![])),
    /// sbira yield hodnoty, pak vrati iterator objekt s metodou `next()`.
    pub(super) fn call_generator(
        &mut self,
        params: Vec<Param>,
        body: Vec<Stmt>,
        args: Vec<JsValue>,
        closure_env: Rc<RefCell<Env>>,
    ) -> EvalResult {
        // Nastav generator rezim
        let prev_buf = self.yield_buffer.take();
        self.yield_buffer = Some(Vec::new());

        // Spust telo generator funkce
        let gen_env = Environment::new_function_child(&closure_env);
        let params = params.clone();
        let body = body.clone();
        self.bind_params(&params, args, &gen_env)?;
        let _ = self.exec_stmts(&body, &gen_env);

        // Vezmi nahromadene hodnoty
        let yielded = self.yield_buffer.take().unwrap_or_default();
        self.yield_buffer = prev_buf;

        // Vytvor iterator objekt (sdileny refcell pro index)
        let values = Rc::new(yielded);
        let index  = Rc::new(RefCell::new(0usize));

        let values2 = Rc::clone(&values);
        let index2  = Rc::clone(&index);

        let mut iter_obj = JsObject::new();

        // next() metoda
        let next_fn = native("(generator).next", move |_args| {
            let i = *index2.borrow();
            if i < values2.len() {
                *index2.borrow_mut() = i + 1;
                let mut result = JsObject::new();
                result.set("value".into(), values2[i].clone());
                result.set("done".into(),  JsValue::Bool(false));
                Ok(JsValue::Object(Rc::new(RefCell::new(result))))
            } else {
                let mut result = JsObject::new();
                result.set("value".into(), JsValue::Undefined);
                result.set("done".into(),  JsValue::Bool(true));
                Ok(JsValue::Object(Rc::new(RefCell::new(result))))
            }
        });
        iter_obj.set("next".into(), next_fn);

        // [Symbol.iterator]() - vraci this (iterator je zaroven iterable)
        let values3 = Rc::clone(&values);
        let index3  = Rc::new(RefCell::new(0usize));
        let sym_iter_fn = native("(generator)[Symbol.iterator]", move |_| {
            // Vrat novy iterator od zacatku
            let values4 = Rc::clone(&values3);
            let index4  = Rc::clone(&index3);
            let mut obj = JsObject::new();
            let v4 = Rc::clone(&values4);
            let i4 = Rc::clone(&index4);
            obj.set("next".into(), native("(gen.iter).next", move |_| {
                let i = *i4.borrow();
                if i < v4.len() {
                    *i4.borrow_mut() = i + 1;
                    let mut r = JsObject::new();
                    r.set("value".into(), v4[i].clone());
                    r.set("done".into(),  JsValue::Bool(false));
                    Ok(JsValue::Object(Rc::new(RefCell::new(r))))
                } else {
                    let mut r = JsObject::new();
                    r.set("value".into(), JsValue::Undefined);
                    r.set("done".into(),  JsValue::Bool(true));
                    Ok(JsValue::Object(Rc::new(RefCell::new(r))))
                }
            }));
            Ok(JsValue::Object(Rc::new(RefCell::new(obj))))
        });
        iter_obj.set("Symbol.iterator".into(), sym_iter_fn);
        // Iterator helpers (ES2025) - marker pres call_method special-case dispatch.
        iter_obj.set("__iterator_helpers__".into(), JsValue::Bool(true));

        Ok(JsValue::Object(Rc::new(RefCell::new(iter_obj))))
    }

}
