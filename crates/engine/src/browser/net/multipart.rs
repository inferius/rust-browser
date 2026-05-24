//! `multipart/form-data` parser + builder.
//!
//! Spec: RFC 7578.
//! Used for HTML form submit s file uploads + Web Share Target POST.

#[derive(Debug, Clone)]
pub struct MultipartPart {
    pub name: String,
    pub filename: Option<String>,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

/// Build multipart body from parts. Returns (body, boundary).
pub fn build(parts: &[MultipartPart]) -> (Vec<u8>, String) {
    let boundary = format!("----rweFormBoundary{}", std::process::id());
    let mut body = Vec::new();
    for p in parts {
        body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
        if let Some(fname) = &p.filename {
            body.extend_from_slice(
                format!("Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n",
                        p.name, fname).as_bytes()
            );
        } else {
            body.extend_from_slice(format!("Content-Disposition: form-data; name=\"{}\"\r\n", p.name).as_bytes());
        }
        if let Some(ct) = &p.content_type {
            body.extend_from_slice(format!("Content-Type: {}\r\n", ct).as_bytes());
        }
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(&p.body);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());
    (body, boundary)
}

/// Parse multipart/form-data body given boundary.
pub fn parse(body: &[u8], boundary: &str) -> Result<Vec<MultipartPart>, String> {
    let sep = format!("--{}", boundary);
    let sep_bytes = sep.as_bytes();
    let mut parts = Vec::new();
    let mut idx = 0;
    while let Some(start) = find_subseq(&body[idx..], sep_bytes) {
        let after_sep = idx + start + sep_bytes.len();
        // Check for terminator
        if after_sep + 2 <= body.len() && &body[after_sep..after_sep + 2] == b"--" {
            break;
        }
        // Skip CRLF after boundary
        let mut header_start = after_sep;
        if header_start + 2 <= body.len() && &body[header_start..header_start + 2] == b"\r\n" {
            header_start += 2;
        }
        // Find next boundary
        let next_idx = match find_subseq(&body[header_start..], sep_bytes) {
            Some(n) => header_start + n,
            None => return Err("missing closing boundary".into()),
        };
        let part_bytes = &body[header_start..next_idx.saturating_sub(2)];
        let (headers, content) = split_headers(part_bytes)?;
        let mut name = String::new();
        let mut filename = None;
        let mut content_type = None;
        for line in headers.split(|b| *b == b'\n') {
            let line = std::str::from_utf8(line).map_err(|e| e.to_string())?.trim_end_matches('\r');
            if let Some(rest) = line.strip_prefix("Content-Disposition:") {
                for kv in rest.split(';') {
                    let kv = kv.trim();
                    if let Some(v) = kv.strip_prefix("name=") {
                        name = v.trim_matches('"').to_string();
                    } else if let Some(v) = kv.strip_prefix("filename=") {
                        filename = Some(v.trim_matches('"').to_string());
                    }
                }
            } else if let Some(rest) = line.strip_prefix("Content-Type:") {
                content_type = Some(rest.trim().to_string());
            }
        }
        parts.push(MultipartPart { name, filename, content_type, body: content.to_vec() });
        idx = next_idx;
    }
    Ok(parts)
}

fn find_subseq(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() { return None; }
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn split_headers(buf: &[u8]) -> Result<(&[u8], &[u8]), String> {
    let sep = find_subseq(buf, b"\r\n\r\n").ok_or("missing header/body separator")?;
    Ok((&buf[..sep], &buf[sep + 4..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_simple() {
        let parts = vec![
            MultipartPart { name: "field1".into(), filename: None, content_type: None, body: b"value1".to_vec() },
        ];
        let (body, _b) = build(&parts);
        let s = String::from_utf8(body).unwrap();
        assert!(s.contains("Content-Disposition: form-data; name=\"field1\""));
        assert!(s.contains("value1"));
    }

    #[test]
    fn build_file_part() {
        let parts = vec![
            MultipartPart {
                name: "file".into(),
                filename: Some("a.txt".into()),
                content_type: Some("text/plain".into()),
                body: b"hi".to_vec(),
            },
        ];
        let (body, _b) = build(&parts);
        let s = String::from_utf8(body).unwrap();
        assert!(s.contains("filename=\"a.txt\""));
        assert!(s.contains("Content-Type: text/plain"));
    }

    #[test]
    fn roundtrip_parse() {
        let parts = vec![
            MultipartPart { name: "x".into(), filename: None, content_type: None, body: b"X".to_vec() },
            MultipartPart { name: "y".into(), filename: None, content_type: None, body: b"YY".to_vec() },
        ];
        let (body, boundary) = build(&parts);
        let parsed = parse(&body, &boundary).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].name, "x");
        assert_eq!(parsed[0].body, b"X");
        assert_eq!(parsed[1].name, "y");
        assert_eq!(parsed[1].body, b"YY");
    }

    #[test]
    fn parse_missing_terminator_errors() {
        // Body lacking closing boundary
        let bad = b"--bnd\r\nContent-Disposition: form-data; name=\"x\"\r\n\r\nvalue";
        assert!(parse(bad, "bnd").is_err());
    }
}
