//! Paint browser shell chrome (tab strip + nav bar + bookmarks bar).
//!
//! Drive uvnitr `run_window_inner` jako nested fn s 5 wrapper layers.
//! Extract do separate module: cleanup + odlehceni render/mod.rs.

use super::super::paint::DisplayCommand;

/// Paint top chrome (tab strip + nav bar + bookmarks bar).
pub(super) fn paint_shell_chrome_with_groups(
    list: &mut Vec<DisplayCommand>,
    win_w: f32,
    chrome_h: f32,
    url: &str,
    tab_titles: Option<&[String]>,
    active: usize,
    favicon_urls: Option<&[Option<String>]>,
    pinned: Option<&[bool]>,
    loading: Option<&[bool]>,
    anim_t: f32,
    groups: Option<&[Option<[u8; 4]>]>,
) {
    // Bookmarks bar paint pod nav bar - dalsi 24px row.
    let bms = crate::devtools::bookmarks::load_bookmarks();
    let tab_h = 28.0;
    let nav_h = chrome_h - tab_h;
    // Chrome bg.
    list.push(DisplayCommand::Rect {
        x: 0.0, y: 0.0, w: win_w, h: chrome_h,
        color: [42, 41, 50, 255], radius: 0.0,
    });
    list.push(DisplayCommand::Rect {
        x: 0.0, y: chrome_h - 1.0, w: win_w, h: 1.0,
        color: [76, 76, 85, 255], radius: 0.0,
    });
    // Tab strip - per-tab chip.
    let titles_owned: Vec<String> = if let Some(t) = tab_titles {
        t.to_vec()
    } else {
        vec![if url.is_empty() { "Nova zalozka".to_string() }
             else {
                 let s = url.split('/').last().unwrap_or(url).to_string();
                 if s.is_empty() { "page".to_string() } else { s }
             }]
    };
    let n = titles_owned.len();
    let pin_count = pinned.map(|p| p.iter().filter(|x| **x).count()).unwrap_or(0);
    let avail_w = win_w - 60.0 - (pin_count as f32) * 40.0;
    let normal_count = n.saturating_sub(pin_count).max(1);
    let tab_w = 200.0_f32.min(avail_w / normal_count as f32);
    let mut tx = 4.0;
    for (i, title) in titles_owned.iter().enumerate() {
        let is_pinned = pinned.map(|p| p.get(i).copied().unwrap_or(false)).unwrap_or(false);
        let chip_w = if is_pinned { 36.0 } else { tab_w };
        let bg = if i == active { [27, 27, 35, 255] } else { [42, 41, 50, 255] };
        list.push(DisplayCommand::Rect {
            x: tx, y: 4.0, w: chip_w, h: tab_h - 4.0,
            color: bg, radius: 4.0,
        });
        // Group color top stripe (3px).
        if let Some(gc) = groups.and_then(|g| g.get(i)).and_then(|c| *c) {
            list.push(DisplayCommand::Rect {
                x: tx + 2.0, y: 4.0, w: chip_w - 4.0, h: 3.0,
                color: gc, radius: 1.5,
            });
        }
        if is_pinned {
            list.push(DisplayCommand::Text {
                x: tx + 4.0, y: 6.0, content: "📌".to_string(),
                color: [254, 191, 84, 255],
                font_size: 11.0, bold: false, font_weight: 400, italic: false,
                font_family: "Inter".into(),
                strikethrough: false, underline: false,
            });
        }
        // Loading: rotujici dot misto favicon.
        let is_loading = loading.and_then(|l| l.get(i)).copied().unwrap_or(false);
        let favicon_present = if is_pinned || is_loading { None } else {
            favicon_urls.and_then(|fs| fs.get(i)).and_then(|f| f.clone())
        };
        let text_x_off = if is_loading && !is_pinned {
            // Spinner: 8 prouzku v kruhu, fade dle uhlu+t.
            let cx = tx + 14.0;
            let cy = 16.0;
            let bars = 8;
            let phase = (anim_t * 8.0) as i32;
            for b in 0..bars {
                let ang = (b as f32) * std::f32::consts::TAU / bars as f32;
                let dx = ang.cos();
                let dy = ang.sin();
                let inner = 3.5;
                let outer = 6.5;
                let x1 = cx + dx * inner;
                let y1 = cy + dy * inner;
                let x2 = cx + dx * outer;
                let y2 = cy + dy * outer;
                let lit = ((b as i32 - phase).rem_euclid(bars as i32)) as f32 / bars as f32;
                let a = (60.0 + lit * 195.0) as u8;
                list.push(DisplayCommand::Rect {
                    x: x1.min(x2) - 0.5, y: y1.min(y2) - 0.5,
                    w: (x2 - x1).abs().max(1.5),
                    h: (y2 - y1).abs().max(1.5),
                    color: [69, 161, 255, a], radius: 0.5,
                });
            }
            28.0
        } else if let Some(furl) = favicon_present {
            list.push(DisplayCommand::Image {
                x: tx + 6.0, y: 8.0, w: 16.0, h: 16.0,
                src: furl,
                radius: 0.0,
            });
            28.0
        } else if is_pinned {
            20.0
        } else {
            8.0
        };
        if !is_pinned {
            let trunc: String = title.chars().take(20).collect();
            list.push(DisplayCommand::Text {
                x: tx + text_x_off, y: 8.0, content: trunc,
                color: [251, 251, 254, 255],
                font_size: 13.0, bold: i == active, font_weight: if i == active {700} else {400}, italic: false,
                font_family: "Inter".into(),
                strikethrough: false, underline: false,
            });
            // Close X (jen non-pinned).
            list.push(DisplayCommand::Rect {
                x: tx + chip_w - 18.0, y: 6.0, w: 16.0, h: 16.0,
                color: [56, 56, 65, 200], radius: 8.0,
            });
            list.push(DisplayCommand::Text {
                x: tx + chip_w - 14.0, y: 8.0, content: "x".to_string(),
                color: [220, 220, 230, 255],
                font_size: 12.0, bold: true, font_weight: 700, italic: false,
                font_family: "Inter".into(),
                strikethrough: false, underline: false,
            });
        }
        tx += chip_w + 2.0;
    }
    // + new tab button.
    list.push(DisplayCommand::Text {
        x: tx + 4.0, y: 6.0, content: "+".to_string(),
        color: [191, 191, 201, 255],
        font_size: 18.0, bold: true, font_weight: 700, italic: false,
        font_family: "Inter".into(),
        strikethrough: false, underline: false,
    });

    // Nav bar (back/forward/reload + URL).
    let ny = tab_h;
    list.push(DisplayCommand::Text {
        x: 12.0, y: ny + 8.0, content: "<".to_string(),
        color: [251, 251, 254, 255],
        font_size: 16.0, bold: true, font_weight: 700, italic: false,
        font_family: "CamingoMono".into(),
        strikethrough: false, underline: false,
    });
    list.push(DisplayCommand::Text {
        x: 32.0, y: ny + 8.0, content: ">".to_string(),
        color: [251, 251, 254, 255],
        font_size: 16.0, bold: true, font_weight: 700, italic: false,
        font_family: "CamingoMono".into(),
        strikethrough: false, underline: false,
    });
    list.push(DisplayCommand::Text {
        x: 52.0, y: ny + 8.0, content: "↻".to_string(),
        color: [251, 251, 254, 255],
        font_size: 14.0, bold: false, font_weight: 400, italic: false,
        font_family: "CamingoMono".into(),
        strikethrough: false, underline: false,
    });
    // Bookmark star indicator.
    let bookmarked = !url.is_empty()
        && crate::devtools::bookmarks::load_bookmarks().iter().any(|b| b.url == url);
    let star_color = if bookmarked { [254, 191, 84, 255] } else { [109, 109, 124, 200] };
    list.push(DisplayCommand::Text {
        x: win_w - 76.0, y: ny + 8.0, content: "★".to_string(),
        color: star_color,
        font_size: 16.0, bold: false, font_weight: 400, italic: false,
        font_family: "Inter".into(),
        strikethrough: false, underline: false,
    });
    // Devtools toggle button.
    let dt_x = win_w - 36.0;
    list.push(DisplayCommand::Rect {
        x: dt_x, y: ny + 4.0, w: 28.0, h: nav_h - 8.0,
        color: [42, 41, 50, 255], radius: 4.0,
    });
    list.push(DisplayCommand::Text {
        x: dt_x + 6.0, y: ny + 8.0, content: "F12".to_string(),
        color: [191, 191, 201, 255],
        font_size: 11.0, bold: true, font_weight: 700, italic: false,
        font_family: "Inter".into(),
        strikethrough: false, underline: false,
    });
    // URL bar.
    let url_x = 78.0;
    let url_w = win_w - url_x - 48.0;
    list.push(DisplayCommand::Rect {
        x: url_x, y: ny + 4.0, w: url_w, h: nav_h - 8.0,
        color: [27, 27, 35, 255], radius: 4.0,
    });
    list.push(DisplayCommand::Text {
        x: url_x + 8.0, y: ny + 9.0, content: url.to_string(),
        color: [251, 251, 254, 255],
        font_size: 12.0, bold: false, font_weight: 400, italic: false,
        font_family: "CamingoMono".into(),
        strikethrough: false, underline: false,
    });
    // Bookmarks bar.
    if !bms.is_empty() && chrome_h >= 88.0 {
        let bm_y = chrome_h - 24.0;
        list.push(DisplayCommand::Rect {
            x: 0.0, y: bm_y, w: win_w, h: 24.0,
            color: [35, 34, 43, 255], radius: 0.0,
        });
        list.push(DisplayCommand::Rect {
            x: 0.0, y: bm_y + 23.0, w: win_w, h: 1.0,
            color: [76, 76, 85, 255], radius: 0.0,
        });
        let mut bx_x = 8.0;
        for bm in bms.iter().take(15) {
            let title_trunc: String = bm.title.chars().take(18).collect();
            let bw = (title_trunc.len() as f32) * 7.0 + 16.0;
            if bx_x + bw > win_w - 8.0 { break; }
            list.push(DisplayCommand::Rect {
                x: bx_x, y: bm_y + 2.0, w: bw, h: 20.0,
                color: [42, 41, 50, 255], radius: 4.0,
            });
            list.push(DisplayCommand::Text {
                x: bx_x + 6.0, y: bm_y + 5.0,
                content: title_trunc, color: [191, 191, 201, 255],
                font_size: 11.0, bold: false, font_weight: 400, italic: false,
                font_family: "Inter".into(),
                strikethrough: false, underline: false,
            });
            bx_x += bw + 4.0;
        }
    }
}
