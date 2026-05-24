//! Intl.DateTimeFormat - calendar-aware formatting.
//!
//! Simplified: Gregorian only. ICU4X provides full calendar support; this is
//! the lightweight surface used by tests + diagnostics.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DateStyle { Full, Long, Medium, Short, None }
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimeStyle { Full, Long, Medium, Short, None }

#[derive(Debug, Clone)]
pub struct DateTimeFormatOptions {
    pub locale: String,
    pub date_style: DateStyle,
    pub time_style: TimeStyle,
    pub time_zone: String,            // IANA name; "UTC" default
    pub hour_12: bool,
}

impl Default for DateTimeFormatOptions {
    fn default() -> Self {
        Self {
            locale: "en-US".into(),
            date_style: DateStyle::Medium,
            time_style: TimeStyle::None,
            time_zone: "UTC".into(),
            hour_12: true,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CalendarDate {
    pub year: i32,
    pub month: u8,        // 1-12
    pub day: u8,          // 1-31
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

pub fn format(date: CalendarDate, opts: &DateTimeFormatOptions) -> String {
    let mut s = String::new();
    match opts.date_style {
        DateStyle::Full => s.push_str(&format!("{}, {} {} {}",
            weekday_name(date), month_name(date.month, opts), date.day, date.year)),
        DateStyle::Long => s.push_str(&format!("{} {}, {}", month_name(date.month, opts), date.day, date.year)),
        DateStyle::Medium => s.push_str(&format!("{} {}, {}",
            month_abbr(date.month, opts), date.day, date.year)),
        DateStyle::Short => {
            if opts.locale.starts_with("en") {
                s.push_str(&format!("{}/{}/{}", date.month, date.day, date.year));
            } else {
                s.push_str(&format!("{}. {}. {}", date.day, date.month, date.year));
            }
        }
        DateStyle::None => {}
    }
    if !matches!(opts.time_style, TimeStyle::None) {
        if !s.is_empty() { s.push_str(", "); }
        s.push_str(&format_time(date, opts));
    }
    s
}

fn weekday_name(_d: CalendarDate) -> &'static str { "Monday" /* placeholder */ }
fn month_name(m: u8, opts: &DateTimeFormatOptions) -> &'static str {
    if opts.locale.starts_with("cs") {
        match m {
            1 => "leden", 2 => "\u{00FA}nor", 3 => "b\u{0159}ezen", 4 => "duben",
            5 => "kv\u{011B}ten", 6 => "\u{010D}erven", 7 => "\u{010D}ervenec", 8 => "srpen",
            9 => "z\u{00E1}\u{0159}\u{00ED}", 10 => "\u{0159}\u{00ED}jen",
            11 => "listopad", 12 => "prosinec",
            _ => "?",
        }
    } else {
        match m {
            1 => "January", 2 => "February", 3 => "March", 4 => "April",
            5 => "May", 6 => "June", 7 => "July", 8 => "August",
            9 => "September", 10 => "October", 11 => "November", 12 => "December",
            _ => "?",
        }
    }
}
fn month_abbr(m: u8, opts: &DateTimeFormatOptions) -> &'static str {
    if opts.locale.starts_with("cs") { return ""; }
    match m {
        1 => "Jan", 2 => "Feb", 3 => "Mar", 4 => "Apr", 5 => "May", 6 => "Jun",
        7 => "Jul", 8 => "Aug", 9 => "Sep", 10 => "Oct", 11 => "Nov", 12 => "Dec",
        _ => "?",
    }
}

fn format_time(d: CalendarDate, opts: &DateTimeFormatOptions) -> String {
    if opts.hour_12 {
        let (h12, ampm) = if d.hour == 0 { (12, "AM") }
                          else if d.hour < 12 { (d.hour, "AM") }
                          else if d.hour == 12 { (12, "PM") }
                          else { (d.hour - 12, "PM") };
        match opts.time_style {
            TimeStyle::Short => format!("{}:{:02} {}", h12, d.minute, ampm),
            TimeStyle::Medium => format!("{}:{:02}:{:02} {}", h12, d.minute, d.second, ampm),
            _ => format!("{}:{:02}:{:02} {}", h12, d.minute, d.second, ampm),
        }
    } else {
        match opts.time_style {
            TimeStyle::Short => format!("{:02}:{:02}", d.hour, d.minute),
            _ => format!("{:02}:{:02}:{:02}", d.hour, d.minute, d.second),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u8, d: u8, h: u8, min: u8, s: u8) -> CalendarDate {
        CalendarDate { year: y, month: m, day: d, hour: h, minute: min, second: s }
    }

    #[test]
    fn english_short() {
        let opts = DateTimeFormatOptions { date_style: DateStyle::Short, ..Default::default() };
        let s = format(date(2024, 5, 1, 0, 0, 0), &opts);
        assert_eq!(s, "5/1/2024");
    }

    #[test]
    fn english_long() {
        let opts = DateTimeFormatOptions { date_style: DateStyle::Long, ..Default::default() };
        let s = format(date(2024, 1, 15, 0, 0, 0), &opts);
        assert_eq!(s, "January 15, 2024");
    }

    #[test]
    fn czech_short() {
        let opts = DateTimeFormatOptions {
            locale: "cs-CZ".into(),
            date_style: DateStyle::Short,
            ..Default::default()
        };
        let s = format(date(2024, 5, 1, 0, 0, 0), &opts);
        assert_eq!(s, "1. 5. 2024");
    }

    #[test]
    fn time_12h_pm() {
        let opts = DateTimeFormatOptions {
            date_style: DateStyle::None,
            time_style: TimeStyle::Short,
            hour_12: true,
            ..Default::default()
        };
        let s = format(date(2024, 1, 1, 14, 30, 0), &opts);
        assert_eq!(s, "2:30 PM");
    }

    #[test]
    fn time_24h() {
        let opts = DateTimeFormatOptions {
            date_style: DateStyle::None,
            time_style: TimeStyle::Short,
            hour_12: false,
            ..Default::default()
        };
        let s = format(date(2024, 1, 1, 14, 30, 0), &opts);
        assert_eq!(s, "14:30");
    }
}
