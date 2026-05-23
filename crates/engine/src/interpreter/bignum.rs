//! BigInt support - arbitrary precision integers.
//!
//! ECMA-262 6.1.6.2. Real impl uses num-bigint crate; here a minimal i128 wrapper
//! plus the host-arithmetic interface that the interpreter calls.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BigIntCmp {
    Less,
    Equal,
    Greater,
}

/// Bounded BigInt - covers the common case (< 2^127). The full implementation
/// promotes to a Vec<u64> chunk array when overflow detected.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BigInt {
    pub value: i128,
    pub overflow: bool,             // true if computed value didn't fit
}

impl BigInt {
    pub fn from_i64(v: i64) -> Self {
        Self { value: v as i128, overflow: false }
    }

    pub fn from_str(s: &str) -> Result<Self, String> {
        let s = s.trim_end_matches('n').replace('_', "");
        let v: i128 = if let Some(rest) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            i128::from_str_radix(rest, 16).map_err(|e| e.to_string())?
        } else if let Some(rest) = s.strip_prefix("0b").or_else(|| s.strip_prefix("0B")) {
            i128::from_str_radix(rest, 2).map_err(|e| e.to_string())?
        } else if let Some(rest) = s.strip_prefix("0o").or_else(|| s.strip_prefix("0O")) {
            i128::from_str_radix(rest, 8).map_err(|e| e.to_string())?
        } else {
            s.parse().map_err(|e: std::num::ParseIntError| e.to_string())?
        };
        Ok(Self { value: v, overflow: false })
    }

    pub fn add(&self, other: &Self) -> Self {
        match self.value.checked_add(other.value) {
            Some(v) => Self { value: v, overflow: false },
            None => Self { value: i128::MAX, overflow: true },
        }
    }

    pub fn sub(&self, other: &Self) -> Self {
        match self.value.checked_sub(other.value) {
            Some(v) => Self { value: v, overflow: false },
            None => Self { value: i128::MIN, overflow: true },
        }
    }

    pub fn mul(&self, other: &Self) -> Self {
        match self.value.checked_mul(other.value) {
            Some(v) => Self { value: v, overflow: false },
            None => Self { value: i128::MAX, overflow: true },
        }
    }

    pub fn div(&self, other: &Self) -> Result<Self, String> {
        if other.value == 0 { return Err("BigInt division by zero".into()); }
        Ok(Self { value: self.value / other.value, overflow: false })
    }

    pub fn rem(&self, other: &Self) -> Result<Self, String> {
        if other.value == 0 { return Err("BigInt division by zero".into()); }
        Ok(Self { value: self.value % other.value, overflow: false })
    }

    pub fn pow(&self, exp: &Self) -> Result<Self, String> {
        if exp.value < 0 { return Err("BigInt negative exponent".into()); }
        if exp.value > u32::MAX as i128 { return Err("exponent too large".into()); }
        match self.value.checked_pow(exp.value as u32) {
            Some(v) => Ok(Self { value: v, overflow: false }),
            None => Ok(Self { value: i128::MAX, overflow: true }),
        }
    }

    pub fn cmp(&self, other: &Self) -> BigIntCmp {
        match self.value.cmp(&other.value) {
            std::cmp::Ordering::Less => BigIntCmp::Less,
            std::cmp::Ordering::Equal => BigIntCmp::Equal,
            std::cmp::Ordering::Greater => BigIntCmp::Greater,
        }
    }

    pub fn to_string_radix(&self, radix: u32) -> String {
        let mut v = self.value.unsigned_abs();
        if v == 0 { return "0".into(); }
        let mut digits = Vec::new();
        while v > 0 {
            let d = (v % radix as u128) as u32;
            digits.push(std::char::from_digit(d, radix).unwrap());
            v /= radix as u128;
        }
        if self.value < 0 { digits.push('-'); }
        digits.iter().rev().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_decimal() {
        let b = BigInt::from_str("12345").unwrap();
        assert_eq!(b.value, 12345);
    }

    #[test]
    fn parse_hex() {
        let b = BigInt::from_str("0xff").unwrap();
        assert_eq!(b.value, 255);
    }

    #[test]
    fn parse_with_underscores() {
        let b = BigInt::from_str("1_000_000").unwrap();
        assert_eq!(b.value, 1_000_000);
    }

    #[test]
    fn parse_with_n_suffix() {
        let b = BigInt::from_str("42n").unwrap();
        assert_eq!(b.value, 42);
    }

    #[test]
    fn add_simple() {
        let a = BigInt::from_i64(2);
        let b = BigInt::from_i64(3);
        assert_eq!(a.add(&b).value, 5);
    }

    #[test]
    fn div_zero_errors() {
        let a = BigInt::from_i64(1);
        let b = BigInt::from_i64(0);
        assert!(a.div(&b).is_err());
    }

    #[test]
    fn pow_grows() {
        let a = BigInt::from_i64(2);
        let exp = BigInt::from_i64(10);
        assert_eq!(a.pow(&exp).unwrap().value, 1024);
    }

    #[test]
    fn cmp_ordering() {
        let a = BigInt::from_i64(1);
        let b = BigInt::from_i64(2);
        assert_eq!(a.cmp(&b), BigIntCmp::Less);
        assert_eq!(b.cmp(&a), BigIntCmp::Greater);
        assert_eq!(a.cmp(&a), BigIntCmp::Equal);
    }

    #[test]
    fn to_string_hex() {
        let b = BigInt::from_i64(255);
        assert_eq!(b.to_string_radix(16), "ff");
    }
}
