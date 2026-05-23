use super::*;

#[test]
fn maybe_decode_passes_through_non_woff() {
    let data = vec![0u8; 100];
    let out = maybe_decode_woff(&data);
    assert_eq!(out, data);
}

#[test]
fn decode_rejects_bad_signature() {
    let data = vec![b'X'; 50];
    assert!(decode_woff1(&data).is_err());
}

#[test]
fn decode_rejects_too_short() {
    let data = b"wOFF";
    assert!(decode_woff1(data).is_err());
}

/// Vyrobi minimalisticky valid WOFF s jednou tabulkou (raw, no compression).
/// Header je presne 44 bytes (8B sig+flavor, 4B length, 2B numTables, 2B reserved,
/// 4B totalSfntSize, 2B+2B major/minor, 4*5=20B meta+priv = 44).
fn make_minimal_woff() -> Vec<u8> {
    let table_data: Vec<u8> = vec![0xDE, 0xAD, 0xBE, 0xEF];
    let table_tag: u32 = 0x6e616d65; // 'name'
    let mut out = Vec::new();
    out.extend_from_slice(b"wOFF"); // 4
    out.extend_from_slice(&0x00010000u32.to_be_bytes()); // flavor TTF (4)
    let total_len = 44u32 + 20 + table_data.len() as u32;
    out.extend_from_slice(&total_len.to_be_bytes()); // length (4) -> 16
    out.extend_from_slice(&1u16.to_be_bytes()); // numTables (2) -> 18
    out.extend_from_slice(&0u16.to_be_bytes()); // reserved (2) -> 20
    out.extend_from_slice(&(12 + 16 + table_data.len() as u32).to_be_bytes()); // totalSfntSize (4) -> 24
    out.extend_from_slice(&0u16.to_be_bytes()); // majorVersion (2) -> 26
    out.extend_from_slice(&0u16.to_be_bytes()); // minorVersion (2) -> 28
    out.extend_from_slice(&0u32.to_be_bytes()); // metaOffset (4) -> 32
    out.extend_from_slice(&0u32.to_be_bytes()); // metaLength (4) -> 36
    out.extend_from_slice(&0u32.to_be_bytes()); // metaOrigLength (4) -> 40
    out.extend_from_slice(&0u32.to_be_bytes()); // privOffset (4) -> 44
    out.extend_from_slice(&0u32.to_be_bytes()); // privLength (4) -> 48 - WAIT to je 48
    // Pozn: spec je 44, takze metaOrigLength + privOffset + privLength musi byt JEN 12B nebo
    // jsme uplne mimo. Spravna struktura:
    // metaOffset(4) + metaLength(4) + metaOrigLength(4) + privOffset(4) + privLength(4) = 20 BYTES
    // Takze 24 + 20 = 44. To jsme dali. Ale ve skutku zkontroluj Rev.

    // Resign: zruseno. Proste vrat 44B header s 20B meta+priv.
    out.truncate(0);
    out.extend_from_slice(b"wOFF");
    out.extend_from_slice(&0x00010000u32.to_be_bytes());
    out.extend_from_slice(&total_len.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&(12 + 16 + table_data.len() as u32).to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes()); // major
    out.extend_from_slice(&0u16.to_be_bytes()); // minor
    // 5x u32 = 20B
    out.extend_from_slice(&[0u8; 20]);
    // Total tak daleko: 4+4+4+2+2+4+2+2+20 = 44 ✓

    // Table directory entry (20B)
    out.extend_from_slice(&table_tag.to_be_bytes());
    out.extend_from_slice(&64u32.to_be_bytes()); // offset (44 + 20 = 64)
    out.extend_from_slice(&(table_data.len() as u32).to_be_bytes()); // compLength
    out.extend_from_slice(&(table_data.len() as u32).to_be_bytes()); // origLength
    out.extend_from_slice(&0u32.to_be_bytes()); // checksum
    // Raw table data at offset 64
    out.extend_from_slice(&table_data);
    out
}

#[test]
fn read_255_uint16_short_form() {
    let data = [42u8, 0];
    let (v, p) = read_255_uint16(&data, 0).unwrap();
    assert_eq!(v, 42);
    assert_eq!(p, 1);
}

#[test]
fn read_255_uint16_two_byte_high_low() {
    // 253 marker -> next 2 bytes BE = 1000.
    let data = [253u8, 0x03, 0xE8];
    let (v, p) = read_255_uint16(&data, 0).unwrap();
    assert_eq!(v, 1000);
    assert_eq!(p, 3);
}

#[test]
fn read_255_uint16_offset_506() {
    let data = [254u8, 100];
    let (v, _) = read_255_uint16(&data, 0).unwrap();
    assert_eq!(v, 606); // 100 + 506
}

#[test]
fn read_255_uint16_offset_253() {
    let data = [255u8, 100];
    let (v, _) = read_255_uint16(&data, 0).unwrap();
    assert_eq!(v, 353); // 100 + 253
}

#[test]
fn decode_woff2_rejects_bad_signature() {
    let data = vec![b'X'; 100];
    assert!(decode_woff2(&data).is_err());
}

#[test]
fn decode_minimal_woff_to_sfnt() {
    let woff = make_minimal_woff();
    let sfnt = decode_woff1(&woff).expect("decode");
    // sfnt musi zacit s 0x00010000 (TTF flavor).
    assert_eq!(&sfnt[0..4], &[0x00, 0x01, 0x00, 0x00]);
    // numTables = 1.
    assert_eq!(u16::from_be_bytes([sfnt[4], sfnt[5]]), 1);
    // Table tag 'name' v directory.
    assert_eq!(&sfnt[12..16], b"name");
}

/// Test ze CSS animations korektne interpoluji unitless (opacity) i px
/// hodnoty (left/width). Driv "0.5" interpolovano jako "0.5px" - opacity
/// parser fail.
#[test]
fn animations_unit_preservation() {
    let html = "<div class='a'></div>".to_string();
    let css = r#"
        .a { animation: t 2s linear infinite; }
        @keyframes t {
            0% { left: 0px; opacity: 1; }
            100% { left: 400px; opacity: 0; }
        }
    "#.to_string();
    let doc = crate::browser::html_parser::parse_html(&html, "");
    let sheets = vec![crate::browser::css_parser::parse_stylesheet(&css)];
    let mut style_map = crate::browser::cascade::cascade(&doc.root, &sheets);
    let _ = crate::browser::cascade::apply_animations(&mut style_map, &sheets, 1.0);
    // Najdi prvni element s class=a.
    let mut found = false;
    doc.root.walk(&mut |n: &std::rc::Rc<crate::browser::dom::Node>| {
        if n.attr("class").as_deref() == Some("a") {
            found = true;
            let id = std::rc::Rc::as_ptr(n) as usize;
            let styles = style_map.get(&id).unwrap();
            assert_eq!(styles.get("left").map(|s| s.as_str()), Some("200px"),
                "left ma byt '200px' got {:?}", styles.get("left"));
            assert_eq!(styles.get("opacity").map(|s| s.as_str()), Some("0.5"),
                "opacity ma byt '0.5' (bez px) got {:?}", styles.get("opacity"));
        }
    });
    assert!(found, "<.a> element not found");
}

/// Test: table layout - cells flex-grow=1 default + tr in block dispatch.
#[test]
fn table_layout_distributes_cells() {
    let html = r#"
        <table>
            <tr><th>A</th><th>B</th><th>C</th></tr>
        </table>
    "#.to_string();
    let css = String::new();
    let doc = crate::browser::html_parser::parse_html(&html, "");
    let sheets = vec![crate::browser::css_parser::parse_stylesheet(&css)];
    let style_map = crate::browser::cascade::cascade(&doc.root, &sheets);
    let pseudo_map = crate::browser::cascade::cascade_pseudo(&doc.root, &sheets);
    let layout = crate::browser::layout::layout_tree_with_pseudo(
        &doc.root, &style_map, &pseudo_map, 900.0, 600.0);

    // Najdi <tr> a over ze ma 3 cells horizontalne distribuovane.
    fn find_tr(bx: &crate::browser::layout::LayoutBox)
        -> Option<&crate::browser::layout::LayoutBox>
    {
        if bx.tag.as_deref() == Some("tr") { return Some(bx); }
        for ch in &bx.children {
            if let Some(t) = find_tr(ch) { return Some(t); }
        }
        None
    }
    let tr = find_tr(&layout).expect("tr nenalezen");
    assert!(tr.rect.width > 100.0, "tr je prilis uzky: {}", tr.rect.width);
    assert_eq!(tr.children.len(), 3, "tr ma mit 3 buncky");
    // Cells horizontalne (rect.x ruzne).
    let xs: Vec<f32> = tr.children.iter().map(|c| c.rect.x).collect();
    assert!(xs[0] < xs[1] && xs[1] < xs[2],
        "cells nejsou horizontalne: {:?}", xs);
    // Kazdy cell aspon ~rovnomerny dil.
    for c in &tr.children {
        assert!(c.rect.width > tr.rect.width / 6.0,
            "cell prilis uzky: {} (tr={})", c.rect.width, tr.rect.width);
    }
}

/// Helper: validuje sfnt vystup struktury.
fn validate_sfnt(sfnt: &[u8], label: &str) {
    // SFNT magic: 0x00010000 (TTF) nebo 0x4F54544F (OTF).
    assert!(matches!(&sfnt[0..4], [0x00, 0x01, 0x00, 0x00] | [0x4F, 0x54, 0x54, 0x4F]),
        "{}: spatny sfnt magic: {:?}", label, &sfnt[0..4]);
    let num_tables = u16::from_be_bytes([sfnt[4], sfnt[5]]) as usize;
    assert!(num_tables > 0 && num_tables < 64,
        "{}: num_tables = {}", label, num_tables);

    // Najdi glyf + loca v directory + over alignment.
    let mut has_glyf = false;
    let mut has_loca = false;
    for i in 0..num_tables {
        let off = 12 + i * 16;
        let tag = &sfnt[off..off+4];
        let table_off = u32::from_be_bytes([sfnt[off+8], sfnt[off+9], sfnt[off+10], sfnt[off+11]]) as usize;
        let table_len = u32::from_be_bytes([sfnt[off+12], sfnt[off+13], sfnt[off+14], sfnt[off+15]]) as usize;
        assert!(table_off + table_len <= sfnt.len(),
            "{}: tag {:?} out of bounds (off={}, len={}, sfnt={})",
            label, tag, table_off, table_len, sfnt.len());
        if tag == b"glyf" { has_glyf = true; }
        if tag == b"loca" { has_loca = true; }
    }
    // Glyf+loca only required pri ttf flavor (ne pri CFF/OTF).
    let is_ttf = sfnt[0..4] == [0x00, 0x01, 0x00, 0x00];
    if is_ttf {
        assert!(has_glyf, "{}: TTF sfnt nema glyf", label);
        assert!(has_loca, "{}: TTF sfnt nema loca", label);
    }

    // swash load test (validuje pres nezavislou implementaci).
    assert!(swash::FontRef::from_index(&sfnt, 0).is_some(),
        "{}: swash nemohl nacist sfnt", label);
}

/// Iteruje pres vsechny .woff2 v static/fonts/ a otestuje round-trip.
/// Pokryva latin, latin-ext, cyrillic, cyrillic-ext, greek, greek-ext,
/// vietnamese, math, symbols (Roboto subsets) + Noto Sans skripty:
/// arabic, hebrew, devanagari, thai, bengali, japanese, korean, chinese,
/// tamil, khmer, georgian, armenian, ethiopic.
#[test]
fn decode_all_real_woff2_fonts() {
    let dir = std::path::Path::new("static/fonts");
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => { eprintln!("[skip] static/fonts neexistuje"); return; }
    };
    let mut tested = 0;
    let mut failures = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("woff2") { continue; }
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("?").to_string();
        let data = match std::fs::read(&path) {
            Ok(d) => d, Err(_) => continue,
        };
        // Skip non-WOFF2 (404 HTML pages).
        if data.len() < 4 || &data[0..4] != b"wOF2" {
            eprintln!("[skip] {}: not WOFF2", name);
            continue;
        }
        tested += 1;
        match decode_woff2(&data) {
            Ok(sfnt) => {
                if let Err(e) = std::panic::catch_unwind(|| validate_sfnt(&sfnt, &name)) {
                    failures.push(format!("{}: validate panic {:?}", name, e));
                } else {
                    eprintln!("[ok] {} ({} B -> {} B sfnt)", name, data.len(), sfnt.len());
                }
            }
            Err(e) => failures.push(format!("{}: decode_woff2 err {:?}", name, e)),
        }
    }
    assert!(tested >= 3, "ocekavany aspon 3 WOFF2 fonty, nalezeno {}", tested);
    if !failures.is_empty() {
        panic!("WOFF2 round-trip selhal pro {} z {} fontu:\n{}",
            failures.len(), tested, failures.join("\n"));
    }
    eprintln!("[woff2] {}/{} fontu OK", tested, tested);
}
