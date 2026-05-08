use super::*;

#[test]
fn insert_at_cursor() {
    let mut i = ConsoleInput::new();
    i.insert("ab");
    assert_eq!(i.text, "ab");
    assert_eq!(i.cursor, 2);
    i.insert("c");
    assert_eq!(i.text, "abc");
}

#[test]
fn backspace_removes_left() {
    let mut i = ConsoleInput::new();
    i.insert("hello");
    i.backspace();
    assert_eq!(i.text, "hell");
    assert_eq!(i.cursor, 4);
}

#[test]
fn backspace_unicode() {
    let mut i = ConsoleInput::new();
    i.insert("ahoj");
    i.insert("č");
    assert_eq!(i.text, "ahojč");
    i.backspace();
    assert_eq!(i.text, "ahoj");
}

#[test]
fn move_left_right() {
    let mut i = ConsoleInput::new();
    i.insert("abc");
    i.move_left(false);
    assert_eq!(i.cursor, 2);
    i.insert("X");
    assert_eq!(i.text, "abXc");
    assert_eq!(i.cursor, 3);
    i.move_right(false);
    assert_eq!(i.cursor, 4);
}

#[test]
fn home_end() {
    let mut i = ConsoleInput::new();
    i.insert("hello");
    i.move_home(false);
    assert_eq!(i.cursor, 0);
    i.move_end(false);
    assert_eq!(i.cursor, 5);
}

#[test]
fn select_all_and_replace() {
    let mut i = ConsoleInput::new();
    i.insert("hello world");
    i.select_all();
    assert!(i.has_selection());
    assert_eq!(i.selected_text().as_deref(), Some("hello world"));
    i.insert("X");
    assert_eq!(i.text, "X");
    assert_eq!(i.cursor, 1);
    assert!(!i.has_selection());
}

#[test]
fn shift_arrow_extends_selection() {
    let mut i = ConsoleInput::new();
    i.insert("hello");
    i.move_left(true);
    i.move_left(true);
    assert!(i.has_selection());
    assert_eq!(i.selected_text().as_deref(), Some("lo"));
}

#[test]
fn cut_returns_text() {
    let mut i = ConsoleInput::new();
    i.insert("abcdef");
    i.move_home(false);
    i.move_right(true);
    i.move_right(true);
    i.move_right(true);
    let s = i.cut();
    assert_eq!(s.as_deref(), Some("abc"));
    assert_eq!(i.text, "def");
    assert_eq!(i.cursor, 0);
}

#[test]
fn submit_resets_and_pushes_history() {
    let mut i = ConsoleInput::new();
    i.insert("foo()");
    let cmd = i.submit();
    assert_eq!(cmd, "foo()");
    assert_eq!(i.text, "");
    assert_eq!(i.cursor, 0);
    assert_eq!(i.history.len(), 1);
}

#[test]
fn history_prev_next() {
    let mut i = ConsoleInput::new();
    i.insert("first");
    i.submit();
    i.insert("second");
    i.submit();
    i.history_prev();
    assert_eq!(i.text, "second");
    i.history_prev();
    assert_eq!(i.text, "first");
    i.history_next();
    assert_eq!(i.text, "second");
    i.history_next();
    assert_eq!(i.text, "");
}

#[test]
fn suggest_member_access() {
    let (start, hits) = suggest("console.l", 9, &[]).expect("hits");
    assert_eq!(start, 8);
    assert!(hits.iter().any(|h| h.text == "log"));
}

#[test]
fn suggest_keyword() {
    let (start, hits) = suggest("ret", 3, &[]).expect("hits");
    assert_eq!(start, 0);
    assert!(hits.iter().any(|h| h.text == "return"));
}

#[test]
fn suggest_global() {
    let globals = vec!["myVar".to_string(), "myFunction".to_string()];
    let (_start, hits) = suggest("my", 2, &globals).expect("hits");
    assert!(hits.iter().any(|h| h.text == "myVar"));
    assert!(hits.iter().any(|h| h.text == "myFunction"));
}

#[test]
fn suggest_empty_at_cursor_zero() {
    let r = suggest("foo", 0, &[]);
    assert!(r.is_none());
}

#[test]
fn suggest_member_math() {
    let (_, hits) = suggest("Math.s", 6, &[]).expect("hits");
    assert!(hits.iter().any(|h| h.text == "sqrt"));
    assert!(hits.iter().any(|h| h.text == "sin"));
}

#[test]
fn delete_forward() {
    let mut i = ConsoleInput::new();
    i.insert("abc");
    i.move_home(false);
    i.delete_forward();
    assert_eq!(i.text, "bc");
    assert_eq!(i.cursor, 0);
}
