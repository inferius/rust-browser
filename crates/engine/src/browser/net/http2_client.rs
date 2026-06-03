//! Real HTTP/2 client - h2 crate + rustls + tokio mini runtime.
//!
//! Sync API (block_on internal tokio runtime) - kompatibilni s ostatnim
//! sync fetch path. HTTPS only (HTTP/2 cleartext H2C rarely deployed).
//!
//! ALPN negotiation - server musi nabidnout "h2". Pri jen "http/1.1"
//! caller fallne na ureq HTTP/1.1.
//!
//! Inspired by hyper-h2 client example + Chromium net::HttpNetworkTransaction.

use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Http2Response {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

#[derive(Debug)]
pub struct Http2Error(pub String);

/// Fetch URL pres HTTP/2 (HTTPS only). Sync block-on.
pub fn fetch_h2(url: &str) -> Result<Http2Response, Http2Error> {
    let parsed = url::Url::parse(url).map_err(|e| Http2Error(format!("URL parse: {}", e)))?;
    if parsed.scheme() != "https" {
        return Err(Http2Error("HTTP/2 client supports HTTPS only (ALPN)".into()));
    }
    let host = parsed.host_str().ok_or_else(|| Http2Error("missing host".into()))?.to_string();
    let port = parsed.port().unwrap_or(443);
    let path = if parsed.path().is_empty() { "/".to_string() } else { parsed.path().to_string() };
    let path_with_query = if let Some(q) = parsed.query() {
        format!("{}?{}", path, q)
    } else { path };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| Http2Error(format!("tokio rt build: {}", e)))?;

    rt.block_on(async {
        // TLS config - root cert store z webpki-roots.
        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let mut config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();
        config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        let connector = tokio_rustls::TlsConnector::from(Arc::new(config));

        // TCP connect.
        let addr = format!("{}:{}", host, port);
        let tcp = tokio::net::TcpStream::connect(&addr).await
            .map_err(|e| Http2Error(format!("TCP connect: {}", e)))?;
        // TLS handshake.
        let server_name = rustls::pki_types::ServerName::try_from(host.clone())
            .map_err(|e| Http2Error(format!("ServerName: {}", e)))?;
        let tls = connector.connect(server_name, tcp).await
            .map_err(|e| Http2Error(format!("TLS handshake: {}", e)))?;
        // Verify ALPN negotiated h2.
        let (_, server_conn) = tls.get_ref();
        if server_conn.alpn_protocol() != Some(b"h2") {
            return Err(Http2Error("server did not negotiate h2 ALPN".into()));
        }
        // h2 handshake.
        let (h2_client, h2_conn) = h2::client::handshake(tls).await
            .map_err(|e| Http2Error(format!("h2 handshake: {}", e)))?;
        // Spawn h2 connection driver - musi runt during request lifetime.
        let conn_handle = tokio::spawn(async move {
            let _ = h2_conn.await;
        });

        // Build request.
        let req = http::Request::builder()
            .method(http::Method::GET)
            .uri(format!("https://{}{}", host, path_with_query))
            .body(())
            .map_err(|e| Http2Error(format!("request build: {}", e)))?;
        let mut h2_client = h2_client.ready().await
            .map_err(|e| Http2Error(format!("h2 ready: {}", e)))?;
        let (resp_fut, _send_stream) = h2_client.send_request(req, true)
            .map_err(|e| Http2Error(format!("h2 send: {}", e)))?;
        let resp = resp_fut.await
            .map_err(|e| Http2Error(format!("h2 response: {}", e)))?;
        let status = resp.status().as_u16();
        let mut headers = HashMap::new();
        for (k, v) in resp.headers().iter() {
            if let Ok(vs) = v.to_str() {
                headers.insert(k.as_str().to_ascii_lowercase(), vs.to_string());
            }
        }
        let mut body = resp.into_body();
        let mut bytes_out = Vec::new();
        while let Some(chunk) = body.data().await {
            let chunk = chunk.map_err(|e| Http2Error(format!("body data: {}", e)))?;
            bytes_out.extend_from_slice(&chunk);
            let _ = body.flow_control().release_capacity(chunk.len());
        }
        conn_handle.abort();
        Ok(Http2Response { status, headers, body: bytes_out })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_http_scheme_rejected() {
        let r = fetch_h2("http://x.com/");
        assert!(r.is_err());
    }

    #[test]
    fn fetch_invalid_url_rejected() {
        let r = fetch_h2("not a url");
        assert!(r.is_err());
    }

    #[test]
    fn fetch_missing_host_rejected() {
        let r = fetch_h2("https:///path");
        assert!(r.is_err());
    }

    // Network tests vyzaduji online connectivity - ignored by default.
    #[test]
    #[ignore]
    fn fetch_real_https_url() {
        let r = fetch_h2("https://www.google.com/");
        match r {
            Ok(resp) => {
                println!("Got status {}", resp.status);
                assert!(resp.status > 0);
            }
            Err(e) => panic!("h2 fetch failed: {:?}", e),
        }
    }
}
