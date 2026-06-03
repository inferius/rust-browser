/// if/else, while, for, switch, try-catch, labeled break/continue.

use super::helpers::*;

#[test]
fn if_true_branch() {
    assert_eq!(as_num(run("if (true) { return 1; } return 2;")), 1.0);
}

#[test]
fn if_false_branch() {
    assert_eq!(as_num(run("if (false) { return 1; } return 2;")), 2.0);
}

#[test]
fn if_else_stmt() {
    assert_eq!(as_num(run("let x = 5; if (x > 3) { return 1; } else { return 0; }")), 1.0);
}

#[test]
fn ternary_operator() {
    assert_eq!(as_num(eval("true ? 1 : 2")), 1.0);
    assert_eq!(as_num(eval("false ? 1 : 2")), 2.0);
}

#[test]
fn while_loop() {
    assert_eq!(as_num(run(r#"
        let sum = 0;
        let i = 0;
        while (i < 5) { sum += i; i++; }
        return sum;
    "#)), 10.0);
}

#[test]
fn for_loop() {
    assert_eq!(as_num(run(r#"
        let sum = 0;
        for (let i = 0; i < 5; i++) { sum += i; }
        return sum;
    "#)), 10.0);
}

#[test]
fn for_break() {
    assert_eq!(as_num(run(r#"
        let x = 0;
        for (let i = 0; i < 10; i++) {
            if (i === 3) break;
            x = i;
        }
        return x;
    "#)), 2.0);
}

#[test]
fn for_continue() {
    assert_eq!(as_num(run(r#"
        let sum = 0;
        for (let i = 0; i < 5; i++) {
            if (i === 2) continue;
            sum += i;
        }
        return sum;
    "#)), 8.0);
}

#[test]
fn try_catch_basic() {
    assert_eq!(as_str(run(r#"
        try {
            throw "oops";
        } catch (e) {
            return e;
        }
    "#)), "oops");
}

#[test]
fn try_catch_no_throw() {
    assert_eq!(as_num(run(r#"
        let x = 0;
        try { x = 5; } catch (e) { x = 99; }
        return x;
    "#)), 5.0);
}

#[test]
fn for_of_array() {
    assert_eq!(as_num(run(r#"
        let sum = 0;
        for (const x of [1, 2, 3, 4]) { sum += x; }
        return sum;
    "#)), 10.0);
}

#[test]
fn for_in_object() {
    assert_eq!(as_num(run(r#"
        const obj = { a: 1, b: 2, c: 3 };
        let count = 0;
        for (const k in obj) { count++; }
        return count;
    "#)), 3.0);
}

#[test]
fn switch_basic_match() {
    assert_eq!(as_num(run(r#"
        let x = 2;
        switch (x) {
            case 1: return 10;
            case 2: return 20;
            case 3: return 30;
        }
        return 0;
    "#)), 20.0);
}

#[test]
fn switch_default_only() {
    assert_eq!(as_num(run(r#"
        switch (99) {
            case 1: return 1;
            default: return 42;
        }
    "#)), 42.0);
}

#[test]
fn switch_default_in_middle() {
    assert_eq!(as_num(run(r#"
        switch (5) {
            case 1: return 1;
            default: return 99;
            case 2: return 2;
        }
    "#)), 99.0);
}

#[test]
fn switch_no_match_no_default() {
    assert_eq!(as_num(run(r#"
        switch (7) {
            case 1: return 1;
            case 2: return 2;
        }
        return 0;
    "#)), 0.0);
}

#[test]
fn switch_fallthrough() {
    assert_eq!(as_num(run(r#"
        let result = 0;
        switch (1) {
            case 1: result += 10;
            case 2: result += 20;
            case 3: result += 30; break;
            case 4: result += 40;
        }
        return result;
    "#)), 60.0);
}

#[test]
fn switch_break_stops() {
    assert_eq!(as_num(run(r#"
        let result = 0;
        switch (2) {
            case 1: result = 1; break;
            case 2: result = 2; break;
            case 3: result = 3; break;
        }
        return result;
    "#)), 2.0);
}

#[test]
fn switch_multiple_cases_same_body() {
    assert_eq!(as_str(run(r#"
        function grade(n) {
            switch (n) {
                case 1:
                case 2: return "low";
                case 3: return "mid";
                case 4:
                case 5: return "high";
                default: return "unknown";
            }
        }
        return grade(2);
    "#)), "low");
}

#[test]
fn switch_strict_equality() {
    assert_eq!(as_num(run(r#"
        switch ("1") {
            case 1:  return 10;
            case "1": return 20;
        }
        return 0;
    "#)), 20.0);
}

#[test]
fn switch_string_discriminant() {
    assert_eq!(as_num(run(r#"
        const day = "mon";
        switch (day) {
            case "sat":
            case "sun": return 0;
            case "mon": return 1;
            default: return -1;
        }
    "#)), 1.0);
}

#[test]
fn switch_with_block_scope() {
    assert_eq!(as_num(run(r#"
        switch (1) {
            case 1: {
                let x = 42;
                return x;
            }
        }
        return 0;
    "#)), 42.0);
}

#[test]
fn labeled_break_outer_loop() {
    assert_eq!(as_num(run(r#"
        let result = 0;
        outer: for (let i = 0; i < 3; i++) {
            for (let j = 0; j < 3; j++) {
                if (i === 1 && j === 1) break outer;
                result++;
            }
        }
        return result;
    "#)), 4.0);
}
