/// Destrukturovani pole / objektu, spread operator, nested patterns.

use super::helpers::*;

#[test]
fn array_destructuring_basic() {
    assert_eq!(as_num(run("const [a, b] = [1, 2]; return a + b;")), 3.0);
}

#[test]
fn array_destructuring_skip() {
    assert_eq!(as_num(run("const [a, , c] = [1, 2, 3]; return a + c;")), 4.0);
}

#[test]
fn array_destructuring_default() {
    assert_eq!(as_num(run("const [a, b = 99] = [1]; return b;")), 99.0);
    assert_eq!(as_num(run("const [a, b = 99] = [1, 5]; return b;")), 5.0);
}

#[test]
fn array_destructuring_rest() {
    assert_eq!(as_num(run("const [a, ...rest] = [1, 2, 3]; return rest.length;")), 2.0);
    assert_eq!(as_num(run("const [a, ...rest] = [1, 2, 3]; return rest[0];")), 2.0);
}

#[test]
fn object_destructuring_basic() {
    assert_eq!(as_num(run("const { x, y } = { x: 10, y: 20 }; return x + y;")), 30.0);
}

#[test]
fn object_destructuring_rename() {
    assert_eq!(as_num(run("const { x: a, y: b } = { x: 3, y: 4 }; return a + b;")), 7.0);
}

#[test]
fn object_destructuring_default() {
    assert_eq!(as_num(run("const { x = 42 } = {}; return x;")), 42.0);
    assert_eq!(as_num(run("const { x = 42 } = { x: 5 }; return x;")), 5.0);
}

#[test]
fn nested_array_destructuring() {
    assert_eq!(as_num(run("const [[a, b], c] = [[1, 2], 3]; return a + b + c;")), 6.0);
}

#[test]
fn nested_object_destructuring() {
    assert_eq!(as_num(run("const { a: { b } } = { a: { b: 99 } }; return b;")), 99.0);
}

#[test]
fn function_param_array_destructuring() {
    assert_eq!(as_num(run(r#"
        function sum([a, b]) { return a + b; }
        return sum([10, 20]);
    "#)), 30.0);
}

#[test]
fn function_param_object_destructuring() {
    assert_eq!(as_num(run(r#"
        function greet({ name, age = 0 }) { return age; }
        return greet({ name: "Alice", age: 25 });
    "#)), 25.0);
}

#[test]
fn function_param_object_default() {
    assert_eq!(as_num(run(r#"
        function f({ x = 10 }) { return x; }
        return f({});
    "#)), 10.0);
}

#[test]
fn for_of_array_destructuring() {
    assert_eq!(as_num(run(r#"
        let sum = 0;
        for (const [k, v] of [[1, 10], [2, 20]]) {
            sum += k + v;
        }
        return sum;
    "#)), 33.0);
}

#[test]
fn for_of_object_destructuring() {
    assert_eq!(as_num(run(r#"
        let sum = 0;
        for (const { x, y } of [{ x: 1, y: 2 }, { x: 3, y: 4 }]) {
            sum += x + y;
        }
        return sum;
    "#)), 10.0);
}

#[test]
fn destructuring_in_arrow_params() {
    assert_eq!(as_num(run(r#"
        const fn = ([a, b]) => a + b;
        return fn([5, 6]);
    "#)), 11.0);
}
