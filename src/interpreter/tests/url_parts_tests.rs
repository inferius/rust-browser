/// Testy pro parse_url_parts + url_encode (interpreter URL helpers).

use crate::interpreter::{parse_url_parts, url_encode};

#[test]
fn url_parts_full() {
    let p = parse_url_parts("https://example.com:8080/path/to?q=1&s=ab#hash");
    assert_eq!(p.protocol, "https:");
    assert_eq!(p.host, "example.com:8080");
    assert_eq!(p.hostname, "example.com");
    assert_eq!(p.port, "8080");
    assert_eq!(p.pathname, "/path/to");
    assert_eq!(p.search, "?q=1&s=ab");
    assert_eq!(p.hash, "#hash");
    assert_eq!(p.origin, "https://example.com:8080");
}

#[test]
fn url_parts_no_port() {
    let p = parse_url_parts("https://example.com/path");
    assert_eq!(p.host, "example.com");
    assert_eq!(p.hostname, "example.com");
    assert_eq!(p.port, "");
    assert_eq!(p.pathname, "/path");
}

#[test]
fn url_parts_no_path() {
    let p = parse_url_parts("https://example.com");
    assert_eq!(p.pathname, "/");
    assert_eq!(p.search, "");
    assert_eq!(p.hash, "");
}

#[test]
fn url_parts_query_only() {
    let p = parse_url_parts("https://example.com/?key=val");
    assert_eq!(p.search, "?key=val");
    assert_eq!(p.hash, "");
    assert_eq!(p.pathname, "/");
}

#[test]
fn url_parts_hash_only() {
    let p = parse_url_parts("https://example.com/#section");
    assert_eq!(p.hash, "#section");
    assert_eq!(p.search, "");
}

#[test]
fn url_parts_query_and_hash() {
    let p = parse_url_parts("https://example.com/path?q=1#anchor");
    assert_eq!(p.pathname, "/path");
    assert_eq!(p.search, "?q=1");
    assert_eq!(p.hash, "#anchor");
}

#[test]
fn url_parts_no_protocol_defaults_to_https() {
    let p = parse_url_parts("example.com/foo");
    assert_eq!(p.protocol, "https:");
    assert_eq!(p.host, "example.com");
    assert_eq!(p.pathname, "/foo");
}

#[test]
fn url_parts_http_protocol() {
    let p = parse_url_parts("http://localhost:3000/api");
    assert_eq!(p.protocol, "http:");
    assert_eq!(p.hostname, "localhost");
    assert_eq!(p.port, "3000");
}

#[test]
fn url_parts_origin_includes_port() {
    let p = parse_url_parts("https://localhost:9000/x");
    assert_eq!(p.origin, "https://localhost:9000");
}

#[test]
fn url_encode_alpha_unchanged() {
    assert_eq!(url_encode("Hello"), "Hello");
}

#[test]
fn url_encode_space_becomes_plus() {
    assert_eq!(url_encode("hello world"), "hello+world");
}

#[test]
fn url_encode_special_chars() {
    assert_eq!(url_encode("a&b=c"), "a%26b%3Dc");
    assert_eq!(url_encode("/?#"), "%2F%3F%23");
}

#[test]
fn url_encode_unreserved_chars() {
    assert_eq!(url_encode("a-b_c.d~e"), "a-b_c.d~e");
}

#[test]
fn url_encode_unicode() {
    // UTF-8 multi-byte musi byt percent-encoded
    let result = url_encode("e+");
    // 'e' nezmeneno, '+' (0x2B) -> %2B
    assert_eq!(result, "e%2B");
}

#[test]
fn url_encode_digit_unchanged() {
    assert_eq!(url_encode("1234567890"), "1234567890");
}
