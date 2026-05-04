/// Generatory + iterator protokol + Symbol.iterator.

use super::helpers::*;

#[test]
fn generator_basic_yield() {
    assert_eq!(as_num(run(r#"
        function* gen() {
            yield 1;
            yield 2;
            yield 3;
        }
        const it = gen();
        const a = it.next().value;
        const b = it.next().value;
        const c = it.next().value;
        return a + b + c;
    "#)), 6.0);
}

#[test]
fn generator_done_flag() {
    assert_eq!(as_bool(run(r#"
        function* gen() { yield 1; }
        const it = gen();
        it.next();
        return it.next().done;
    "#)), true);
}

#[test]
fn generator_for_of() {
    assert_eq!(as_num(run(r#"
        function* range(n) {
            for (let i = 0; i < n; i++) {
                yield i;
            }
        }
        let sum = 0;
        for (const x of range(5)) {
            sum += x;
        }
        return sum;
    "#)), 10.0);
}

#[test]
fn generator_expression() {
    assert_eq!(as_num(run(r#"
        const gen = function*() {
            yield 10;
            yield 20;
        };
        let sum = 0;
        for (const x of gen()) { sum += x; }
        return sum;
    "#)), 30.0);
}

#[test]
fn generator_yield_star_array() {
    assert_eq!(as_num(run(r#"
        function* gen() {
            yield* [1, 2, 3];
            yield 4;
        }
        let sum = 0;
        for (const x of gen()) { sum += x; }
        return sum;
    "#)), 10.0);
}

#[test]
fn generator_yield_star_other_gen() {
    assert_eq!(as_num(run(r#"
        function* inner() { yield 1; yield 2; }
        function* outer() { yield* inner(); yield 3; }
        let sum = 0;
        for (const x of outer()) { sum += x; }
        return sum;
    "#)), 6.0);
}

#[test]
fn symbol_iterator_custom_iterable() {
    assert_eq!(as_num(run(r#"
        const range = {
            from: 1,
            to: 5,
            [Symbol.iterator]() {
                let i = this.from;
                const to = this.to;
                return {
                    next() {
                        if (i <= to) {
                            return { value: i++, done: false };
                        }
                        return { value: undefined, done: true };
                    }
                };
            }
        };
        let sum = 0;
        for (const x of range) { sum += x; }
        return sum;
    "#)), 15.0);
}

#[test]
fn symbol_iterator_string_concat_key() {
    assert_eq!(as_str(run(r#"
        return Symbol.iterator;
    "#)), "Symbol.iterator");
}

#[test]
fn generator_parser_function_star_decl() {
    assert_eq!(as_num(run(r#"
        function* nums() { yield 1; yield 2; yield 3; }
        const arr = [];
        for (const n of nums()) { arr.push(n); }
        return arr.length;
    "#)), 3.0);
}

#[test]
fn generator_next_returns_object_with_value_and_done() {
    assert_eq!(as_bool(run(r#"
        function* g() { yield 42; }
        const it = g();
        const step = it.next();
        return step.value === 42 && step.done === false;
    "#)), true);
}

#[test]
fn generator_multiple_calls() {
    assert_eq!(as_num(run(r#"
        function* gen() { yield 1; yield 2; }
        const it1 = gen();
        const it2 = gen();
        it1.next();
        return it2.next().value;
    "#)), 1.0);
}

#[test]
fn generator_with_params() {
    assert_eq!(as_num(run(r#"
        function* take(arr, n) {
            for (let i = 0; i < n && i < arr.length; i++) {
                yield arr[i];
            }
        }
        let sum = 0;
        for (const x of take([10, 20, 30, 40], 3)) { sum += x; }
        return sum;
    "#)), 60.0);
}
