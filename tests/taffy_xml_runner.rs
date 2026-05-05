//! Integration test runner pro taffy XML fixtury.
//!
//! Fixtury jsou prevzaty z taffy crate (MIT licence).
//! Viz tests/fixtures/LICENSE_TAFFY.md pro attribution.
//!
//! Format fixture:
//! ```xml
//! <test name="..." use-rounding="true">
//!   <viewport width="..." height="..."/>
//!   <input>
//!     <div display="flex" width="400px" height="300px">
//!       <div width="50%" aspect-ratio="3" .../>
//!     </div>
//!   </input>
//!   <expectations>
//!     <node x="0" y="0" width="400" height="300">
//!       <node x="20" y="15" width="200" height="67"/>
//!     </node>
//!   </expectations>
//! </test>
//! ```
//!
//! Strategie:
//! - Parse XML do TestNode tree
//! - Aplikuj na nas LayoutBox
//! - Compute layout
//! - Compare expectations s actual rect
//!
//! Aktualne velka cast bude failovat dokud nebudeme mit vsechny features
//! (aspect-ratio, position absolute, percent, RTL, atd.).
//! Tests jsou marked #[ignore] pro features ktere neumime - postupne unignore
//! jak rozsirovat layout engine.

use std::fs;
use std::path::Path;

#[derive(Debug, Default, Clone)]
struct TestNode {
    attrs: Vec<(String, String)>,
    children: Vec<TestNode>,
}

#[derive(Debug, Default)]
struct ExpectedNode {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    children: Vec<ExpectedNode>,
}

#[derive(Debug, Default)]
struct Fixture {
    name: String,
    input_root: Option<TestNode>,
    expected_root: Option<ExpectedNode>,
}

/// Velmi jednoduchy XML parser - jen pro taffy fixture format.
/// Nepodporuje CDATA, comments, namespaces, attribute escapes.
fn parse_xml(content: &str) -> Option<Fixture> {
    let mut fixture = Fixture::default();
    let mut chars = content.chars().peekable();
    let mut node_stack: Vec<TestNode> = Vec::new();
    let mut exp_stack: Vec<ExpectedNode> = Vec::new();
    let mut in_input = false;
    let mut in_expectations = false;

    while let Some(&c) = chars.peek() {
        if c.is_whitespace() { chars.next(); continue; }
        if c != '<' { chars.next(); continue; }
        chars.next(); // consume '<'
        // Closing tag?
        if chars.peek() == Some(&'/') {
            chars.next();
            let mut tag = String::new();
            while let Some(&cc) = chars.peek() {
                if cc == '>' { chars.next(); break; }
                tag.push(cc);
                chars.next();
            }
            if in_input && (tag == "div" || tag == "node") {
                if let Some(top) = node_stack.pop() {
                    if let Some(parent) = node_stack.last_mut() {
                        parent.children.push(top);
                    } else {
                        fixture.input_root = Some(top);
                    }
                }
            } else if in_expectations && tag == "node" {
                if let Some(top) = exp_stack.pop() {
                    if let Some(parent) = exp_stack.last_mut() {
                        parent.children.push(top);
                    } else {
                        fixture.expected_root = Some(top);
                    }
                }
            } else if tag == "input" { in_input = false; }
            else if tag == "expectations" { in_expectations = false; }
            continue;
        }
        // Opening tag
        let mut tag = String::new();
        while let Some(&cc) = chars.peek() {
            if cc == ' ' || cc == '/' || cc == '>' { break; }
            tag.push(cc);
            chars.next();
        }
        // Parse attrs
        let mut attrs: Vec<(String, String)> = Vec::new();
        let mut self_closing = false;
        while let Some(&cc) = chars.peek() {
            if cc == '>' { chars.next(); break; }
            if cc == '/' { self_closing = true; chars.next(); continue; }
            if cc.is_whitespace() { chars.next(); continue; }
            // Attr name
            let mut attr_name = String::new();
            while let Some(&dd) = chars.peek() {
                if dd == '=' || dd == '>' || dd == ' ' { break; }
                attr_name.push(dd);
                chars.next();
            }
            if chars.peek() == Some(&'=') {
                chars.next();
                if chars.peek() == Some(&'"') { chars.next(); }
                let mut attr_val = String::new();
                while let Some(&dd) = chars.peek() {
                    if dd == '"' { chars.next(); break; }
                    attr_val.push(dd);
                    chars.next();
                }
                attrs.push((attr_name, attr_val));
            } else if !attr_name.is_empty() {
                attrs.push((attr_name, String::new()));
            }
        }
        match tag.as_str() {
            "test" => {
                if let Some(name) = attrs.iter().find(|(k, _)| k == "name") {
                    fixture.name = name.1.clone();
                }
            }
            "input" => { in_input = true; }
            "expectations" => { in_expectations = true; }
            "div" | "node" if in_input => {
                let n = TestNode { attrs, children: Vec::new() };
                if self_closing {
                    if let Some(parent) = node_stack.last_mut() {
                        parent.children.push(n);
                    } else {
                        fixture.input_root = Some(n);
                    }
                } else {
                    node_stack.push(n);
                }
            }
            "node" if in_expectations => {
                let mut e = ExpectedNode::default();
                for (k, v) in &attrs {
                    let f = v.parse::<f32>().unwrap_or(0.0);
                    match k.as_str() {
                        "x" => e.x = f,
                        "y" => e.y = f,
                        "width" => e.width = f,
                        "height" => e.height = f,
                        _ => {}
                    }
                }
                if self_closing {
                    if let Some(parent) = exp_stack.last_mut() {
                        parent.children.push(e);
                    } else {
                        fixture.expected_root = Some(e);
                    }
                } else {
                    exp_stack.push(e);
                }
            }
            _ => {}
        }
    }
    Some(fixture)
}

/// Vrati pocet fixtur v adresari.
fn count_fixtures(dir: &str) -> usize {
    fs::read_dir(dir).map(|d| d.filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "xml"))
        .count()).unwrap_or(0)
}

/// Test ze fixtury jsou validni XML a parsovatelne.
#[test]
fn taffy_flex_fixtures_present() {
    let count = count_fixtures("tests/fixtures/taffy_flex");
    assert!(count > 1000, "Ocekavalo >1000 flex fixtur, naseklo {}", count);
}

#[test]
fn taffy_grid_fixtures_present() {
    let count = count_fixtures("tests/fixtures/taffy_grid");
    assert!(count > 500, "Ocekavalo >500 grid fixtur, naseklo {}", count);
}

#[test]
fn taffy_block_fixtures_present() {
    let count = count_fixtures("tests/fixtures/taffy_block");
    assert!(count > 500, "Ocekavalo >500 block fixtur, naseklo {}", count);
}

/// Test parser na sample fixture.
#[test]
fn parse_xml_sample_flex_fixture() {
    let path = "tests/fixtures/taffy_flex/absolute_aspect_ratio_fill_height__border_box_ltr.xml";
    if !Path::new(path).exists() { return; }
    let content = fs::read_to_string(path).expect("read fixture");
    let fixture = parse_xml(&content).expect("parse fixture");
    assert_eq!(fixture.name, "absolute_aspect_ratio_fill_height__border_box_ltr");
    assert!(fixture.input_root.is_some());
    assert!(fixture.expected_root.is_some());
    let exp = fixture.expected_root.as_ref().unwrap();
    assert_eq!(exp.width, 400.0);
    assert_eq!(exp.height, 300.0);
    assert_eq!(exp.children.len(), 1);
    assert_eq!(exp.children[0].x, 20.0);
}

/// Smoke test - parse vsech flex fixtur, count valid + invalid.
#[test]
fn taffy_flex_all_parsable() {
    let dir = "tests/fixtures/taffy_flex";
    let mut total = 0;
    let mut parsed = 0;
    for entry in fs::read_dir(dir).expect("read dir") {
        let entry = entry.expect("entry");
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "xml") { continue; }
        total += 1;
        if let Ok(content) = fs::read_to_string(&path) {
            if let Some(f) = parse_xml(&content) {
                if f.input_root.is_some() && f.expected_root.is_some() {
                    parsed += 1;
                }
            }
        }
    }
    println!("Flex fixtures: {parsed}/{total} parsable");
    // Aspon 90% by mela byt parsovatelna
    assert!(parsed * 100 / total.max(1) >= 90, "{parsed}/{total} parsable, expected >=90%");
}

#[test]
fn taffy_grid_all_parsable() {
    let dir = "tests/fixtures/taffy_grid";
    let mut total = 0;
    let mut parsed = 0;
    for entry in fs::read_dir(dir).expect("read dir") {
        let entry = entry.expect("entry");
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "xml") { continue; }
        total += 1;
        if let Ok(content) = fs::read_to_string(&path) {
            if let Some(f) = parse_xml(&content) {
                if f.input_root.is_some() && f.expected_root.is_some() {
                    parsed += 1;
                }
            }
        }
    }
    println!("Grid fixtures: {parsed}/{total} parsable");
    assert!(parsed * 100 / total.max(1) >= 90);
}

#[test]
fn taffy_block_all_parsable() {
    let dir = "tests/fixtures/taffy_block";
    let mut total = 0;
    let mut parsed = 0;
    for entry in fs::read_dir(dir).expect("read dir") {
        let entry = entry.expect("entry");
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "xml") { continue; }
        total += 1;
        if let Ok(content) = fs::read_to_string(&path) {
            if let Some(f) = parse_xml(&content) {
                if f.input_root.is_some() && f.expected_root.is_some() {
                    parsed += 1;
                }
            }
        }
    }
    println!("Block fixtures: {parsed}/{total} parsable");
    assert!(parsed * 100 / total.max(1) >= 90);
}

/// Pocet input attribute typu pres vsechny fixtury (analyza coverage).
#[test]
fn taffy_fixtures_attribute_coverage() {
    use std::collections::HashSet;
    let mut all_attrs: HashSet<String> = HashSet::new();
    for dir in &["tests/fixtures/taffy_flex", "tests/fixtures/taffy_grid", "tests/fixtures/taffy_block"] {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten().take(100) {
                let path = entry.path();
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Some(fixture) = parse_xml(&content) {
                        if let Some(root) = fixture.input_root {
                            collect_attrs(&root, &mut all_attrs);
                        }
                    }
                }
            }
        }
    }
    println!("Unique input attributes: {}", all_attrs.len());
    // Ocekavame >20 unique attributes (display, width, height, position, ...)
    assert!(all_attrs.len() > 20);
}

fn collect_attrs(node: &TestNode, set: &mut std::collections::HashSet<String>) {
    for (k, _) in &node.attrs {
        set.insert(k.clone());
    }
    for child in &node.children {
        collect_attrs(child, set);
    }
}
