//! Reader mode - extract main article + strip chrome.
//!
//! Heuristic-based content extraction (similar to Mozilla Readability).

#[derive(Debug, Clone, Default)]
pub struct ReaderArticle {
    pub title: String,
    pub byline: String,
    pub site_name: String,
    pub content_html: String,
    pub text_content: String,
    pub language: String,
    pub published_time: Option<String>,
    pub estimated_reading_time_min: u32,
    pub estimated_word_count: u32,
}

impl ReaderArticle {
    pub fn estimate_reading_time(word_count: u32) -> u32 {
        const WPM: u32 = 250;
        (word_count / WPM).max(1)
    }

    pub fn count_words(text: &str) -> u32 {
        text.split_ascii_whitespace().count() as u32
    }
}

/// Tag scoring heuristic - higher = more likely to be article content.
pub fn tag_base_score(tag: &str) -> i32 {
    match tag.to_ascii_lowercase().as_str() {
        "article" | "main" => 50,
        "section" => 25,
        "div" | "p" => 5,
        "pre" | "blockquote" => 3,
        "td" => 3,
        "address" | "form" | "footer" | "header" | "nav" | "aside" => -25,
        "ul" | "ol" | "li" => -3,
        "h1" | "h2" | "h3" => -5,
        _ => 0,
    }
}

/// Class/ID heuristic per Readability.js.
pub fn class_id_score(class_or_id: &str) -> i32 {
    let lower = class_or_id.to_ascii_lowercase();
    let positive = ["article", "body", "content", "entry", "main", "post", "story", "text"];
    let negative = ["aside", "footer", "header", "nav", "sidebar", "comment", "share", "promo", "ad-",
                    "advertisement", "pager", "pagination", "menu", "modal", "popup"];
    let mut score = 0;
    for p in &positive { if lower.contains(p) { score += 25; } }
    for n in &negative { if lower.contains(n) { score -= 25; } }
    score
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_time_minimum_1() {
        assert_eq!(ReaderArticle::estimate_reading_time(0), 1);
        assert_eq!(ReaderArticle::estimate_reading_time(500), 2);
    }

    #[test]
    fn count_words_basic() {
        assert_eq!(ReaderArticle::count_words("hello world foo"), 3);
    }

    #[test]
    fn tag_score_article() {
        assert!(tag_base_score("article") > tag_base_score("aside"));
        assert!(tag_base_score("p") > 0);
    }

    #[test]
    fn class_score_promotes_content() {
        assert!(class_id_score("article-body") > 0);
        assert!(class_id_score("sidebar") < 0);
    }

    #[test]
    fn class_score_neutral_unknown() {
        assert_eq!(class_id_score("custom-tag"), 0);
    }
}
