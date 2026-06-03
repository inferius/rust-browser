//! Intl.NumberFormat - locale-aware decimal/percent/currency formatting.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NumberStyle {
    Decimal,
    Currency,
    Percent,
    Unit,
}

#[derive(Debug, Clone)]
pub struct NumberFormatOptions {
    pub style: NumberStyle,
    pub minimum_integer_digits: u32,
    pub minimum_fraction_digits: u32,
    pub maximum_fraction_digits: u32,
    pub use_grouping: bool,
    pub group_separator: char,
    pub decimal_separator: char,
    pub currency: Option<String>,
    pub currency_display: String,
    pub locale: String,
}

impl NumberFormatOptions {
    pub fn english() -> Self {
        Self {
            style: NumberStyle::Decimal,
            minimum_integer_digits: 1,
            minimum_fraction_digits: 0,
            maximum_fraction_digits: 3,
            use_grouping: true,
            group_separator: ',', decimal_separator: '.',
            currency: None,
            currency_display: "symbol".into(),
            locale: "en-US".into(),
        }
    }

    pub fn czech() -> Self {
        Self {
            style: NumberStyle::Decimal,
            minimum_integer_digits: 1,
            minimum_fraction_digits: 0,
            maximum_fraction_digits: 3,
            use_grouping: true,
            group_separator: '\u{00A0}', // non-breaking space
            decimal_separator: ',',
            currency: None,
            currency_display: "symbol".into(),
            locale: "cs-CZ".into(),
        }
    }
}

pub fn format_number(value: f64, opts: &NumberFormatOptions) -> String {
    let abs = value.abs();
    let neg = value < 0.0;
    let mut s = if opts.style == NumberStyle::Percent {
        format_decimal(abs * 100.0, opts)
    } else {
        format_decimal(abs, opts)
    };
    match opts.style {
        NumberStyle::Percent => s.push('%'),
        NumberStyle::Currency => {
            if let Some(c) = &opts.currency {
                let sym = currency_symbol(c, &opts.locale);
                s = format!("{}{}", sym, s);
            }
        }
        _ => {}
    }
    if neg { s.insert(0, '-'); }
    s
}

fn format_decimal(value: f64, opts: &NumberFormatOptions) -> String {
    let pow = 10f64.powi(opts.maximum_fraction_digits as i32);
    let scaled = (value * pow).round();
    let int_part = (scaled / pow).abs() as u64;
    let frac_part = ((scaled - int_part as f64 * pow).abs() as u64) % pow as u64;

    let mut int_str = int_part.to_string();
    while int_str.len() < opts.minimum_integer_digits as usize {
        int_str.insert(0, '0');
    }
    if opts.use_grouping && int_str.len() > 3 {
        let mut out = String::with_capacity(int_str.len() + int_str.len() / 3);
        for (i, c) in int_str.chars().rev().enumerate() {
            if i > 0 && i % 3 == 0 { out.push(opts.group_separator); }
            out.push(c);
        }
        int_str = out.chars().rev().collect();
    }

    let mut frac_str = format!("{:0>width$}", frac_part, width = opts.maximum_fraction_digits as usize);
    // Trim trailing zeros beyond min.
    while frac_str.ends_with('0') && frac_str.len() > opts.minimum_fraction_digits as usize {
        frac_str.pop();
    }
    if frac_str.is_empty() { int_str }
    else { format!("{}{}{}", int_str, opts.decimal_separator, frac_str) }
}

fn currency_symbol(code: &str, locale: &str) -> &'static str {
    match (code, locale) {
        ("USD", _) => "$",
        ("EUR", _) => "\u{20AC}",
        ("GBP", _) => "\u{00A3}",
        ("JPY", _) => "\u{00A5}",
        ("CZK", _) => "K\u{010D} ",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_basic() {
        let opts = NumberFormatOptions::english();
        assert_eq!(format_number(1234.5, &opts), "1,234.5");
    }

    #[test]
    fn czech_grouping_and_decimal() {
        let opts = NumberFormatOptions::czech();
        let s = format_number(1234.5, &opts);
        assert!(s.contains(','));
        assert!(s.contains('\u{00A0}'));
    }

    #[test]
    fn percent_multiplies() {
        let mut opts = NumberFormatOptions::english();
        opts.style = NumberStyle::Percent;
        opts.maximum_fraction_digits = 1;
        assert_eq!(format_number(0.5, &opts), "50%");
    }

    #[test]
    fn negative_prefix() {
        let opts = NumberFormatOptions::english();
        let s = format_number(-1.5, &opts);
        assert!(s.starts_with('-'));
    }

    #[test]
    fn currency_with_symbol() {
        let mut opts = NumberFormatOptions::english();
        opts.style = NumberStyle::Currency;
        opts.currency = Some("USD".into());
        opts.minimum_fraction_digits = 2;
        opts.maximum_fraction_digits = 2;
        let s = format_number(99.5, &opts);
        assert!(s.starts_with('$'));
        assert!(s.contains("99.50"));
    }

    #[test]
    fn min_integer_padding() {
        let mut opts = NumberFormatOptions::english();
        opts.minimum_integer_digits = 4;
        opts.use_grouping = false;
        opts.maximum_fraction_digits = 0;
        assert_eq!(format_number(7.0, &opts), "0007");
    }
}
