/// L5 step 4 Phase 3 Step D: Visual regression test infrastructure.
///
/// Dva snapshot types:
/// 1. LayoutBox snapshot - serialize LayoutBox tree do textu, golden compare.
///    Catches layout regrese: rect, display, position, transform applied.
/// 2. DisplayList snapshot - serialize paint commands. Catches paint regrese:
///    gradient, polygon, shadow, image emit + colors.
///
/// Golden files v src/browser/tests/golden/. Update pres env var:
///   UPDATE_GOLDEN=1 cargo test visual_snapshot
///
/// Bez golden file (prvni run) - test fail s "missing golden". UPDATE_GOLDEN=1
/// vytvori golden + test pass.

use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout, paint};
use crate::browser::paint::DisplayCommand;
use crate::browser::layout::LayoutBox;

/// Serialize LayoutBox tree do textu pro snapshot compare.
/// Format: indented tree, per-line "tag rect:(x,y,w,h) display=X position=Y bg=#RGB".
fn serialize_layout(bx: &LayoutBox, depth: usize) -> String {
    let mut out = String::new();
    let indent = "  ".repeat(depth);
    let tag = bx.tag.as_deref().unwrap_or("?");
    out.push_str(&format!(
        "{indent}{tag} rect=({:.1},{:.1},{:.1}x{:.1}) display={:?} pos={:?}",
        bx.rect.x, bx.rect.y, bx.rect.width, bx.rect.height,
        bx.display, bx.position,
    ));
    if let Some(bg) = bx.bg_color {
        out.push_str(&format!(" bg=#{:02x}{:02x}{:02x}{:02x}", bg[0], bg[1], bg[2], bg[3]));
    }
    if let Some(tc) = bx.text_color {
        out.push_str(&format!(" color=#{:02x}{:02x}{:02x}", tc[0], tc[1], tc[2]));
    }
    if bx.opacity < 1.0 {
        out.push_str(&format!(" opacity={:.2}", bx.opacity));
    }
    if let Some(t) = bx.transform {
        out.push_str(&format!(" transform={:?}", t));
    }
    if let Some(text) = &bx.text {
        let t = text.trim();
        if !t.is_empty() {
            let preview: String = t.chars().take(40).collect();
            out.push_str(&format!(" text={:?}", preview));
        }
    }
    out.push('\n');
    for ch in &bx.children {
        out.push_str(&serialize_layout(ch, depth + 1));
    }
    out
}

/// Serialize DisplayList do textu. Per command 1 line.
fn serialize_dl(cmds: &[DisplayCommand]) -> String {
    let mut out = String::new();
    for cmd in cmds {
        match cmd {
            DisplayCommand::Rect { x, y, w, h, color, .. } => {
                out.push_str(&format!("Rect ({:.1},{:.1},{:.1}x{:.1}) #{:02x}{:02x}{:02x}{:02x}\n",
                    x, y, w, h, color[0], color[1], color[2], color[3]));
            }
            DisplayCommand::Text { x, y, content, color, .. } => {
                let preview: String = content.chars().take(30).collect();
                out.push_str(&format!("Text ({:.1},{:.1}) {:?} #{:02x}{:02x}{:02x}\n",
                    x, y, preview, color[0], color[1], color[2]));
            }
            DisplayCommand::Image { x, y, w, h, src, .. } => {
                out.push_str(&format!("Image ({:.1},{:.1},{:.1}x{:.1}) {}\n",
                    x, y, w, h, src));
            }
            DisplayCommand::Gradient { x, y, w, h, .. } => {
                out.push_str(&format!("Gradient ({:.1},{:.1},{:.1}x{:.1})\n", x, y, w, h));
            }
            DisplayCommand::Shadow { x, y, w, h, .. } => {
                out.push_str(&format!("Shadow ({:.1},{:.1},{:.1}x{:.1})\n", x, y, w, h));
            }
            DisplayCommand::Border { x, y, w, h, .. } => {
                out.push_str(&format!("Border ({:.1},{:.1},{:.1}x{:.1})\n", x, y, w, h));
            }
            DisplayCommand::FilterBegin { .. } => out.push_str("FilterBegin\n"),
            DisplayCommand::FilterEnd => out.push_str("FilterEnd\n"),
            other => out.push_str(&format!("{:?}\n", other)),
        }
    }
    out
}

/// Snapshot compare helper. Pri UPDATE_GOLDEN=1 zapis novou golden,
/// jinak load existing + assert equal.
fn assert_golden(name: &str, actual: &str) {
    let golden_path = format!("src/browser/tests/golden/{}.snap", name);
    let update = std::env::var("UPDATE_GOLDEN").is_ok();
    if update {
        // Vytvor adresar pokud chybi.
        let _ = std::fs::create_dir_all("src/browser/tests/golden");
        std::fs::write(&golden_path, actual).expect("Cannot write golden");
        eprintln!("UPDATE_GOLDEN: wrote {}", golden_path);
        return;
    }
    let expected = match std::fs::read_to_string(&golden_path) {
        Ok(s) => s,
        Err(_) => {
            eprintln!("Missing golden: {}. Run UPDATE_GOLDEN=1 cargo test {} to create.", golden_path, name);
            panic!("missing golden snapshot");
        }
    };
    if actual != expected {
        // Diff preview - first 10 different lines.
        let act_lines: Vec<&str> = actual.lines().collect();
        let exp_lines: Vec<&str> = expected.lines().collect();
        let mut diff = String::new();
        for (i, (a, e)) in act_lines.iter().zip(exp_lines.iter()).enumerate() {
            if a != e {
                diff.push_str(&format!("L{}: -{}\n     +{}\n", i, e, a));
                if diff.lines().count() > 20 { break; }
            }
        }
        if act_lines.len() != exp_lines.len() {
            diff.push_str(&format!("line count: golden={} actual={}\n", exp_lines.len(), act_lines.len()));
        }
        panic!("Snapshot mismatch for {}\n{}\nFull actual:\n{}\n---\nGolden:\n{}",
            name, diff, actual, expected);
    }
}

/// Build LayoutBox + DisplayList z HTML + CSS pro snapshot.
fn build_snapshot(html: &str, css: &str) -> (LayoutBox, Vec<DisplayCommand>) {
    let doc = parse_html(html, "");
    let css_sheet = parse_stylesheet(css);
    let map = cascade::cascade(&doc.root, &[css_sheet]);
    let layout = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let dl = paint::build_display_list(&layout);
    (layout, dl)
}

// ─── Basic snapshot tests ──────────────────────────────────────────────────

#[test]
fn snapshot_simple_box() {
    let (lr, dl) = build_snapshot(
        r#"<html><body><div id="x" style="width:100px;height:50px;background:red;">Hello</div></body></html>"#,
        ""
    );
    let layout_snap = serialize_layout(&lr, 0);
    let dl_snap = serialize_dl(&dl);
    assert_golden("simple_box_layout", &layout_snap);
    assert_golden("simple_box_dl", &dl_snap);
}

#[test]
fn snapshot_flex_row() {
    let (lr, _dl) = build_snapshot(
        r#"<html><body><div id="row" style="display:flex;width:300px;">
            <div style="flex:1;background:red;">A</div>
            <div style="flex:1;background:green;">B</div>
            <div style="flex:1;background:blue;">C</div>
        </div></body></html>"#,
        ""
    );
    let layout_snap = serialize_layout(&lr, 0);
    assert_golden("flex_row_layout", &layout_snap);
}

#[test]
fn snapshot_text_transform_uppercase() {
    let (lr, _dl) = build_snapshot(
        r#"<html><body><p style="text-transform:uppercase;">hello world</p></body></html>"#,
        ""
    );
    let layout_snap = serialize_layout(&lr, 0);
    assert_golden("text_transform_layout", &layout_snap);
}

#[test]
fn snapshot_position_absolute() {
    let (lr, _dl) = build_snapshot(
        r#"<html><body><div style="position:relative;width:200px;height:100px;">
            <div style="position:absolute;top:10px;left:20px;width:50px;height:30px;background:blue;"></div>
        </div></body></html>"#,
        ""
    );
    let layout_snap = serialize_layout(&lr, 0);
    assert_golden("position_absolute_layout", &layout_snap);
}

#[test]
fn snapshot_gradient_bg() {
    let (_lr, dl) = build_snapshot(
        r#"<html><body><div style="width:100px;height:50px;background:linear-gradient(red,blue);"></div></body></html>"#,
        ""
    );
    let dl_snap = serialize_dl(&dl);
    assert_golden("gradient_bg_dl", &dl_snap);
}
