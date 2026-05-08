//! Statement execution dispatcher (exec_stmt + exec_stmts).

use super::*;

/// Snapshot lokalnich promennych ze scope chain (current env + parent).
/// Vraci pary (name, stringified value), serazene podle nazvu.
fn capture_locals(env: &Rc<RefCell<Environment>>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut cur = Some(Rc::clone(env));
    let mut depth = 0;
    while let Some(e) = cur {
        let names = e.borrow().names();
        for n in names {
            if seen.contains(&n) { continue; }
            // Skip globaly (vetsinou builtins) - jen lokalni scopes.
            if depth > 4 { break; }
            if let Some(v) = e.borrow().get(&n) {
                // Skip funkce + typeof "[Function]" - bloat.
                if matches!(v, JsValue::Function(_)) { continue; }
                seen.insert(n.clone());
                out.push((n, v.pretty_print()));
            }
        }
        cur = e.borrow().parent_chain();
        depth += 1;
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

impl Interpreter {
    pub(super) fn exec_stmts(&mut self, stmts: &[Stmt], env: &Rc<RefCell<Environment>>) -> StmtResult {
        for s in stmts {
            if let Some(sig) = self.exec_stmt(s, env)? { return Ok(Some(sig)); }
        }
        Ok(None)
    }

    pub(super) fn exec_stmt(&mut self, stmt: &Stmt, env: &Rc<RefCell<Environment>>) -> StmtResult {
        match stmt {
            Stmt::WithLine { line, inner } => {
                self.current_line = *line;
                // Breakpoint OR step check: pri match capture locals + log + pause flag.
                let should_pause = {
                    let dbg = self.debugger.borrow();
                    dbg.is_breakpoint(*line) || dbg.step_should_pause()
                };
                if should_pause {
                    let msg = format!("Breakpoint hit at line {}", line);
                    self.console_log.borrow_mut().push(("warn".into(), msg));
                    // Capture lokalni promenne ze scope chain pro UI panel.
                    let locals = capture_locals(env);
                    let mut dbg = self.debugger.borrow_mut();
                    dbg.pause_at(*line);
                    dbg.locals = locals;
                    dbg.step = None;
                }
                return self.exec_stmt(inner, env);
            }
            Stmt::Empty => Ok(None),

            Stmt::Expr(e) => { self.eval(e, env)?; Ok(None) }

            Stmt::Block(body) => {
                let child = Environment::new_child(env);
                self.exec_stmts(body, &child)
            }

            Stmt::Var { kind, decls } => {
                for d in decls {
                    let val = match &d.init { Some(e) => self.eval(e, env)?, None => JsValue::Undefined };
                    // var = function-scoped (global), let/const = block-scoped
                    let target_env = if *kind == VarKind::Var {
                        Rc::clone(&self.global)
                    } else {
                        Rc::clone(env)
                    };
                    self.destructure_bind(&d.pattern, val, &target_env)?;
                }
                Ok(None)
            }

            Stmt::Function { name, params, body } => {
                let func = JsValue::Function(JsFunc::User {
                    name: Some(name.clone()),
                    params: params.clone(),
                    body: FuncBody::Stmts(body.clone()),
                    env: Rc::clone(env),
                });
                env.borrow_mut().define(name, func);
                Ok(None)
            }

            // Generator funkce: `function* name(params) { body }`
            Stmt::GeneratorFunc { name, params, body } => {
                let func = JsValue::Function(JsFunc::Generator {
                    name: Some(name.clone()),
                    params: params.clone(),
                    body: body.clone(),
                    env: Rc::clone(env),
                });
                env.borrow_mut().define(name, func);
                Ok(None)
            }

            // Async funkce: `async function name(params) { body }`
            // Async generator: implementovan jako bezny generator (sync model).
            // V realnem JS by kazdy yield vracel Promise, my v sync vraci hodnotu.
            Stmt::AsyncGeneratorFunc { name, params, body } => {
                let func = JsValue::Function(JsFunc::Generator {
                    name: Some(name.clone()),
                    params: params.clone(),
                    body: body.clone(),
                    env: Rc::clone(env),
                });
                env.borrow_mut().define(name, func);
                Ok(None)
            }
            Stmt::AsyncFunc { name, params, body } => {
                let func = JsValue::Function(JsFunc::Async {
                    name: Some(name.clone()),
                    params: params.clone(),
                    body: FuncBody::Stmts(body.clone()),
                    env: Rc::clone(env),
                });
                env.borrow_mut().define(name, func);
                Ok(None)
            }

            // Import: nacti modul a binduj specifiers do scope
            Stmt::Import { source, specifiers } => {
                let ns = self.load_module(source)?;
                let ns_obj = match &ns {
                    JsValue::Object(o) => Rc::clone(o),
                    _ => return Err(JsError::Runtime(
                        format!("ModuleError: modul '{source}' nevratil objekt")
                    )),
                };
                for spec in specifiers {
                    match spec {
                        ImportSpecifier::Default(local) => {
                            let v = ns_obj.borrow().props.get("default").cloned()
                                .unwrap_or(JsValue::Undefined);
                            env.borrow_mut().define(local, v);
                        }
                        ImportSpecifier::Named { imported, local } => {
                            let v = ns_obj.borrow().props.get(imported).cloned()
                                .unwrap_or(JsValue::Undefined);
                            env.borrow_mut().define(local, v);
                        }
                        ImportSpecifier::Namespace(local) => {
                            env.borrow_mut().define(local, JsValue::Object(Rc::clone(&ns_obj)));
                        }
                    }
                }
                Ok(None)
            }

            // Export: zaregistruje hodnotu do current_exports mapy.
            // Funguje jen pri nacitani modulu (current_exports = Some).
            Stmt::Export(kind) => {
                match kind {
                    ExportKind::Decl(decl) => {
                        // Spust deklaraci a pak najdi v env nove definovane jmeno(a)
                        let pre_keys: Vec<String> = env.borrow().vars.keys().cloned().collect();
                        self.exec_stmt(decl, env)?;
                        let post_keys: Vec<String> = env.borrow().vars.keys().cloned().collect();
                        if let Some(exports) = &self.current_exports {
                            for k in post_keys {
                                if !pre_keys.contains(&k) {
                                    let v = env.borrow().get(&k).unwrap_or(JsValue::Undefined);
                                    exports.borrow_mut().insert(k, v);
                                }
                            }
                        }
                    }
                    ExportKind::Default(expr) => {
                        let v = self.eval(expr, env)?;
                        if let Some(exports) = &self.current_exports {
                            exports.borrow_mut().insert("default".to_string(), v);
                        }
                    }
                    ExportKind::Named(pairs) => {
                        if let Some(exports) = &self.current_exports {
                            for (local, exported) in pairs {
                                let v = env.borrow().get(local).unwrap_or(JsValue::Undefined);
                                exports.borrow_mut().insert(exported.clone(), v);
                            }
                        }
                    }
                }
                Ok(None)
            }

            Stmt::Return(val) => {
                let v = match val { Some(e) => self.eval(e, env)?, None => JsValue::Undefined };
                Ok(Some(Signal::Return(v)))
            }

            Stmt::Throw(e) => Err(JsError::Thrown(self.eval(e, env)?)),

            Stmt::Break(label)    => Ok(Some(Signal::Break(label.clone()))),
            Stmt::Continue(label) => Ok(Some(Signal::Continue(label.clone()))),

            Stmt::Switch { discriminant, cases } => {
                let value = self.eval(discriminant, env)?;

                // Najdi odpovidajici case (strict ===) a pozici default
                let mut match_idx: Option<usize> = None;
                let mut default_idx: Option<usize> = None;

                for (i, case) in cases.iter().enumerate() {
                    match &case.test {
                        None => { default_idx = Some(i); }
                        Some(test_expr) => {
                            if match_idx.is_none() {
                                let test_val = self.eval(test_expr, env)?;
                                if value.strict_eq(&test_val) {
                                    match_idx = Some(i);
                                }
                            }
                        }
                    }
                }

                // Spust od prvniho odpovidajiciho case, nebo od default
                let start = match_idx.or(default_idx);

                if let Some(start_idx) = start {
                    let switch_env = Environment::new_child(env);
                    for case in &cases[start_idx..] {
                        for stmt in &case.body {
                            match self.exec_stmt(stmt, &switch_env)? {
                                // break (bez labelu) ukonci switch
                                Some(Signal::Break(None)) => return Ok(None),
                                // break s labelem - propaguj nahoru (zpracuje Labeled)
                                Some(s) => return Ok(Some(s)),
                                None => {}
                            }
                        }
                    }
                }

                Ok(None)
            }

            Stmt::If { test, yes, no } => {
                if self.eval(test, env)?.is_truthy() {
                    self.exec_stmt(yes, env)
                } else if let Some(alt) = no {
                    self.exec_stmt(alt, env)
                } else { Ok(None) }
            }

            Stmt::While { test, body } => {
                loop {
                    if !self.eval(test, env)?.is_truthy() { break; }
                    match self.exec_stmt(body, env)? {
                        Some(Signal::Break(None))    => break,
                        Some(Signal::Continue(None)) => continue,
                        Some(s) => return Ok(Some(s)),  // labeled/Return propaguj nahoru
                        None => {}
                    }
                }
                Ok(None)
            }

            Stmt::DoWhile { body, test } => {
                loop {
                    match self.exec_stmt(body, env)? {
                        Some(Signal::Break(None))    => break,
                        Some(Signal::Continue(None)) => {}
                        Some(s) => return Ok(Some(s)),
                        None => {}
                    }
                    if !self.eval(test, env)?.is_truthy() { break; }
                }
                Ok(None)
            }

            Stmt::For { init, test, update, body } => {
                let for_env = Environment::new_child(env);
                if let Some(init) = init {
                    match init {
                        ForInit::Var { kind: _, decls } => {
                            for d in decls {
                                let v = match &d.init { Some(e) => self.eval(e, &for_env)?, None => JsValue::Undefined };
                                // for init vzdy bindu do for_env (let/const scoped)
                                let target_env = Rc::clone(&for_env);
                                self.destructure_bind(&d.pattern, v, &target_env)?;
                            }
                        }
                        ForInit::Expr(e) => { self.eval(e, &for_env)?; }
                    }
                }
                loop {
                    if let Some(cond) = test {
                        if !self.eval(cond, &for_env)?.is_truthy() { break; }
                    }
                    match self.exec_stmt(body, &for_env)? {
                        Some(Signal::Break(None))    => break,
                        Some(Signal::Continue(None)) => {}
                        Some(s) => return Ok(Some(s)),
                        None => {}
                    }
                    if let Some(upd) = update { self.eval(upd, &for_env)?; }
                }
                Ok(None)
            }

            Stmt::ForOf { kind: _, target, iter, body } => {
                let arr_val = self.eval(iter, env)?;
                // Podpora pro custom iterables pres Symbol.iterator
                let items = self.collect_iterable(arr_val)?;
                for item in items {
                    let loop_env = Environment::new_child(env);
                    self.bind_target_expr(target, item, &loop_env)?;
                    match self.exec_stmt(body, &loop_env)? {
                        Some(Signal::Break(None))    => break,
                        Some(Signal::Continue(None)) => continue,
                        Some(s) => return Ok(Some(s)),
                        None => {}
                    }
                }
                Ok(None)
            }

            // For-await-of: jako for-of, ale kazdy yielded element rozbal jako Promise
            // V nasi sync implementaci stejne jako for-of, ale s unwrap_promise_result
            Stmt::ForAwaitOf { kind: _, target, iter, body } => {
                let arr_val = self.eval(iter, env)?;
                let items = self.collect_iterable(arr_val)?;
                for item in items {
                    // Pokud je item Promise, await ho
                    let resolved = match unwrap_promise_result(item) {
                        Ok(v) => v,
                        Err(reason) => return Err(JsError::Thrown(reason)),
                    };
                    let loop_env = Environment::new_child(env);
                    self.bind_target_expr(target, resolved, &loop_env)?;
                    match self.exec_stmt(body, &loop_env)? {
                        Some(Signal::Break(None))    => break,
                        Some(Signal::Continue(None)) => continue,
                        Some(s) => return Ok(Some(s)),
                        None => {}
                    }
                }
                Ok(None)
            }

            Stmt::ForIn { kind: _, target, iter, body } => {
                let obj_val = self.eval(iter, env)?;
                let keys = match &obj_val {
                    JsValue::Object(o) => {
                        let mut raw: Vec<String> = o.borrow().props.keys()
                            .filter(|k| !is_internal_key(k))
                            .cloned().collect();
                        // JS spec: integer-index keys ascending, then remaining keys
                        raw.sort_by(|a, b| {
                            let ai = a.parse::<u64>().ok();
                            let bi = b.parse::<u64>().ok();
                            match (ai, bi) {
                                (Some(x), Some(y)) => x.cmp(&y),
                                (Some(_), None)    => std::cmp::Ordering::Less,
                                (None, Some(_))    => std::cmp::Ordering::Greater,
                                (None, None)       => std::cmp::Ordering::Equal,
                            }
                        });
                        raw
                    }
                    _ => vec![],
                };
                for key in keys {
                    let loop_env = Environment::new_child(env);
                    self.bind_target_expr(target, JsValue::Str(key), &loop_env)?;
                    match self.exec_stmt(body, &loop_env)? {
                        Some(Signal::Break(None))    => break,
                        Some(Signal::Continue(None)) => continue,
                        Some(s) => return Ok(Some(s)),
                        None => {}
                    }
                }
                Ok(None)
            }

            Stmt::Try { body, catch, finally } => {
                let try_env = Environment::new_child(env);
                let result = self.exec_stmts(body, &try_env);
                let sig = match result {
                    Ok(s) => s,
                    Err(e) => {
                        if let Some(c) = catch {
                            let catch_env = Environment::new_child(env);
                            if let Some(param) = &c.param {
                                let err_val = match e {
                                    JsError::Thrown(v) => v,
                                    JsError::Runtime(s) => JsValue::Str(s),
                                };
                                catch_env.borrow_mut().define(param, err_val);
                            }
                            self.exec_stmts(&c.body, &catch_env)?
                        } else { return Err(e); }
                    }
                };
                if let Some(fin) = finally {
                    let fin_env = Environment::new_child(env);
                    self.exec_stmts(fin, &fin_env)?;
                }
                Ok(sig)
            }

            Stmt::Labeled { label, body } => {
                match self.exec_stmt(body, env)? {
                    // break label / continue label odpovidajici tomuto labelu -> konzumuj signal
                    Some(Signal::Break(Some(l)))    if l == *label => Ok(None),
                    Some(Signal::Continue(Some(l))) if l == *label => Ok(None),
                    // vsechno ostatni propaguj (Return, Break(None), Break(jiny_label), ...)
                    other => Ok(other),
                }
            }

            Stmt::Class { name, super_class, body } => {
                let super_val = if let Some(sc) = super_class {
                    Some(Box::new(self.eval(sc, env)?))
                } else { None };
                let cls = self.make_class_func(Some(name.clone()), super_val, body, env);
                env.borrow_mut().define(name, cls);
                Ok(None)
            }
        }
    }

    // ─── Výrazy ───────────────────────────────────────────────────────────────

}
