use super::*;

#[test]
fn vlq_basic_zero() {
    let v = decode_vlq_seq("A");
    assert_eq!(v, vec![0]);
}

#[test]
fn vlq_basic_positive() {
    // 'C' = 2 -> raw = 010 -> sign=0, value=1 -> +1
    let v = decode_vlq_seq("C");
    assert_eq!(v, vec![1]);
}

#[test]
fn vlq_negative_one() {
    // 'D' = 3 -> raw = 011 -> sign=1, value=1 -> -1
    let v = decode_vlq_seq("D");
    assert_eq!(v, vec![-1]);
}

#[test]
fn vlq_segment_4_values() {
    // "AAAA" - 4 chars = 4 zero values
    let v = decode_vlq_seq("AAAA");
    assert_eq!(v, vec![0, 0, 0, 0]);
}

#[test]
fn vlq_continuation() {
    // "gB" -> g=32 = continuation; B=1 -> raw=000001
    // first: cont=1, raw=00000, sign=0, value=0, shift=4
    // next: cont=0, raw=00001, value |= 1<<4 = 16
    // result: +16
    let v = decode_vlq_seq("gB");
    assert_eq!(v, vec![16]);
}

#[test]
fn detect_source_map_url_js() {
    let src = "var x = 1;\n//# sourceMappingURL=app.js.map\n";
    let url = detect_source_map_url(src, SourceLang::JavaScript);
    assert_eq!(url.as_deref(), Some("app.js.map"));
}

#[test]
fn detect_source_map_url_css() {
    let src = "body{}\n/*# sourceMappingURL=style.css.map */";
    let url = detect_source_map_url(src, SourceLang::Css);
    assert_eq!(url.as_deref(), Some("style.css.map"));
}

#[test]
fn detect_source_map_url_none() {
    let src = "var x = 1;\nconsole.log(x);";
    assert!(detect_source_map_url(src, SourceLang::JavaScript).is_none());
}

#[test]
fn parse_source_map_basic() {
    let json = r#"{"version":3,"sources":["a.js"],"names":["x"],"mappings":"AAAA"}"#;
    let m = parse_source_map(json).expect("parse");
    assert_eq!(m.version, 3);
    assert_eq!(m.sources, vec!["a.js"]);
    assert_eq!(m.names, vec!["x"]);
    assert_eq!(m.mappings.len(), 1);
    assert_eq!(m.mappings[0].len(), 1);
    let seg = m.mappings[0][0];
    assert_eq!(seg.gen_col, 0);
    assert_eq!(seg.src_idx, Some(0));
    assert_eq!(seg.src_line, Some(0));
    assert_eq!(seg.src_col, Some(0));
}

#[test]
fn parse_source_map_multi_line() {
    // 2 generated lines with mappings
    let json = r#"{"version":3,"sources":["a.js"],"names":[],"mappings":"AAAA;CACC"}"#;
    let m = parse_source_map(json).expect("parse");
    assert_eq!(m.mappings.len(), 2);
    assert_eq!(m.mappings[0][0].gen_col, 0);
    // line 2 segment: CACC -> gen_col=1, src_idx_delta=0, src_line_delta=1, src_col_delta=1
    let seg2 = m.mappings[1][0];
    assert_eq!(seg2.gen_col, 1);
    assert_eq!(seg2.src_line, Some(1));
}

#[test]
fn add_file_assigns_id() {
    let mut s = SourcesState::default();
    let id1 = s.add_file("a.js".into(), "x".into(), SourceLang::JavaScript);
    let id2 = s.add_file("b.js".into(), "y".into(), SourceLang::JavaScript);
    assert_eq!(id1, 0);
    assert_eq!(id2, 1);
    assert_eq!(s.files.len(), 2);
}

#[test]
fn breakpoint_toggle() {
    let mut s = SourcesState::default();
    let id = s.add_file("a.js".into(), "x".into(), SourceLang::JavaScript);
    assert!(!s.has_breakpoint(id, 5));
    let added = s.toggle_breakpoint(id, 5);
    assert!(added);
    assert!(s.has_breakpoint(id, 5));
    let removed = s.toggle_breakpoint(id, 5);
    assert!(!removed);
    assert!(!s.has_breakpoint(id, 5));
}
