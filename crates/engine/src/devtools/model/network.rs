//! Network panel data: per-request entry s podrobnostmi.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkResourceType {
    Document,
    Stylesheet,
    Script,
    Image,
    Font,
    Xhr,
    Fetch,
    Other,
}

impl NetworkResourceType {
    pub fn from_url(url: &str) -> Self {
        let lower = url.to_ascii_lowercase();
        if lower.ends_with(".html") || lower.ends_with(".htm") { NetworkResourceType::Document }
        else if lower.ends_with(".css") { NetworkResourceType::Stylesheet }
        else if lower.ends_with(".js") || lower.ends_with(".mjs") { NetworkResourceType::Script }
        else if lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg")
             || lower.ends_with(".gif") || lower.ends_with(".webp") || lower.ends_with(".svg")
             || lower.ends_with(".avif") { NetworkResourceType::Image }
        else if lower.ends_with(".woff") || lower.ends_with(".woff2")
             || lower.ends_with(".ttf") || lower.ends_with(".otf") { NetworkResourceType::Font }
        else { NetworkResourceType::Other }
    }
}

#[derive(Debug, Clone)]
pub struct NetworkEntry {
    pub url: String,
    pub method: String,
    pub status: u16,
    pub resource_type: NetworkResourceType,
    pub size_bytes: usize,
    pub duration_ms: u32,
    pub started_at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkFilter {
    All,
    Document,
    Stylesheet,
    Script,
    Image,
    Font,
    Xhr,
}

impl NetworkFilter {
    pub fn matches(self, ty: NetworkResourceType) -> bool {
        match self {
            NetworkFilter::All => true,
            NetworkFilter::Document => ty == NetworkResourceType::Document,
            NetworkFilter::Stylesheet => ty == NetworkResourceType::Stylesheet,
            NetworkFilter::Script => ty == NetworkResourceType::Script,
            NetworkFilter::Image => ty == NetworkResourceType::Image,
            NetworkFilter::Font => ty == NetworkResourceType::Font,
            NetworkFilter::Xhr => matches!(ty, NetworkResourceType::Xhr | NetworkResourceType::Fetch),
        }
    }
}
