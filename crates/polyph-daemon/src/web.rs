//! The browser client: the daemon serves its static files and signs a tab in
//! with a one-time link, so the API token never appears in a URL.
//!
//! `polyph web` calls `web.link` (API token only) and opens
//! `http://127.0.0.1:<port>/app/#code=<code>`. The page posts the code to
//! `/web/session` and gets a session token for the WebSocket. Codes are
//! single use and expire after two minutes; session tokens live in memory
//! until the daemon stops or a week passes.
//!
//! Every browser-facing route checks the `Host` header (127.0.0.1 or
//! localhost), so a rebound DNS name cannot reach it, and the WebSocket
//! rejects pages from any other origin.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use parking_lot::Mutex;

const LINK_TTL: Duration = Duration::from_secs(120);
const SESSION_TTL: Duration = Duration::from_secs(7 * 86_400);
/// Outstanding links and sessions kept at most (oldest dropped first).
const MAX_ENTRIES: usize = 64;

#[derive(Default)]
pub struct WebAuth {
    links: Mutex<HashMap<String, Instant>>,
    sessions: Mutex<HashMap<String, Instant>>,
}

impl std::fmt::Debug for WebAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print codes or tokens.
        f.debug_struct("WebAuth").finish_non_exhaustive()
    }
}

fn random_hex(n: usize) -> Option<String> {
    let mut b = vec![0u8; n];
    getrandom::getrandom(&mut b).ok()?;
    Some(hex::encode(b))
}

fn prune(m: &mut HashMap<String, Instant>, now: Instant) {
    m.retain(|_, exp| *exp > now);
    while m.len() >= MAX_ENTRIES {
        let Some(oldest) = m.iter().min_by_key(|(_, e)| **e).map(|(k, _)| k.clone()) else { break };
        m.remove(&oldest);
    }
}

impl WebAuth {
    /// A new one-time code for `polyph web`.
    pub fn link(&self) -> Option<String> {
        let code = random_hex(16)?;
        let now = Instant::now();
        let mut l = self.links.lock();
        prune(&mut l, now);
        l.insert(code.clone(), now + LINK_TTL);
        Some(code)
    }

    /// Trades a one-time code for a session token.
    pub fn redeem(&self, code: &str) -> Option<String> {
        let now = Instant::now();
        let exp = self.links.lock().remove(code)?;
        if exp <= now {
            return None;
        }
        let token = random_hex(32)?;
        let mut s = self.sessions.lock();
        prune(&mut s, now);
        s.insert(token.clone(), now + SESSION_TTL);
        Some(token)
    }

    pub fn session_valid(&self, token: &str) -> bool {
        let now = Instant::now();
        let s = self.sessions.lock();
        // Compare against every entry so timing does not reveal a prefix.
        s.iter().fold(false, |ok, (t, exp)| ok | (crate::api::ct_eq(t.as_bytes(), token.as_bytes()) && *exp > now))
    }
}

/// True when the request was addressed to this machine's loopback name.
pub fn local_host(headers: &HeaderMap) -> bool {
    let Some(h) = headers.get(header::HOST).and_then(|v| v.to_str().ok()) else { return false };
    let name = match h.rsplit_once(':') {
        Some((n, port)) if port.chars().all(|c| c.is_ascii_digit()) => n,
        _ => h,
    };
    matches!(name, "127.0.0.1" | "localhost" | "[::1]")
}

/// Origins allowed to open the WebSocket: the daemon's own pages and the
/// desktop app's webview. Clients without an Origin (CLI, TUI) are not browsers.
pub fn origin_allowed(headers: &HeaderMap) -> bool {
    let Some(o) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else { return true };
    if matches!(o, "tauri://localhost" | "http://tauri.localhost" | "https://tauri.localhost") {
        return true;
    }
    let rest = o.strip_prefix("http://").unwrap_or("");
    let host = rest.rsplit_once(':').map(|(h, _)| h).unwrap_or(rest);
    matches!(host, "127.0.0.1" | "localhost" | "[::1]")
}

/// Where the browser client's files are: `POLYPH_WEB_DIR`, else next to the
/// executable (`../share/polyph/web` or `web/`).
pub fn web_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("POLYPH_WEB_DIR") {
        return Some(PathBuf::from(d));
    }
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    [dir.join("../share/polyph/web"), dir.join("web")].into_iter().find(|p| p.join("app/index.html").is_file())
}

fn content_type(p: &Path) -> &'static str {
    match p.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "webmanifest" => "application/manifest+json",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "txt" => "text/plain; charset=utf-8",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

/// The page policy: scripts and styles from this origin only, the WebSocket
/// back to the daemon, no frames, no third-party requests.
const CSP: &str = "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; \
img-src 'self' data: blob:; font-src 'self' data:; connect-src 'self' ws://127.0.0.1:* ws://localhost:*; \
worker-src 'self' blob:; frame-ancestors 'none'; base-uri 'self'; form-action 'none'";

/// Serves a file from the client's static export. Paths that leave the
/// directory are refused; a directory serves its `index.html`.
pub async fn serve_file(dir: &Path, path: &str) -> Response {
    let rel = Path::new(path.trim_start_matches('/'));
    if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let mut p = dir.join(rel);
    if p.is_dir() {
        p = p.join("index.html");
    } else if !p.exists() && p.extension().is_none() {
        // Next.js static export writes `/app` as `app.html` or `app/index.html`.
        p.set_extension("html");
    }
    match tokio::fs::read(&p).await {
        Ok(body) => {
            let mut r = body.into_response();
            let h = r.headers_mut();
            h.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type(&p)));
            h.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(CSP));
            h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
            h.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
            let cache =
                if path.contains("/_next/static/") { "public, max-age=31536000, immutable" } else { "no-cache" };
            h.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
            r
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn links_are_single_use() {
        let w = WebAuth::default();
        let code = w.link().expect("randomness");
        let token = w.redeem(&code).expect("first use works");
        assert!(w.session_valid(&token));
        assert!(w.redeem(&code).is_none(), "second use fails");
        assert!(!w.session_valid("nope"));
    }

    #[test]
    fn hosts_and_origins() {
        let mut h = HeaderMap::new();
        h.insert(header::HOST, HeaderValue::from_static("127.0.0.1:4100"));
        assert!(local_host(&h));
        h.insert(header::HOST, HeaderValue::from_static("evil.example:4100"));
        assert!(!local_host(&h));
        let mut o = HeaderMap::new();
        assert!(origin_allowed(&o));
        o.insert(header::ORIGIN, HeaderValue::from_static("http://127.0.0.1:4100"));
        assert!(origin_allowed(&o));
        o.insert(header::ORIGIN, HeaderValue::from_static("tauri://localhost"));
        assert!(origin_allowed(&o));
        o.insert(header::ORIGIN, HeaderValue::from_static("https://evil.example"));
        assert!(!origin_allowed(&o));
        o.insert(header::ORIGIN, HeaderValue::from_static("http://127.0.0.1.evil.example"));
        assert!(!origin_allowed(&o));
    }

    #[tokio::test]
    async fn static_paths_stay_inside() {
        let dir = std::env::temp_dir().join(format!("polyph-web-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("app")).unwrap();
        std::fs::write(dir.join("app/index.html"), "<p>hi</p>").unwrap();
        assert_eq!(serve_file(&dir, "/app/").await.status(), StatusCode::OK);
        assert_eq!(serve_file(&dir, "/app").await.status(), StatusCode::OK);
        assert_eq!(serve_file(&dir, "/../etc/passwd").await.status(), StatusCode::NOT_FOUND);
        assert_eq!(serve_file(&dir, "/missing.js").await.status(), StatusCode::NOT_FOUND);
        let _ = std::fs::remove_dir_all(dir);
    }
}
