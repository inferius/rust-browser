/// Staticky import/export + dynamicky import().

use super::helpers::*;

#[test]
fn import_default() {
    let v = run_with_modules(
        r#"
            import greet from "greeter";
            return greet("svet");
        "#,
        &[("greeter", r#"
            export default function(name) { return "ahoj " + name; }
        "#)],
    );
    assert_eq!(as_str(v), "ahoj svet");
}

#[test]
fn import_named() {
    let v = run_with_modules(
        r#"
            import { add, sub } from "math";
            return add(10, 3) * 100 + sub(10, 3);
        "#,
        &[("math", r#"
            export function add(a, b) { return a + b; }
            export function sub(a, b) { return a - b; }
        "#)],
    );
    assert_eq!(as_num(v), 1307.0);
}

#[test]
fn import_named_with_alias() {
    let v = run_with_modules(
        r#"
            import { value as PI } from "consts";
            return PI;
        "#,
        &[("consts", r#"export const value = 3.14;"#)],
    );
    assert_eq!(as_num(v), 3.14);
}

#[test]
fn import_namespace() {
    let v = run_with_modules(
        r#"
            import * as utils from "utils";
            return utils.double(21);
        "#,
        &[("utils", r#"
            export function double(x) { return x * 2; }
            export const name = "utils";
        "#)],
    );
    assert_eq!(as_num(v), 42.0);
}

#[test]
fn import_default_and_named_combined() {
    let v = run_with_modules(
        r#"
            import main, { helper } from "lib";
            return main() + helper();
        "#,
        &[("lib", r#"
            export default function() { return 100; }
            export function helper() { return 23; }
        "#)],
    );
    assert_eq!(as_num(v), 123.0);
}

#[test]
fn import_side_effect_only() {
    let v = run_with_modules(
        r#"
            import "side";
            return "ok";
        "#,
        &[("side", r#"
            const x = 42;
        "#)],
    );
    assert_eq!(as_str(v), "ok");
}

#[test]
fn export_named_list() {
    let v = run_with_modules(
        r#"
            import { x, y } from "vars";
            return x + y;
        "#,
        &[("vars", r#"
            const x = 10;
            const y = 32;
            export { x, y };
        "#)],
    );
    assert_eq!(as_num(v), 42.0);
}

#[test]
fn export_named_list_with_alias() {
    let v = run_with_modules(
        r#"
            import { renamed } from "vars";
            return renamed;
        "#,
        &[("vars", r#"
            const original = 99;
            export { original as renamed };
        "#)],
    );
    assert_eq!(as_num(v), 99.0);
}

#[test]
fn export_default_value() {
    let v = run_with_modules(
        r#"
            import x from "default-only";
            return x;
        "#,
        &[("default-only", r#"export default 42;"#)],
    );
    assert_eq!(as_num(v), 42.0);
}

#[test]
fn export_class() {
    let v = run_with_modules(
        r#"
            import { Animal } from "animals";
            const a = new Animal("rex");
            return a.name;
        "#,
        &[("animals", r#"
            export class Animal {
                constructor(name) { this.name = name; }
            }
        "#)],
    );
    assert_eq!(as_str(v), "rex");
}

#[test]
fn module_cache_executes_once() {
    let v = run_with_modules(
        r#"
            import { count } from "counter";
            import { count as c2 } from "counter";
            return count + c2;
        "#,
        &[("counter", r#"
            export const count = 1;
        "#)],
    );
    assert_eq!(as_num(v), 2.0);
}

// ─── Dynamicky import() ──────────────────────────────────────────

#[test]
fn dynamic_import_returns_promise() {
    let v = run_with_modules(
        r#"
            let result = "no";
            import("test-mod").then(m => { result = "yes:" + m.x; });
            return result;
        "#,
        &[("test-mod", r#"export const x = 42;"#)],
    );
    assert_eq!(as_str(v), "yes:42");
}

#[test]
fn dynamic_import_resolves_namespace() {
    let v = run_with_modules(
        r#"
            const ns = await import("dyn-mod");
            return ns.value;
        "#,
        &[("dyn-mod", r#"export const value = 100;"#)],
    );
    assert_eq!(as_num(v), 100.0);
}

#[test]
fn dynamic_import_then_chain() {
    let v = run_with_modules(
        r#"
            let result = 0;
            import("data").then(m => { result = m.x + m.y; });
            return result;
        "#,
        &[("data", r#"
            export const x = 5;
            export const y = 7;
        "#)],
    );
    assert_eq!(as_num(v), 12.0);
}

#[test]
fn dynamic_import_unknown_rejects() {
    let v = run_with_modules(
        r#"
            let err = null;
            import("nonexistent").catch(e => { err = e.message; });
            return typeof err;
        "#,
        &[],
    );
    assert_eq!(as_str(v), "string");
}
