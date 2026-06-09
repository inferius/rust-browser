//! Expression eval: eval (dispatcher), eval_unary/binary/logical/assign,
//! assign_to, destructure_bind, bind_target_expr.

use super::*;

impl Interpreter {
    pub fn eval(&mut self, expr: &Expr, env: &Rc<RefCell<Environment>>) -> EvalResult {
        match expr {
            Expr::Number(n)    => Ok(JsValue::Number(*n)),
            Expr::BigInt(s)    => {
                let n = BigInt::from_str(s)
                    .map_err(|_| JsError::Runtime(format!("SyntaxError: invalid BigInt '{s}'")))?;
                Ok(JsValue::BigInt(Rc::new(n)))
            }
            Expr::DynamicImport(arg) => {
                // Vyhodnot specifier, nacti modul, vrat Promise.fulfilled(namespace).
                // V chybovem pripade vrat Promise.rejected(error).
                let spec = self.eval(arg, env)?;
                let source = spec.to_string();
                match self.load_module(&source) {
                    Ok(ns) => Ok(make_settled_promise("fulfilled", ns)),
                    Err(JsError::Runtime(msg)) => {
                        let mut err = JsObject::new();
                        err.set("name".into(),    JsValue::Str("Error".into()));
                        err.set("message".into(), JsValue::Str(msg));
                        Ok(make_settled_promise("rejected",
                            JsValue::Object(Rc::new(RefCell::new(err)))))
                    }
                    Err(e) => Err(e),
                }
            }
            Expr::Str(s)       => Ok(JsValue::Str(s.clone())),
            Expr::Bool(b)      => Ok(JsValue::Bool(*b)),
            Expr::Null         => Ok(JsValue::Null),
            Expr::Undefined    => Ok(JsValue::Undefined),
            Expr::Regex(p, f)  => Ok(make_regex_object(p, f)),

            Expr::Ident(name)  => {
                // 1. Standard scope lookup pres env chain.
                if let Some(v) = env.borrow().get(name) {
                    return Ok(v);
                }
                // 2. Browser-spec fallback: bare ident resolves pres global object
                // (= window). Pres UMD pattern `global.lucide = factory()` sets
                // window.lucide, ale frontend code uses bare `lucide.createIcons`.
                // Real browser propaguje window props -> global env. My here check
                // window object (= globalThis) props.
                if let Some(window) = self.global.borrow().get("window") {
                    if let JsValue::Object(obj) = window {
                        let val = obj.borrow().get(name);
                        if !matches!(val, JsValue::Undefined) {
                            return Ok(val);
                        }
                    }
                }
                Err(JsError::Runtime(format!("ReferenceError: '{name}' není definováno")))
            }

            Expr::Template { quasis, expressions } => {
                let mut s = quasis[0].clone();
                for (i, e) in expressions.iter().enumerate() {
                    s.push_str(&self.eval(e, env)?.to_string());
                    if let Some(q) = quasis.get(i + 1) { s.push_str(q); }
                }
                Ok(JsValue::Str(s))
            }

            Expr::Array(items) => {
                let mut arr = Vec::new();
                for item in items {
                    arr.push(match item { Some(e) => self.eval(e, env)?, None => JsValue::Undefined });
                }
                Ok(JsValue::Array(Rc::new(RefCell::new(arr))))
            }

            Expr::Object(props) => {
                let mut obj = JsObject::new();
                for p in props {
                    // ES2018 object spread: `{ ...src }` - copy enumerable own
                    // props z src do current obj. Pres null/undefined ignore.
                    if matches!(p.key, PropKey::Spread) {
                        let src = self.eval(&p.value, env)?;
                        if let JsValue::Object(o) = src {
                            let src_obj = o.borrow();
                            for (k, v) in src_obj.props.iter() {
                                obj.set(k.clone(), v.clone());
                            }
                        }
                        continue;
                    }
                    let key = match &p.key {
                        PropKey::Ident(s) | PropKey::Str(s) => s.clone(),
                        PropKey::Num(n) => n.to_string(),
                        PropKey::Computed(e) => self.eval(e, env)?.to_string(),
                        PropKey::Spread => unreachable!(),
                    };
                    let val = self.eval(&p.value, env)?;
                    obj.set(key, val);
                }
                Ok(JsValue::Object(Rc::new(RefCell::new(obj))))
            }

            Expr::Function { name, params, body } => Ok(JsValue::Function(JsFunc::User {
                name: name.clone(), params: params.clone(),
                body: FuncBody::Stmts(body.clone()), env: Rc::clone(env),
            })),

            // Generator funkcni vyraz: `const gen = function*() { yield 1; }`
            Expr::GeneratorFunc { name, params, body } => Ok(JsValue::Function(JsFunc::Generator {
                name: name.clone(),
                params: params.clone(),
                body: body.clone(),
                env: Rc::clone(env),
            })),

            // Async funkcni vyraz: `const f = async function() {}` nebo `async () => {}`
            Expr::AsyncFunc { name, params, body } => Ok(JsValue::Function(JsFunc::Async {
                name: name.clone(),
                params: params.clone(),
                body: FuncBody::Stmts(body.clone()),
                env: Rc::clone(env),
            })),

            // Await vyraz: `await promise` - synchronne rozbaluje Promise
            Expr::Await { value } => {
                let val = self.eval(value, env)?;
                // Rozbal promise pokud to je Promise
                match unwrap_promise_result(val) {
                    Ok(v) => Ok(v),
                    Err(reason) => Err(JsError::Thrown(reason)),
                }
            }

            // Yield vyraz: `yield value`
            Expr::Yield { value, delegate } => {
                let val = if let Some(e) = value {
                    self.eval(e, env)?
                } else {
                    JsValue::Undefined
                };
                if *delegate {
                    // yield* - delegace na iterable (rozlozi do yield_buffer)
                    let items = self.collect_iterable(val.clone())?;
                    if let Some(buf) = &mut self.yield_buffer {
                        buf.extend(items);
                    }
                    Ok(JsValue::Undefined)
                } else if let Some(buf) = &mut self.yield_buffer {
                    buf.push(val);
                    Ok(JsValue::Undefined)
                } else {
                    Err(JsError::Runtime("yield lze pouzit jen v generator funkci".into()))
                }
            }

            Expr::Arrow { params, body } => Ok(JsValue::Function(JsFunc::User {
                name: None, params: params.clone(),
                body: match body {
                    ArrowBody::Block(b) => FuncBody::Stmts(b.clone()),
                    ArrowBody::Expr(e)  => FuncBody::Expr(e.clone()),
                },
                env: Rc::clone(env),
            })),

            Expr::Unary  { op, arg }          => self.eval_unary(op, arg, env),
            Expr::Binary { op, left, right }   => self.eval_binary(op, left, right, env),
            Expr::Logical { op, left, right }  => self.eval_logical(op, left, right, env),

            Expr::Ternary { test, yes, no } => {
                if self.eval(test, env)?.is_truthy() { self.eval(yes, env) } else { self.eval(no, env) }
            }

            Expr::Assign { op, target, value } => self.eval_assign(op, target, value, env),

            Expr::Call   { callee, args, optional }     => self.eval_call(callee, args, *optional, env),

            Expr::New { callee, args } => {
                let func = self.eval(callee, env)?;
                let mut arg_vals = Vec::new();
                for a in args { arg_vals.push(self.eval(a, env)?); }
                self.call_new(func, arg_vals)
            }

            Expr::Member { object, prop, optional }     => self.eval_member(object, prop, *optional, env),

            Expr::Spread(e)   => self.eval(e, env),

            Expr::Sequence(exprs) => {
                let mut last = JsValue::Undefined;
                for e in exprs { last = self.eval(e, env)?; }
                Ok(last)
            }

            Expr::ClassExpr { name, super_class, body } => {
                let super_val = if let Some(sc) = super_class {
                    Some(Box::new(self.eval(sc, env)?))
                } else { None };
                Ok(self.make_class_func(name.clone(), super_val, body, env))
            }
        }
    }

    pub(super) fn eval_unary(&mut self, op: &UnaryOp, arg: &Expr, env: &Rc<RefCell<Environment>>) -> EvalResult {
        match op {
            UnaryOp::Typeof => {
                let t = if let Expr::Ident(name) = arg {
                    // Standard env lookup, pres miss fallback na window object
                    // (browser-spec: bare ident resolves pres global props).
                    let val = env.borrow().get(name).unwrap_or_else(|| {
                        self.global.borrow().get("window")
                            .and_then(|w| if let JsValue::Object(o) = w {
                                let v = o.borrow().get(name);
                                if matches!(v, JsValue::Undefined) { None } else { Some(v) }
                            } else { None })
                            .unwrap_or(JsValue::Undefined)
                    });
                    val.type_of()
                } else {
                    self.eval(arg, env)?.type_of()
                };
                Ok(JsValue::Str(t.to_string()))
            }
            UnaryOp::Void   => { self.eval(arg, env)?; Ok(JsValue::Undefined) }
            UnaryOp::Not    => Ok(JsValue::Bool(!self.eval(arg, env)?.is_truthy())),
            UnaryOp::Minus  => {
                let v = self.eval(arg, env)?;
                match v {
                    JsValue::BigInt(n)    => Ok(JsValue::BigInt(Rc::new(-n.as_ref().clone()))),
                    JsValue::BigNumber(n) => Ok(JsValue::BigNumber(Rc::new(-n.as_ref().clone()))),
                    other => Ok(JsValue::Number(-other.to_number())),
                }
            }
            UnaryOp::Plus   => {
                // +bigint je TypeError v JS (nelze koercovat). Zde permisivni: vrat BigInt jako BigInt.
                let v = self.eval(arg, env)?;
                match v {
                    JsValue::BigInt(_) | JsValue::BigNumber(_) => Ok(v),
                    other => Ok(JsValue::Number(other.to_number())),
                }
            }
            UnaryOp::BitNot => {
                let v = self.eval(arg, env)?;
                match v {
                    JsValue::BigInt(n) => Ok(JsValue::BigInt(Rc::new(!n.as_ref().clone()))),
                    other => Ok(JsValue::Number(!(other.to_number() as i32) as f64)),
                }
            }
            UnaryOp::Delete => {
                if let Expr::Member { object, prop, .. } = arg {
                    let obj = self.eval(object, env)?;
                    let key = self.resolve_prop_key(prop, env)?;
                    if let JsValue::Object(o) = &obj { o.borrow_mut().props.remove(&key); }
                }
                Ok(JsValue::Bool(true))
            }
            UnaryOp::PreInc => {
                let v = self.eval(arg, env)?.to_number() + 1.0;
                self.assign_to(arg, JsValue::Number(v), env)?;
                Ok(JsValue::Number(v))
            }
            UnaryOp::PreDec => {
                let v = self.eval(arg, env)?.to_number() - 1.0;
                self.assign_to(arg, JsValue::Number(v), env)?;
                Ok(JsValue::Number(v))
            }
        }
    }

    pub(super) fn eval_binary(&mut self, op: &BinaryOp, left: &Expr, right: &Expr, env: &Rc<RefCell<Environment>>) -> EvalResult {
        if *op == BinaryOp::PostInc {
            let old = self.eval(left, env)?.to_number();
            self.assign_to(left, JsValue::Number(old + 1.0), env)?;
            return Ok(JsValue::Number(old));
        }
        if *op == BinaryOp::PostDec {
            let old = self.eval(left, env)?.to_number();
            self.assign_to(left, JsValue::Number(old - 1.0), env)?;
            return Ok(JsValue::Number(old));
        }

        let l = self.eval(left, env)?;
        let r = self.eval(right, env)?;

        // BigInt aritmetika: pokud aspon jeden operand je BigInt a ZADNY neni BigNumber,
        // proved operaci v BigInt presnosti. BigInt+BigNumber spada do BigNumber vetve nize.
        let has_bigint    = matches!(&l, JsValue::BigInt(_)) || matches!(&r, JsValue::BigInt(_));
        let has_bignumber = matches!(&l, JsValue::BigNumber(_)) || matches!(&r, JsValue::BigNumber(_));
        if has_bigint && !has_bignumber {
            if let (Some(la), Some(ra)) = (l.to_bigint(), r.to_bigint()) {
                match op {
                    BinaryOp::Add  => return Ok(JsValue::BigInt(Rc::new(la + ra))),
                    BinaryOp::Sub  => return Ok(JsValue::BigInt(Rc::new(la - ra))),
                    BinaryOp::Mul  => return Ok(JsValue::BigInt(Rc::new(la * ra))),
                    BinaryOp::Div  => {
                        if NumZero::is_zero(&ra) { return Err(JsError::Runtime("RangeError: deleni nulou v BigInt".into())); }
                        return Ok(JsValue::BigInt(Rc::new(la / ra)));
                    }
                    BinaryOp::Mod  => {
                        if NumZero::is_zero(&ra) { return Err(JsError::Runtime("RangeError: modulo nulou v BigInt".into())); }
                        return Ok(JsValue::BigInt(Rc::new(la % ra)));
                    }
                    BinaryOp::Exp  => {
                        // BigInt umocneni: exponent musi byt nezaporny
                        let exp = ra.to_u32().unwrap_or(0);
                        return Ok(JsValue::BigInt(Rc::new(la.pow(exp))));
                    }
                    BinaryOp::Lt   => return Ok(JsValue::Bool(la < ra)),
                    BinaryOp::Gt   => return Ok(JsValue::Bool(la > ra)),
                    BinaryOp::LtEq => return Ok(JsValue::Bool(la <= ra)),
                    BinaryOp::GtEq => return Ok(JsValue::Bool(la >= ra)),
                    BinaryOp::StrictEq    => {
                        // Strict eq vyzaduje stejny typ - BigInt vs Number neni striktne stejne
                        let same_type = matches!(&l, JsValue::BigInt(_)) && matches!(&r, JsValue::BigInt(_));
                        return Ok(JsValue::Bool(same_type && la == ra));
                    }
                    BinaryOp::StrictNotEq => {
                        let same_type = matches!(&l, JsValue::BigInt(_)) && matches!(&r, JsValue::BigInt(_));
                        return Ok(JsValue::Bool(!(same_type && la == ra)));
                    }
                    BinaryOp::Eq    => return Ok(JsValue::Bool(la == ra)),
                    BinaryOp::NotEq => return Ok(JsValue::Bool(la != ra)),
                    BinaryOp::BitAnd => return Ok(JsValue::BigInt(Rc::new(la & ra))),
                    BinaryOp::BitOr  => return Ok(JsValue::BigInt(Rc::new(la | ra))),
                    BinaryOp::BitXor => return Ok(JsValue::BigInt(Rc::new(la ^ ra))),
                    BinaryOp::Shl  => {
                        let shift = ra.to_i64().unwrap_or(0);
                        if shift >= 0 {
                            return Ok(JsValue::BigInt(Rc::new(la << shift as u32)));
                        } else {
                            return Ok(JsValue::BigInt(Rc::new(la >> (-shift) as u32)));
                        }
                    }
                    BinaryOp::Shr => {
                        let shift = ra.to_i64().unwrap_or(0);
                        if shift >= 0 {
                            return Ok(JsValue::BigInt(Rc::new(la >> shift as u32)));
                        } else {
                            return Ok(JsValue::BigInt(Rc::new(la << (-shift) as u32)));
                        }
                    }
                    _ => {} // Ostatni - pust dal
                }
            }
        }

        // BigNumber aritmetika: pokud aspon jeden operand je BigNumber,
        // preved oba na BigDecimal a proved operaci
        if matches!((&l, &r), (JsValue::BigNumber(_), _) | (_, JsValue::BigNumber(_))) {
            if let (Some(la), Some(ra)) = (l.to_bigdecimal(), r.to_bigdecimal()) {
                match op {
                    BinaryOp::Add  => return Ok(JsValue::BigNumber(Rc::new(la + ra))),
                    BinaryOp::Sub  => return Ok(JsValue::BigNumber(Rc::new(la - ra))),
                    BinaryOp::Mul  => return Ok(JsValue::BigNumber(Rc::new(la * ra))),
                    BinaryOp::Div  => {
                        if ra.is_zero() { return Ok(JsValue::Number(f64::NAN)); }
                        return Ok(JsValue::BigNumber(Rc::new(la / ra)));
                    }
                    BinaryOp::Mod  => {
                        if ra.is_zero() { return Ok(JsValue::Number(f64::NAN)); }
                        return Ok(JsValue::BigNumber(Rc::new(la % ra)));
                    }
                    BinaryOp::Exp  => {
                        let exp = ra.to_u64().unwrap_or(0);
                        return Ok(JsValue::BigNumber(Rc::new(bigdecimal_pow(la, exp))));
                    }
                    BinaryOp::Lt   => return Ok(JsValue::Bool(la < ra)),
                    BinaryOp::Gt   => return Ok(JsValue::Bool(la > ra)),
                    BinaryOp::LtEq => return Ok(JsValue::Bool(la <= ra)),
                    BinaryOp::GtEq => return Ok(JsValue::Bool(la >= ra)),
                    BinaryOp::StrictEq    => return Ok(JsValue::Bool(la == ra)),
                    BinaryOp::StrictNotEq => return Ok(JsValue::Bool(la != ra)),
                    BinaryOp::Eq    => return Ok(JsValue::Bool(la == ra)),
                    BinaryOp::NotEq => return Ok(JsValue::Bool(la != ra)),
                    _ => {} // Ostatni operace - pust dal jako cislo
                }
            }
        }

        Ok(match op {
            BinaryOp::Add => match (&l, &r) {
                (JsValue::Str(a), _) => JsValue::Str(format!("{a}{r}")),
                (_, JsValue::Str(b)) => JsValue::Str(format!("{l}{b}")),
                _ => JsValue::Number(l.to_number() + r.to_number()),
            },
            BinaryOp::Sub  => JsValue::Number(l.to_number() - r.to_number()),
            BinaryOp::Mul  => JsValue::Number(l.to_number() * r.to_number()),
            BinaryOp::Div  => JsValue::Number(l.to_number() / r.to_number()),
            BinaryOp::Mod  => JsValue::Number(l.to_number() % r.to_number()),
            BinaryOp::Exp  => JsValue::Number(l.to_number().powf(r.to_number())),
            BinaryOp::Eq        => JsValue::Bool(l.loose_eq(&r)),
            BinaryOp::NotEq     => JsValue::Bool(!l.loose_eq(&r)),
            BinaryOp::StrictEq  => JsValue::Bool(l.strict_eq(&r)),
            BinaryOp::StrictNotEq => JsValue::Bool(!l.strict_eq(&r)),
            BinaryOp::Lt   => JsValue::Bool(l.to_number() < r.to_number()),
            BinaryOp::Gt   => JsValue::Bool(l.to_number() > r.to_number()),
            BinaryOp::LtEq => JsValue::Bool(l.to_number() <= r.to_number()),
            BinaryOp::GtEq => JsValue::Bool(l.to_number() >= r.to_number()),
            BinaryOp::BitAnd => JsValue::Number((l.to_number() as i32 & r.to_number() as i32) as f64),
            BinaryOp::BitOr  => JsValue::Number((l.to_number() as i32 | r.to_number() as i32) as f64),
            BinaryOp::BitXor => JsValue::Number((l.to_number() as i32 ^ r.to_number() as i32) as f64),
            BinaryOp::Shl    => JsValue::Number(((l.to_number() as i32) << (r.to_number() as u32 & 31)) as f64),
            BinaryOp::Shr    => JsValue::Number(((l.to_number() as i32) >> (r.to_number() as u32 & 31)) as f64),
            BinaryOp::Ushr   => JsValue::Number(((l.to_number() as u32) >> (r.to_number() as u32 & 31)) as f64),
            BinaryOp::In => {
                let key = l.to_string();
                let found = match &r {
                    JsValue::Object(o) => {
                        // Prochazi prototypovym retezcem (max 100 uroven)
                        let mut current: Option<Rc<RefCell<JsObject>>> = Some(Rc::clone(o));
                        let mut found = false;
                        let mut depth = 0;
                        while let Some(obj) = current {
                            if depth > 100 { break; }
                            if obj.borrow().props.contains_key(&key) { found = true; break; }
                            current = obj.borrow().proto.clone();
                            depth += 1;
                        }
                        found
                    }
                    _ => false,
                };
                JsValue::Bool(found)
            }
            BinaryOp::Instanceof => {
                // Ziskej jmeno tridy z praveho operandu
                let class_name = match &r {
                    JsValue::Function(JsFunc::Class { name, .. }) => {
                        name.as_deref().unwrap_or("").to_string()
                    }
                    JsValue::Function(JsFunc::Native(name, _)) => name.clone(),
                    _ => return Ok(JsValue::Bool(false)),
                };
                if class_name.is_empty() { return Ok(JsValue::Bool(false)); }
                // Zkontroluj retezec trid ulozeny na instanci (pro tridy)
                // nebo typ intern. vlastnosti (pro vestavene typy)
                match &l {
                    JsValue::Object(o) => {
                        let obj = o.borrow();
                        // Tridy: __class_chain__
                        if let Some(JsValue::Str(chain)) = obj.props.get("__class_chain__") {
                            if chain.split(',').any(|n| n == class_name) {
                                return Ok(JsValue::Bool(true));
                            }
                        }
                        // Vestavene typy podle vnitrnich klicu
                        let result = match class_name.as_str() {
                            "Error" | "TypeError" | "RangeError" | "SyntaxError"
                            | "ReferenceError" | "URIError" | "EvalError" => {
                                // Error instance ma property "name"
                                if let Some(JsValue::Str(name)) = obj.props.get("name") {
                                    class_name == "Error"
                                        || name == &class_name
                                        || name.ends_with("Error")
                                } else { false }
                            }
                            "Date"    => obj.props.contains_key("__date_ms__"),
                            "RegExp"  => obj.props.contains_key("__regex_pattern__"),
                            "Promise" => obj.props.contains_key("__promise_state__"),
                            _ => false,
                        };
                        JsValue::Bool(result)
                    }
                    JsValue::Map(_) => JsValue::Bool(class_name == "Map"),
                    JsValue::Set(_) => JsValue::Bool(class_name == "Set"),
                    JsValue::Array(_) => JsValue::Bool(class_name == "Array"),
                    JsValue::Function(_) => JsValue::Bool(class_name == "Function"),
                    _ => JsValue::Bool(false),
                }
            }
            BinaryOp::PostInc | BinaryOp::PostDec => unreachable!(),
        })
    }

    pub(super) fn eval_logical(&mut self, op: &LogicalOp, left: &Expr, right: &Expr, env: &Rc<RefCell<Environment>>) -> EvalResult {
        let l = self.eval(left, env)?;
        match op {
            LogicalOp::And      => if !l.is_truthy() { Ok(l) } else { self.eval(right, env) },
            LogicalOp::Or       => if l.is_truthy()  { Ok(l) } else { self.eval(right, env) },
            LogicalOp::NullCoal => if matches!(l, JsValue::Null | JsValue::Undefined) { self.eval(right, env) } else { Ok(l) },
        }
    }

    pub(super) fn eval_assign(&mut self, op: &AssignOp, target: &Expr, value: &Expr, env: &Rc<RefCell<Environment>>) -> EvalResult {
        // Logical assignment: short-circuit pred eval rhs
        match op {
            AssignOp::LogicalAnd => {
                let cur = self.eval(target, env)?;
                if !cur.is_truthy() { return Ok(cur); }
                let rhs = self.eval(value, env)?;
                self.assign_to(target, rhs.clone(), env)?;
                return Ok(rhs);
            }
            AssignOp::LogicalOr => {
                let cur = self.eval(target, env)?;
                if cur.is_truthy() { return Ok(cur); }
                let rhs = self.eval(value, env)?;
                self.assign_to(target, rhs.clone(), env)?;
                return Ok(rhs);
            }
            AssignOp::NullCoal => {
                let cur = self.eval(target, env)?;
                if !matches!(cur, JsValue::Null | JsValue::Undefined) { return Ok(cur); }
                let rhs = self.eval(value, env)?;
                self.assign_to(target, rhs.clone(), env)?;
                return Ok(rhs);
            }
            _ => {}
        }

        let new_val = if *op == AssignOp::Assign {
            self.eval(value, env)?
        } else {
            let old = self.eval(target, env)?;
            let rhs = self.eval(value, env)?;
            match op {
                AssignOp::Add    => match (&old, &rhs) {
                    (JsValue::Str(a), _) => JsValue::Str(format!("{a}{rhs}")),
                    _ => JsValue::Number(old.to_number() + rhs.to_number()),
                },
                AssignOp::Sub    => JsValue::Number(old.to_number() - rhs.to_number()),
                AssignOp::Mul    => JsValue::Number(old.to_number() * rhs.to_number()),
                AssignOp::Div    => JsValue::Number(old.to_number() / rhs.to_number()),
                AssignOp::Mod    => JsValue::Number(old.to_number() % rhs.to_number()),
                AssignOp::Exp    => JsValue::Number(old.to_number().powf(rhs.to_number())),
                AssignOp::BitAnd => JsValue::Number((old.to_number() as i32 & rhs.to_number() as i32) as f64),
                AssignOp::BitOr  => JsValue::Number((old.to_number() as i32 | rhs.to_number() as i32) as f64),
                AssignOp::BitXor => JsValue::Number((old.to_number() as i32 ^ rhs.to_number() as i32) as f64),
                AssignOp::Shl    => JsValue::Number(((old.to_number() as i32) << (rhs.to_number() as u32 & 31)) as f64),
                AssignOp::Shr    => JsValue::Number(((old.to_number() as i32) >> (rhs.to_number() as u32 & 31)) as f64),
                AssignOp::Ushr   => JsValue::Number(((old.to_number() as u32) >> (rhs.to_number() as u32 & 31)) as f64),
                AssignOp::Assign | AssignOp::LogicalAnd | AssignOp::LogicalOr | AssignOp::NullCoal => unreachable!(),
            }
        };
        self.assign_to(target, new_val.clone(), env)?;
        Ok(new_val)
    }

    pub(super) fn assign_to(&mut self, target: &Expr, val: JsValue, env: &Rc<RefCell<Environment>>) -> Result<(), JsError> {
        match target {
            Expr::Ident(name) => {
                if !env.borrow_mut().set(name, val.clone()) {
                    env.borrow_mut().define(name, val);
                }
                Ok(())
            }
            Expr::Member { object, prop, .. } => {
                let obj = self.eval(object, env)?;
                let key = self.resolve_prop_key(prop, env)?;
                match &obj {
                    JsValue::DomNode(n) => {
                        // DOM property setters
                        match key.as_str() {
                            "textContent" | "innerText" => {
                                n.set_text_content(&val.to_string());
                                // Content-only bump: text NEovlivnuje kaskadu (krome
                                // :empty/content() - vzacne) -> cascade cache PREZIJE.
                                // Bez tohoto FPS counter / log updaty textContent
                                // kazdy frame = full re-cascade (12ms) = <1 FPS pri
                                // animaci/RAF. Layout keyuje na dom_version (content)
                                // takze text se prelayoutuje.
                                self.bump_dom_version_content_only();
                                return Ok(());
                            }
                            "value" => {
                                // Form inputs - ulozit jako attribute "value".
                                // Content (input text), neovlivnuje kaskadu.
                                n.set_attr("value", &val.to_string());
                                self.bump_dom_version_content_only();
                                return Ok(());
                            }
                            "checked" => {
                                let s = if val.is_truthy() { "checked" } else { "" };
                                if s.is_empty() { n.remove_attr("checked"); }
                                else { n.set_attr("checked", "checked"); }
                                self.bump_dom_version();
                                return Ok(());
                            }
                            "innerHTML" => {
                                // Parse HTML fragment a nahrad children
                                let frag = crate::browser::html_parser::parse_html_fragment(&val.to_string());
                                n.children.borrow_mut().clear();
                                let frag_children: Vec<_> = frag.children.borrow().clone();
                                for ch in frag_children {
                                    // Vnoreny <html><body>... structure - extrahuj body deti
                                    let body_children: Vec<Rc<crate::browser::dom::Node>> = ch.children.borrow().clone();
                                    for grandch in body_children {
                                        n.append_child(Rc::clone(&grandch));
                                    }
                                }
                                self.bump_dom_version();
                                return Ok(());
                            }
                            "id" => {
                                n.set_attr("id", &val.to_string());
                                self.bump_dom_version();
                                return Ok(());
                            }
                            "className" => {
                                n.set_attr("class", &val.to_string());
                                self.bump_dom_version();
                                return Ok(());
                            }
                            "scrollTop" | "scrollLeft" => {
                                // Element-level scroll setter. Host (WebView)
                                // sync element_scroll_overrides -> element_scroll
                                // per frame. Bez teto override interp NEMA pristup
                                // k WV.element_scroll directly.
                                let v = match val {
                                    JsValue::Number(num) => num as f32,
                                    JsValue::Str(ref s) => s.parse::<f32>().unwrap_or(0.0),
                                    _ => return Ok(()),
                                };
                                // Aktualne i set attribute pres getter compat
                                // (eval_member.rs cte n.attr(scrollTop)).
                                n.set_attr(&key, &v.to_string());
                                let ptr = std::rc::Rc::as_ptr(n) as usize;
                                let mut overrides = self.element_scroll_overrides.borrow_mut();
                                let entry = overrides.entry(ptr).or_insert((0.0, 0.0));
                                if key == "scrollTop" { entry.1 = v; } else { entry.0 = v; }
                                return Ok(());
                            }
                            "width" | "height" if matches!(n.tag_name().as_deref(),
                                Some("canvas") | Some("img") | Some("svg")) => {
                                // canvas.width/height = N -> zapis do attr (layout +
                                // getter to ctou). Drive se write tise zahodil ->
                                // canvas.width vracelo 0 -> vsechna kreslici matematika
                                // degenerovana (particles na x=0, wave 0 iteraci).
                                let num = match val {
                                    JsValue::Number(num) => num,
                                    JsValue::Str(ref s) => s.trim().parse::<f64>().unwrap_or(0.0),
                                    _ => 0.0,
                                };
                                n.set_attr(&key, &(num as i64).to_string());
                                self.bump_dom_version();
                                return Ok(());
                            }
                            _ => {
                                // Ostatni props - ignorujeme (DomNode nema generic prop store)
                                return Ok(());
                            }
                        }
                    }
                    JsValue::Object(o) => {
                        // Worker.onmessage = fn -> registruj jako on_message v WorkerState
                        if matches!(o.borrow().props.get("__worker__"), Some(JsValue::Bool(true)))
                            && key == "onmessage"
                        {
                            let id = match o.borrow().props.get("__worker_id__").cloned() {
                                Some(JsValue::Number(n)) => n as u32,
                                _ => 0,
                            };
                            if let Some(state) = self.workers.borrow_mut().get_mut(&id) {
                                state.on_message = Some(val.clone());
                            }
                            o.borrow_mut().props.insert(key, val);
                            return Ok(());
                        }
                        // Proxy trap 'set': handler.set(target, key, value, receiver)
                        let has_handler = o.borrow().props.contains_key("__proxy_handler__");
                        if has_handler && !key.starts_with("__") {
                            let handler = o.borrow().props.get("__proxy_handler__").cloned()
                                .unwrap_or(JsValue::Undefined);
                            let target = o.borrow().props.get("__proxy_target__").cloned()
                                .unwrap_or(JsValue::Undefined);
                            if let JsValue::Object(h) = &handler {
                                let trap = h.borrow().props.get("set").cloned();
                                if let Some(trap_fn) = trap {
                                    self.call_function(
                                        trap_fn,
                                        vec![target, JsValue::Str(key.clone()), val.clone(), obj.clone()],
                                        None,
                                    )?;
                                    return Ok(());
                                }
                            }
                        }
                        // Specialni klic __proto__: prirazeni meni prototyp
                        if key == "__proto__" {
                            match &val {
                                JsValue::Object(p) => { o.borrow_mut().proto = Some(Rc::clone(p)); }
                                JsValue::Null       => { o.borrow_mut().proto = None; }
                                _ => {}
                            }
                            return Ok(());
                        }
                        // Setter podpora: kdyz objekt ma `__set_key__`, zavolej setter
                        let setter_key = format!("__set_{key}__");
                        let setter_fn = o.borrow().props.get(&setter_key).cloned();
                        if let Some(setter) = setter_fn {
                            self.call_function(setter, vec![val], Some(obj.clone()))?;
                            return Ok(());
                        }
                        // Frozen objekt: zmeny se tisnich ignoruji (soulad s JS non-strict)
                        if o.borrow().frozen { return Ok(()); }
                        // DOMTokenList.value setter - prepise cely class attr
                        // (per spec). Drzeny pres __token_list_node__ Rc<Node>.
                        if key == "value"
                            && matches!(o.borrow().props.get("__dom_token_list__"),
                                Some(JsValue::Bool(true)))
                        {
                            let node_val = o.borrow().props.get("__token_list_node__").cloned();
                            if let Some(JsValue::DomNode(n)) = node_val {
                                n.set_attr("class", &val.to_string());
                                o.borrow_mut().props.insert("value".into(), val);
                                return Ok(());
                            }
                        }
                        // Style object proxy: pokud ma `__style_node__` (internal Rc<Node>),
                        // sync prop do node.set_attr("style", ...).
                        // Skip metody (setProperty etc.) a internal `__...__` props.
                        if !key.starts_with("__")
                            && !matches!(key.as_str(),
                                "setProperty" | "getPropertyValue" | "removeProperty" | "cssText")
                        {
                            let style_node = o.borrow().props.get("__style_node__").cloned();
                            if let Some(JsValue::DomNode(n)) = style_node {
                                let value_str = val.to_string();
                                super::dom_props::update_style_attr(&n, &key, &value_str);
                            }
                        }
                        // cssText specialni - prepise cely style
                        if key == "cssText" {
                            let style_node = o.borrow().props.get("__style_node__").cloned();
                            if let Some(JsValue::DomNode(n)) = style_node {
                                n.set_attr("style", &val.to_string());
                            }
                        }
                        o.borrow_mut().props.insert(key, val);
                        Ok(())
                    }
                    JsValue::Array(a) => {
                        if key == "length" {
                            let new_len = val.to_number() as usize;
                            let mut arr = a.borrow_mut();
                            if new_len < arr.len() { arr.truncate(new_len); }
                            else { while arr.len() < new_len { arr.push(JsValue::Undefined); } }
                        } else if let Ok(idx) = key.parse::<usize>() {
                            let mut arr = a.borrow_mut();
                            while arr.len() <= idx { arr.push(JsValue::Undefined); }
                            arr[idx] = val;
                        }
                        Ok(())
                    }
                    _ => Err(JsError::Runtime(format!("Nelze priradit do vlastnosti '{key}'")))
                }
            }
            _ => Err(JsError::Runtime("Neplatny cil prirazeni".into())),
        }
    }

    // ─── Destrukturovani ──────────────────────────────────────────────────────

    /// Binduje hodnotu `val` do promenne/promennych definovanych vzorem `pattern`.
    ///
    /// Pouziva se pri:
    /// - `const [a, b] = arr` (Stmt::Var s Array/Object pattern)
    /// - `function f({ x, y }) {}` (parametry funkci)
    /// - `for (const [k, v] of ...)` (ForOf/ForIn pres bind_target_expr)
    ///
    /// Vsechny deklarovane promenne jsou definovany v `env`.
    pub(super) fn destructure_bind(&mut self, pattern: &Pattern, val: JsValue, env: &Rc<RefCell<Environment>>) -> Result<(), JsError> {
        match pattern {
            Pattern::Ident(name) => {
                env.borrow_mut().define(name, val);
                Ok(())
            }

            Pattern::Array(elems) => {
                let items: Vec<JsValue> = match &val {
                    JsValue::Array(a) => a.borrow().clone(),
                    // retezec lze destrukturovat jako pole znaku
                    JsValue::Str(s) => s.chars().map(|c| JsValue::Str(c.to_string())).collect(),
                    _ => vec![],
                };
                let mut i = 0usize;
                for elem in elems {
                    let Some(pat) = &elem.pattern else {
                        // hole: preskoc pozici
                        i += 1;
                        continue;
                    };
                    if elem.rest {
                        // ...rest = vsechny zbyvajici prvky
                        let rest = JsValue::Array(Rc::new(RefCell::new(
                            items.get(i..).unwrap_or(&[]).to_vec()
                        )));
                        self.destructure_bind(pat, rest, env)?;
                        break;
                    }
                    let item = items.get(i).cloned().unwrap_or(JsValue::Undefined);
                    let item = if matches!(item, JsValue::Undefined) {
                        if let Some(def) = &elem.default {
                            self.eval(def, env)?
                        } else { item }
                    } else { item };
                    self.destructure_bind(pat, item, env)?;
                    i += 1;
                }
                Ok(())
            }

            Pattern::Object(props) => {
                // Klice ktere uz byly spotrebovany (pro ...rest - zatim neni implementovan)
                for prop in props {
                    let key = match &prop.key {
                        PropKey::Ident(s) | PropKey::Str(s) => s.clone(),
                        PropKey::Num(n) => format!("{}", *n as i64),
                        PropKey::Computed(e) => self.eval(e, env)?.to_string(),
                        // PropKey::Spread v destrukturalizaci = `{...rest}` rest pattern.
                        // Aktualne ne-implementovany - skip prop (= bind nic).
                        PropKey::Spread => continue,
                    };
                    let item = match &val {
                        JsValue::Object(o) => o.borrow().get(&key),
                        _ => JsValue::Undefined,
                    };
                    let item = if matches!(item, JsValue::Undefined) {
                        if let Some(def) = &prop.default {
                            self.eval(def, env)?
                        } else { item }
                    } else { item };
                    self.destructure_bind(&prop.pattern, item, env)?;
                }
                Ok(())
            }
        }
    }

    /// Binduje hodnotu `val` do cile ulozeného jako Expr.
    ///
    /// Pouziva se pro ForOf/ForIn target, kde AST uklada target jako `Expr`
    /// (preveden z Pattern pres `pattern_to_expr` v parseru).
    ///
    /// Podporuje:
    /// - `Expr::Ident` - jednoducha promenna
    /// - `Expr::Array` - array destrukturovani `[a, b]`
    /// - `Expr::Object` - object destrukturovani `{ x, y }`
    pub(super) fn bind_target_expr(&mut self, target: &Expr, val: JsValue, env: &Rc<RefCell<Environment>>) -> Result<(), JsError> {
        match target {
            Expr::Ident(name) => {
                env.borrow_mut().define(name, val);
                Ok(())
            }
            Expr::Array(items) => {
                let vals: Vec<JsValue> = match &val {
                    JsValue::Array(a) => a.borrow().clone(),
                    JsValue::Str(s) => s.chars().map(|c| JsValue::Str(c.to_string())).collect(),
                    _ => vec![],
                };
                let mut i = 0usize;
                for item in items {
                    let Some(expr) = item else {
                        // hole
                        i += 1;
                        continue;
                    };
                    // rest element je ulozen jako Spread(inner)
                    if let Expr::Spread(inner) = expr.as_ref() {
                        let rest = JsValue::Array(Rc::new(RefCell::new(
                            vals.get(i..).unwrap_or(&[]).to_vec()
                        )));
                        self.bind_target_expr(inner, rest, env)?;
                        break;
                    }
                    let v = vals.get(i).cloned().unwrap_or(JsValue::Undefined);
                    self.bind_target_expr(expr, v, env)?;
                    i += 1;
                }
                Ok(())
            }
            Expr::Object(props) => {
                for prop in props {
                    let key = match &prop.key {
                        PropKey::Ident(s) | PropKey::Str(s) => s.clone(),
                        PropKey::Num(n) => format!("{}", *n as i64),
                        PropKey::Computed(e) => {
                            let e = e.as_ref().clone();
                            self.eval(&e, env)?.to_string()
                        }
                        // PropKey::Spread v destructuring assign = ...rest target.
                        // Aktualne ne-implementovany - skip.
                        PropKey::Spread => continue,
                    };
                    let v = match &val {
                        JsValue::Object(o) => o.borrow().get(&key),
                        _ => JsValue::Undefined,
                    };
                    self.bind_target_expr(&prop.value, v, env)?;
                }
                Ok(())
            }
            // Pro prirazeni (x = ...) pouzij assign_to
            other => {
                self.assign_to(other, val, env)
            }
        }
    }

}
