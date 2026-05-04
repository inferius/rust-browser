/// BigNumber - arbitrary precision decimal (bigdecimal).

use super::helpers::*;

#[test]
fn bignumber_typeof() {
    let v = run(r#"return typeof BigNumber;"#);
    assert_eq!(as_str(v), "function");
}

#[test]
fn bignumber_basic_arithmetic() {
    assert_eq!(as_str(run(r#"
        const a = new BigNumber("100");
        const b = new BigNumber("200");
        return a.plus(b).toString();
    "#)), "300");
}

#[test]
fn bignumber_large_number() {
    assert_eq!(as_str(run(r#"
        const a = new BigNumber("999999999999999999999999999999");
        const b = new BigNumber("1");
        return a.plus(b).toString();
    "#)), "1000000000000000000000000000000");
}

#[test]
fn bignumber_times_div() {
    let v = run(r#"
        const a = new BigNumber("6");
        const b = new BigNumber("7");
        return a.times(b).toNumber();
    "#);
    assert_eq!(as_num(v), 42.0);
}

#[test]
fn bignumber_comparison() {
    assert_eq!(as_bool(run(r#"
        return new BigNumber("10").gt(new BigNumber("5"));
    "#)), true);
    assert_eq!(as_bool(run(r#"
        return new BigNumber("3").lt(new BigNumber("5"));
    "#)), true);
    assert_eq!(as_bool(run(r#"
        return new BigNumber("7").eq(new BigNumber("7"));
    "#)), true);
}

#[test]
fn bignumber_to_fixed() {
    assert_eq!(as_str(run(r#"
        return new BigNumber("3.14159").toFixed(2);
    "#)), "3.14");
}

#[test]
fn bignumber_abs_neg() {
    assert_eq!(as_str(run(r#"
        return new BigNumber("-42").abs().toString();
    "#)), "42");
    assert_eq!(as_str(run(r#"
        return new BigNumber("5").negated().toString();
    "#)), "-5");
}

#[test]
fn bignumber_pow() {
    assert_eq!(as_str(run(r#"
        return new BigNumber("2").pow(10).toString();
    "#)), "1024");
}

#[test]
fn bignumber_is_zero_is_positive() {
    assert_eq!(as_bool(run(r#"return new BigNumber("0").isZero();"#)), true);
    assert_eq!(as_bool(run(r#"return new BigNumber("5").isPositive();"#)), true);
    assert_eq!(as_bool(run(r#"return new BigNumber("-3").isNegative();"#)), true);
}
