//! Class evaluation: make_class_func, construct_class, run_super_constructor,
//! get_class_method_func, bind_params.

use super::*;

impl Interpreter {
    /// Vytvori JsValue::Function(JsFunc::Class) z AST ClassMember listu.
    ///
    /// Rozdeli cleny na: konstruktor, instance metody, staticke metody, gettery, settery.
    pub(super) fn make_class_func(
        &self,
        name: Option<String>,
        super_val: Option<Box<JsValue>>,
        body: &[ClassMember],
        env: &Rc<RefCell<Env>>,
    ) -> JsValue {
        let mut has_ctor = false;
        let mut ctor_params = Vec::new();
        let mut ctor_body   = Vec::new();
        let mut methods  = Vec::new();
        let mut statics  = Vec::new();
        let mut getters  = Vec::new();
        let mut setters  = Vec::new();

        for m in body {
            let def = ClassMethodDef {
                name: m.name.clone(),
                params: m.params.clone(),
                body: m.body.clone(),
            };
            if m.name == "constructor" && !m.is_static {
                has_ctor = true;
                ctor_params = m.params.clone();
                ctor_body   = m.body.clone();
            } else if m.is_static {
                statics.push(def);
            } else if m.is_getter {
                getters.push(def);
            } else if m.is_setter {
                setters.push(def);
            } else {
                methods.push(def);
            }
        }

        JsValue::Function(JsFunc::Class {
            name,
            super_val,
            has_ctor,
            ctor_params,
            ctor_body,
            methods,
            statics,
            getters,
            setters,
            env: Rc::clone(env),
        })
    }

    /// Konstruuje novou instanci tridy (`new Foo(args)`).
    pub(super) fn construct_class(&mut self, class_val: JsValue, args: Vec<JsValue>) -> EvalResult {
        let JsValue::Function(JsFunc::Class {
            name,
            super_val,
            has_ctor,
            ctor_params,
            ctor_body,
            methods,
            statics: _,
            getters,
            setters,
            env,
        }) = class_val else {
            return Err(JsError::Runtime("construct_class: ocekavana trida".into()));
        };

        let this_obj = Rc::new(RefCell::new(JsObject::new()));
        let this_val = JsValue::Object(Rc::clone(&this_obj));

        // Uloz retezec trid pro `instanceof`
        {
            let chain = build_class_chain(name.as_deref().unwrap_or(""), super_val.as_deref());
            if !chain.is_empty() {
                this_obj.borrow_mut().set("__class_chain__".to_string(), JsValue::Str(chain));
            }
        }

        // Env pro metody obsahuje __super_class__ (pro super.method() uvnitr metod)
        let method_env = Environment::new_child(&env);
        if let Some(sv) = &super_val {
            method_env.borrow_mut().define("__super_class__", (**sv).clone());
        }

        // Prirad instance metody objektu
        for mdef in &methods {
            let mfunc = JsValue::Function(JsFunc::User {
                name: Some(mdef.name.clone()),
                params: mdef.params.clone(),
                body: FuncBody::Stmts(mdef.body.clone()),
                env: Rc::clone(&method_env),
            });
            this_obj.borrow_mut().set(mdef.name.clone(), mfunc);
        }

        // Prirad gettery (ulozeny jako __get_name__ pro speciální eval_member handling)
        for gdef in &getters {
            let gfunc = JsValue::Function(JsFunc::User {
                name: Some(gdef.name.clone()),
                params: gdef.params.clone(),
                body: FuncBody::Stmts(gdef.body.clone()),
                env: Rc::clone(&method_env),
            });
            this_obj.borrow_mut().set(format!("__get_{}__", gdef.name), gfunc);
        }

        // Prirad settery
        for sdef in &setters {
            let sfunc = JsValue::Function(JsFunc::User {
                name: Some(sdef.name.clone()),
                params: sdef.params.clone(),
                body: FuncBody::Stmts(sdef.body.clone()),
                env: Rc::clone(&method_env),
            });
            this_obj.borrow_mut().set(format!("__set_{}__", sdef.name), sfunc);
        }

        // Konstruktor env: this + __super_class__
        let ctor_env = Environment::new_child(&env);
        ctor_env.borrow_mut().define("this", this_val.clone());
        if let Some(sv) = &super_val {
            ctor_env.borrow_mut().define("__super_class__", (**sv).clone());
        }

        if has_ctor {
            // Explicitni konstruktor - svaz parametry a spust telo
            let ctor_params = ctor_params.clone();
            self.bind_params(&ctor_params, args, &ctor_env)?;
            self.exec_stmts(&ctor_body, &ctor_env)?;
        } else if let Some(sv) = super_val {
            // Zadny konstruktor + ma super -> auto-deleguj super(args)
            self.run_super_constructor(*sv, args, &this_obj, &ctor_env)?;
        }
        // Else: zadny konstruktor, zadny super -> objekt je prazdny (vlastnosti se priradi rucne)

        Ok(this_val)
    }

    /// Spusti konstruktor rodicovske tridy na existujicim `this` objektu.
    ///
    /// Pouziva se pri `super(args)` uvnitr konstruktoru podtridy.
    /// Mutuje `this_obj` - priradi vlastnosti a metody parenta.
    pub(super) fn run_super_constructor(
        &mut self,
        super_class: JsValue,
        args: Vec<JsValue>,
        this_obj: &Rc<RefCell<JsObject>>,
        _parent_env: &Rc<RefCell<Env>>,
    ) -> Result<(), JsError> {
        match super_class {
            JsValue::Function(JsFunc::Class {
                super_val,
                has_ctor,
                ctor_params,
                ctor_body,
                methods,
                getters,
                setters,
                env,
                ..
            }) => {
                // Env pro metody parenta: super_val jako __super_class__ (pro super.method() uvnitr parenta)
                let method_env = Environment::new_child(&env);
                if let Some(sv) = &super_val {
                    method_env.borrow_mut().define("__super_class__", (**sv).clone());
                }

                // Prirad metody parenta - jen pokud uz nejsou defibovany podtridou
                for mdef in &methods {
                    if !this_obj.borrow().props.contains_key(&mdef.name) {
                        let mfunc = JsValue::Function(JsFunc::User {
                            name: Some(mdef.name.clone()),
                            params: mdef.params.clone(),
                            body: FuncBody::Stmts(mdef.body.clone()),
                            env: Rc::clone(&method_env),
                        });
                        this_obj.borrow_mut().set(mdef.name.clone(), mfunc);
                    }
                }
                for gdef in &getters {
                    let key = format!("__get_{}__", gdef.name);
                    if !this_obj.borrow().props.contains_key(&key) {
                        let gfunc = JsValue::Function(JsFunc::User {
                            name: Some(gdef.name.clone()),
                            params: gdef.params.clone(),
                            body: FuncBody::Stmts(gdef.body.clone()),
                            env: Rc::clone(&method_env),
                        });
                        this_obj.borrow_mut().set(key, gfunc);
                    }
                }
                for sdef in &setters {
                    let key = format!("__set_{}__", sdef.name);
                    if !this_obj.borrow().props.contains_key(&key) {
                        let sfunc = JsValue::Function(JsFunc::User {
                            name: Some(sdef.name.clone()),
                            params: sdef.params.clone(),
                            body: FuncBody::Stmts(sdef.body.clone()),
                            env: Rc::clone(&method_env),
                        });
                        this_obj.borrow_mut().set(key, sfunc);
                    }
                }

                // Spust konstruktor parenta
                let ctor_env = Environment::new_child(&env);
                ctor_env.borrow_mut().define("this", JsValue::Object(Rc::clone(this_obj)));
                if let Some(sv) = &super_val {
                    ctor_env.borrow_mut().define("__super_class__", (**sv).clone());
                }

                if has_ctor {
                    self.bind_params(&ctor_params, args, &ctor_env)?;
                    self.exec_stmts(&ctor_body, &ctor_env)?;
                } else if let Some(sv) = super_val {
                    // Auto-deleguj na praprarodice
                    self.run_super_constructor(*sv, args, this_obj, &ctor_env)?;
                }

                Ok(())
            }
            // Parent je stara-style function constructor (ne class)
            JsValue::Function(JsFunc::User { params, body, env, .. }) => {
                let ctor_env = Environment::new_child(&env);
                ctor_env.borrow_mut().define("this", JsValue::Object(Rc::clone(this_obj)));
                self.bind_params(&params, args, &ctor_env)?;
                if let FuncBody::Stmts(stmts) = body {
                    self.exec_stmts(&stmts, &ctor_env)?;
                }
                Ok(())
            }
            // Native funkce jako super (napr. HTMLElement) - no-op, zadny stav neni treba prenest
            JsValue::Function(JsFunc::Native(..)) => Ok(()),
            _ => Err(JsError::Runtime("super(): rodicovska hodnota neni trida".into()))
        }
    }

    /// Ziska metodu z tridy pro `super.method()` volani.
    ///
    /// Prochazi hierarchii trid (super_val retezec) pokud metoda neni nalezena.
    /// Vraci `JsValue::Function` nebo `JsValue::Undefined`.
    pub(super) fn get_class_method_func(&self, class_val: &JsValue, name: &str) -> EvalResult {
        match class_val {
            JsValue::Function(JsFunc::Class { super_val, methods, env, .. }) => {
                for mdef in methods {
                    if mdef.name == name {
                        // Env metody: obsahuje __super_class__ pro dalsi super.method() volani
                        let method_env = Environment::new_child(env);
                        if let Some(sv) = super_val {
                            method_env.borrow_mut().define("__super_class__", (**sv).clone());
                        }
                        return Ok(JsValue::Function(JsFunc::User {
                            name: Some(mdef.name.clone()),
                            params: mdef.params.clone(),
                            body: FuncBody::Stmts(mdef.body.clone()),
                            env: Rc::clone(&method_env),
                        }));
                    }
                }
                // Metoda nenalezena - zkus v super (pro vicenasobnou dedicnost)
                if let Some(sv) = super_val {
                    return self.get_class_method_func(sv, name);
                }
                Ok(JsValue::Undefined)
            }
            _ => Ok(JsValue::Undefined),
        }
    }

    /// Svaze parametry funkce s argumenty do `env`.
    ///
    /// Refaktorovana spolecna logika pouzivana v `call_function`,
    /// `construct_class` i `run_super_constructor`.
    pub(super) fn bind_params(
        &mut self,
        params: &[Param],
        args: Vec<JsValue>,
        env: &Rc<RefCell<Env>>,
    ) -> Result<(), JsError> {
        let mut arg_idx = 0usize;
        for p in params {
            if p.rest {
                let rest = JsValue::Array(Rc::new(RefCell::new(
                    args.get(arg_idx..).unwrap_or(&[]).to_vec()
                )));
                self.destructure_bind(&p.pattern, rest, env)?;
                break;
            }
            let val = args.get(arg_idx).cloned().unwrap_or(JsValue::Undefined);
            let val = if matches!(val, JsValue::Undefined) {
                if let Some(def) = &p.default {
                    let de = *def.clone();
                    self.eval(&de, env)?
                } else { val }
            } else { val };
            self.destructure_bind(&p.pattern, val, env)?;
            arg_idx += 1;
        }
        Ok(())
    }
}
