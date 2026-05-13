//! `Engine` - sdilene resources (GPU device/queue, font/image atlas, settings).
//!
//! Z hostujici aplikace si embedder vytvori 1 instanci `Engine` a pak nad ni
//! spawnuje N `WebView` instanci (1 per tab). GPU resources jsou shared pres
//! `Arc` - cilem je nemit duplicate font atlas / image cache per tab.
//!
//! # Standalone vs embed
//!
//! Pro testy / engine demo poskytujeme `Engine::run_standalone(...)` - tahle
//! funkce si sama vytvori winit Window + wgpu Surface, spawne 1 fullscreen
//! WebView a beha render loop. Pro hostujici aplikace (shell, custom UI):
//! `Engine::new(device, queue)` kde host predava sve sdilene GPU resources.

use std::path::PathBuf;
use std::sync::Arc;

/// Sdilene engine resources. WebView drzi `Arc<Engine>` a sahnae sem pro
/// fontove + image cache + GPU access.
///
/// V Phase 2 je struktura prevazne placeholder - skutecne presunuti
/// device/queue/atlasu z `browser::render::Renderer` probehne v Phase 5.
pub struct Engine {
    /// GPU device sdileny pres vsechny WebView. `Arc` umoznuje hostujici
    /// aplikaci pouzit stejne device pro vlastni rendering (chrome UI).
    ///
    /// `None` = headless mode (Phase 3 testy, server-side parsing/layout
    /// bez GPU). WebView::render vyzaduje device - pri None vrati None.
    pub(crate) device: Option<Arc<wgpu::Device>>,
    /// GPU queue, taky shared. `None` v headless mode.
    pub(crate) queue: Option<Arc<wgpu::Queue>>,
    /// Glyph atlas pro vsechny webviews. Phase 5 sem migruje GlyphAtlas struct.
    pub(crate) _glyph_atlas: (),
    /// Image RGBA atlas, shared cache.
    pub(crate) _image_atlas: (),
    /// Font registry (@font-face nactene fonts + system fonts).
    pub(crate) _font_registry: (),
    /// Globalni nastaveni - default font family, devtools defaults, ...
    pub(crate) settings: EngineSettings,
}

/// Konfigurace enginu - default fonty, viewport, UA string.
#[derive(Debug, Clone)]
pub struct EngineSettings {
    /// Default font family pouzity kdyz CSS nestanovuje `font-family`.
    pub default_font_family: String,
    /// Default font size v CSS px.
    pub default_font_size: f32,
    /// User-Agent string pro fetch().
    pub user_agent: String,
    /// Maximalni pocet WebView instanci aktivnich naraz (warning v hostujici
    /// aplikaci kdyz pretece - shared atlas neni neomezeny).
    pub max_webviews: usize,
}

impl Default for EngineSettings {
    fn default() -> Self {
        Self {
            default_font_family: "Times New Roman".to_string(),
            default_font_size: 16.0,
            user_agent: format!("RustWebEngine/{}", env!("CARGO_PKG_VERSION")),
            max_webviews: 64,
        }
    }
}

impl Engine {
    /// Vytvori novy Engine se sdilenymi GPU resources. Hostujici aplikace si
    /// predtim sama vytvorila wgpu::Instance + Adapter + Device.
    ///
    /// V Phase 2 je tohle pouze konstruktor - real init font/image atlas
    /// se presune v Phase 5.
    pub fn new(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) -> Self {
        Self {
            device: Some(device),
            queue: Some(queue),
            _glyph_atlas: (),
            _image_atlas: (),
            _font_registry: (),
            settings: EngineSettings::default(),
        }
    }

    /// Headless Engine bez GPU resources. Pouziti: unit testy WebView pro
    /// page state setup (load_html / interpreter / layout) bez wgpu init.
    /// `render()` na WebView vrati None pokud device chybi.
    pub fn new_headless() -> Self {
        Self {
            device: None,
            queue: None,
            _glyph_atlas: (),
            _image_atlas: (),
            _font_registry: (),
            settings: EngineSettings::default(),
        }
    }

    /// Pristup k engine settings.
    pub fn settings(&self) -> &EngineSettings { &self.settings }

    /// Pristup k device pro custom rendering (shell chrome paint, ...).
    /// `None` v headless mode.
    pub fn device(&self) -> Option<&wgpu::Device> { self.device.as_deref() }

    /// Pristup k queue pro submit hostujici aplikace. `None` v headless mode.
    pub fn queue(&self) -> Option<&wgpu::Queue> { self.queue.as_deref() }

    /// `true` pokud engine ma plne GPU resources (ne headless).
    pub fn has_gpu(&self) -> bool { self.device.is_some() && self.queue.is_some() }

    /// Standalone run - engine si sam udela window + surface a spousti 1
    /// WebView fullscreen. Pro testovani enginu + `rwe-engine` bin (debug,
    /// devtools dispatch).
    ///
    /// Phase 2: vola prozatim staraz `browser::render::run_window_with_options`
    /// pro backwards compatibility. Phase 5 prejde na novy compositor.
    pub fn run_standalone(
        html: String,
        css: String,
        current_html_path: Option<PathBuf>,
        auto_devtools: bool,
        base_url: Option<String>,
    ) -> Result<(), String> {
        crate::browser::render::run_window_with_options(
            html, css, current_html_path, auto_devtools, base_url,
        )
    }
}
