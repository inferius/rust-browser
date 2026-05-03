/// ⚙️ Evaluator - Interpretuje AST a vrací výsledky
///
/// Evaluator procházejí stromem (AST) a vykonává operace.
/// Spravuje Environment (proměnné, funkce, atd).

use crate::ast::*;
use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;

/// Hodnota v JavaScriptu
#[derive(Debug, Clone)]
pub enum Value {
    Number(f64),
    String(String),
    Boolean(bool),
    Null,
    Undefined,
    Object(Rc<RefCell<HashMap<String, Value>>>),
    Array(Rc<RefCell<Vec<Value>>>),
    Function {
        params: Vec<String>,
        body: Vec<Statement>,
        closure: Environment,
    },
}

impl Value {
    /// Konverze na boolean (pro if, while, atd)
    pub fn to_boolean(&self) -> bool {
        match self {
            Value::Boolean(b) => *b,
            Value::Null | Value::Undefined => false,
            Value::Number(n) => *n != 0.0 && !n.is_nan(),
            Value::String(s) => !s.is_empty(),
            _ => true,
        }
    }

    /// Konverze na string (pro výpisy)
    pub fn to_string(&self) -> String {
        match self {
            Value::Number(n) => {
                // JS formátuje čísla speciálně
                if n.fract() == 0.0 && n.is_finite() {
                    format!("{:.0}", n)
                } else {
                    n.to_string()
                }
            }
            Value::String(s) => s.clone(),
            Value::Boolean(b) => b.to_string(),
            Value::Null => "null".to_string(),
            Value::Undefined => "undefined".to_string(),
            Value::Object(_) => "[object Object]".to_string(),
            Value::Array(_) => "[Array]".to_string(),
            Value::Function { .. } => "[Function]".to_string(),
        }
    }

    /// Konverze na číslo
    pub fn to_number(&self) -> f64 {
        match self {
            Value::Number(n) => *n,
            Value::Boolean(b) => if *b { 1.0 } else { 0.0 },
            Value::Null => 0.0,
            Value::Undefined => f64::NAN,
            Value::String(s) => s.parse::<f64>().unwrap_or(f64::NAN),
            _ => f64::NAN,
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Number(a), Value::Number(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Boolean(a), Value::Boolean(b)) => a == b,
            (Value::Null, Value::Null) => true,
            (Value::Undefined, Value::Undefined) => true,
            _ => false,
        }
    }
}

/// Prostředí - uchovává proměnné a funkce
#[derive(Debug, Clone)]
pub struct Environment {
    scopes: Vec<HashMap<String, Value>>,
}

impl Environment {
    pub fn new() -> Self {
        let mut env = Environment {
            scopes: vec![HashMap::new()],
        };
        env.setup_globals();
        env
    }

    fn setup_globals(&mut self) {
        // Přidat globální funkce později (console.log, atd)
    }

    pub fn define(&mut self, name: String, value: Value) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, value);
        }
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(value) = scope.get(name) {
                return Some(value.clone());
            }
        }
        None
    }

    pub fn set(&mut self, name: String, value: Value) -> Result<(), String> {
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(&name) {
                scope.insert(name, value);
                return Ok(());
            }
        }
        // Pokud proměnná neexistuje, vytvoříme ji v aktuální scope
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, value);
            Ok(())
        } else {
            Err("No scope available".to_string())
        }
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }
}

/// Control flow signály
#[derive(Debug)]
pub enum ControlFlow {
    None,
    Return(Value),
    Break,
    Continue,
}

/// Evaluator
pub struct Evaluator {
    environment: Environment,
}

impl Evaluator {
    pub fn new() -> Self {
        Evaluator {
            environment: Environment::new(),
        }
    }

    /// Interpretuje program
    pub fn eval(&mut self, program: Program) -> Result<Value, String> {
        let mut last_value = Value::Undefined;

        for statement in program.statements {
            match self.eval_statement(&statement)? {
                ControlFlow::None => {}
                ControlFlow::Return(v) => {
                    last_value = v;
                    break;
                }
                ControlFlow::Break | ControlFlow::Continue => {
                    return Err("Unexpected break/continue".to_string());
                }
            }
        }

        Ok(last_value)
    }

    fn eval_statement(&mut self, stmt: &Statement) -> Result<ControlFlow, String> {
        match stmt {
            Statement::VariableDeclaration { kind: _, declarations } => {
                for decl in declarations {
                    let value = if let Some(init) = &decl.init {
                        self.eval_expression(init)?
                    } else {
                        Value::Undefined
                    };
                    self.environment.define(decl.id.clone(), value);
                }
                Ok(ControlFlow::None)
            }

            Statement::ExpressionStatement(expr) => {
                self.eval_expression(expr)?;
                Ok(ControlFlow::None)
            }

            Statement::FunctionDeclaration { name, params, body } => {
                let func = Value::Function {
                    params: params.clone(),
                    body: body.clone(),
                    closure: self.environment.clone(),
                };
                self.environment.define(name.clone(), func);
                Ok(ControlFlow::None)
            }

            Statement::IfStatement { test, consequent, alternate } => {
                let test_value = self.eval_expression(test)?;
                if test_value.to_boolean() {
                    for stmt in consequent {
                        match self.eval_statement(stmt)? {
                            ControlFlow::None => {}
                            cf => return Ok(cf),
                        }
                    }
                } else if let Some(alt) = alternate {
                    for stmt in alt {
                        match self.eval_statement(stmt)? {
                            ControlFlow::None => {}
                            cf => return Ok(cf),
                        }
                    }
                }
                Ok(ControlFlow::None)
            }

            Statement::WhileStatement { test, body } => {
                loop {
                    let test_value = self.eval_expression(test)?;
                    if !test_value.to_boolean() {
                        break;
                    }

                    for stmt in body {
                        match self.eval_statement(stmt)? {
                            ControlFlow::None => {}
                            ControlFlow::Break => return Ok(ControlFlow::None),
                            ControlFlow::Continue => break, // přejdi na další iteraci
                            cf => return Ok(cf),
                        }
                    }
                }
                Ok(ControlFlow::None)
            }

            Statement::ReturnStatement(expr) => {
                let value = if let Some(e) = expr {
                    self.eval_expression(e)?
                } else {
                    Value::Undefined
                };
                Ok(ControlFlow::Return(value))
            }

            Statement::BreakStatement => Ok(ControlFlow::Break),
            Statement::ContinueStatement => Ok(ControlFlow::Continue),

            Statement::BlockStatement(stmts) => {
                self.environment.push_scope();
                let mut result = ControlFlow::None;

                for stmt in stmts {
                    match self.eval_statement(stmt)? {
                        ControlFlow::None => {}
                        cf => {
                            result = cf;
                            break;
                        }
                    }
                }

                self.environment.pop_scope();
                Ok(result)
            }

            _ => Ok(ControlFlow::None),
        }
    }

    fn eval_expression(&mut self, expr: &Expression) -> Result<Value, String> {
        match expr {
            Expression::Literal(lit) => Ok(self.eval_literal(lit)),

            Expression::Identifier(name) => {
                self.environment.get(name)
                    .ok_or_else(|| format!("Undefined variable: {}", name))
            }

            Expression::BinaryExpression { left, operator, right } => {
                let left_val = self.eval_expression(left)?;
                let right_val = self.eval_expression(right)?;
                self.eval_binary_op(&left_val, *operator, &right_val)
            }

            Expression::UnaryExpression { operator, argument } => {
                let arg_val = self.eval_expression(argument)?;
                self.eval_unary_op(*operator, &arg_val)
            }

            Expression::AssignmentExpression { left, right } => {
                let value = self.eval_expression(right)?;

                if let Expression::Identifier(name) = &**left {
                    self.environment.set(name.clone(), value.clone())?;
                    Ok(value)
                } else {
                    Err("Invalid assignment target".to_string())
                }
            }

            Expression::LogicalExpression { left, operator, right } => {
                let left_val = self.eval_expression(left)?;

                match operator {
                    LogicalOperator::And => {
                        if !left_val.to_boolean() {
                            Ok(left_val)
                        } else {
                            self.eval_expression(right)
                        }
                    }
                    LogicalOperator::Or => {
                        if left_val.to_boolean() {
                            Ok(left_val)
                        } else {
                            self.eval_expression(right)
                        }
                    }
                }
            }

            Expression::CallExpression { callee, arguments } => {
                let func_val = self.eval_expression(callee)?;

                // Speciální případ: console.log
                if let Expression::MemberExpression { object, property, .. } = &**callee {
                    if let Expression::Identifier(obj_name) = &**object {
                        if obj_name == "console" {
                            if let Expression::Literal(Literal::String(method)) = &**property {
                                if method == "log" {
                                    let args: Vec<String> = arguments
                                        .iter()
                                        .map(|a| {
                                            self.eval_expression(a).map(|v| v.to_string())
                                        })
                                        .collect::<Result<Vec<_>, _>>()?;
                                    println!("{}", args.join(", "));
                                    return Ok(Value::Undefined);
                                }
                            }
                        }
                    }
                }

                match func_val {
                    Value::Function { params, body, closure } => {
                        if arguments.len() != params.len() {
                            return Err(format!(
                                "Function expects {} arguments, got {}",
                                params.len(),
                                arguments.len()
                            ));
                        }

                        // Vyhodnotit argumenty
                        let arg_values: Vec<Value> = arguments
                            .iter()
                            .map(|a| self.eval_expression(a))
                            .collect::<Result<Vec<_>, _>>()?;

                        // Vytvořit nové prostředí pro funkci
                        let saved_env = std::mem::replace(&mut self.environment, closure);
                        self.environment.push_scope();

                        // Bind parameters
                        for (param, arg) in params.iter().zip(arg_values) {
                            self.environment.define(param.clone(), arg);
                        }

                        // Vykonat tělo funkce
                        let mut result = Value::Undefined;
                        for stmt in &body {
                            match self.eval_statement(stmt)? {
                                ControlFlow::None => {}
                                ControlFlow::Return(v) => {
                                    result = v;
                                    break;
                                }
                                _ => {}
                            }
                        }

                        self.environment.pop_scope();
                        self.environment = saved_env;

                        Ok(result)
                    }
                    _ => Err("Not a function".to_string()),
                }
            }

            Expression::ConditionalExpression { test, consequent, alternate } => {
                let test_val = self.eval_expression(test)?;
                if test_val.to_boolean() {
                    self.eval_expression(consequent)
                } else {
                    self.eval_expression(alternate)
                }
            }

            _ => Ok(Value::Undefined),
        }
    }

    fn eval_literal(&self, lit: &Literal) -> Value {
        match lit {
            Literal::Number(n) => Value::Number(*n),
            Literal::String(s) => Value::String(s.clone()),
            Literal::Boolean(b) => Value::Boolean(*b),
            Literal::Null => Value::Null,
            Literal::Undefined => Value::Undefined,
        }
    }

    fn eval_binary_op(&self, left: &Value, op: BinaryOperator, right: &Value) -> Result<Value, String> {
        match op {
            BinaryOperator::Add => {
                // String concatenation nebo numeric addition
                if matches!(left, Value::String(_)) || matches!(right, Value::String(_)) {
                    Ok(Value::String(format!("{}{}", left.to_string(), right.to_string())))
                } else {
                    Ok(Value::Number(left.to_number() + right.to_number()))
                }
            }
            BinaryOperator::Subtract => Ok(Value::Number(left.to_number() - right.to_number())),
            BinaryOperator::Multiply => Ok(Value::Number(left.to_number() * right.to_number())),
            BinaryOperator::Divide => Ok(Value::Number(left.to_number() / right.to_number())),
            BinaryOperator::Modulo => Ok(Value::Number(left.to_number() % right.to_number())),
            BinaryOperator::Exponent => Ok(Value::Number(left.to_number().powf(right.to_number()))),

            BinaryOperator::Equal => {
                // Loose equality
                Ok(Value::Boolean(
                    left.to_number() == right.to_number() ||
                        left.to_string() == right.to_string() ||
                        left == right
                ))
            }
            BinaryOperator::StrictEqual => Ok(Value::Boolean(left == right)),
            BinaryOperator::NotEqual => {
                let eq = left.to_number() == right.to_number() ||
                    left.to_string() == right.to_string() ||
                    left == right;
                Ok(Value::Boolean(!eq))
            }
            BinaryOperator::StrictNotEqual => Ok(Value::Boolean(left != right)),

            BinaryOperator::Less => Ok(Value::Boolean(left.to_number() < right.to_number())),
            BinaryOperator::LessEqual => Ok(Value::Boolean(left.to_number() <= right.to_number())),
            BinaryOperator::Greater => Ok(Value::Boolean(left.to_number() > right.to_number())),
            BinaryOperator::GreaterEqual => Ok(Value::Boolean(left.to_number() >= right.to_number())),

            _ => Err(format!("Operator {:?} not implemented", op)),
        }
    }

    fn eval_unary_op(&self, op: UnaryOperator, arg: &Value) -> Result<Value, String> {
        match op {
            UnaryOperator::Minus => Ok(Value::Number(-arg.to_number())),
            UnaryOperator::Plus => Ok(Value::Number(arg.to_number())),
            UnaryOperator::Not => Ok(Value::Boolean(!arg.to_boolean())),
            UnaryOperator::Typeof => {
                let type_str = match arg {
                    Value::Number(_) => "number",
                    Value::String(_) => "string",
                    Value::Boolean(_) => "boolean",
                    Value::Null => "object", // JS oddity
                    Value::Undefined => "undefined",
                    Value::Function { .. } => "function",
                    _ => "object",
                };
                Ok(Value::String(type_str.to_string()))
            }
            _ => Err(format!("Unary operator {:?} not implemented", op)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_to_boolean() {
        assert_eq!(Value::Boolean(true).to_boolean(), true);
        assert_eq!(Value::Boolean(false).to_boolean(), false);
        assert_eq!(Value::Number(0.0).to_boolean(), false);
        assert_eq!(Value::Number(1.0).to_boolean(), true);
        assert_eq!(Value::String("".to_string()).to_boolean(), false);
        assert_eq!(Value::String("hello".to_string()).to_boolean(), true);
        assert_eq!(Value::Null.to_boolean(), false);
        assert_eq!(Value::Undefined.to_boolean(), false);
    }

    #[test]
    fn test_value_to_string() {
        assert_eq!(Value::Number(42.0).to_string(), "42");
        assert_eq!(Value::String("hello".to_string()).to_string(), "hello");
        assert_eq!(Value::Boolean(true).to_string(), "true");
        assert_eq!(Value::Null.to_string(), "null");
    }

    #[test]
    fn test_simple_arithmetic() {
        let mut eval = Evaluator::new();

        // let x = 5;
        // let y = 3;
        // x + y = 8

        // Budeme testovat až propojíme parser
    }
}
