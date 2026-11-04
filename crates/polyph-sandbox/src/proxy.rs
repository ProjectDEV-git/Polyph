//! The daemon's outbound proxy for agents. Supports HTTP `CONNECT` (HTTPS
//! and anything tunnelled) and plain HTTP requests with absolute URIs.
//! Hosts on the allowlist pass; anything else asks the decider, which turns
//! it into an approval request. Listens on loopback TCP and, on Unix, on a
//! Unix socket that sandboxed namespaces reach through the helper.
//!
//! Plain HTTP requests to [`INTERNAL_HOST`] never leave the machine: they go to
//! the daemon's [`Internal`] handler. This is how the Polyph MCP server inside
//! the sandbox reaches its daemon under every sandbox mechanism, and because
//! each agent has its own proxy, the daemon knows which agent is calling.

use std::future::Future;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tracing::debug;

/// Decides about a host not on the allowlist. Returns true to allow.
pub type Decider = Arc<dyn Fn(String, u16) -> Pin<Box<dyn Future<Output = bool> + Send>> + Send + Sync>;

/// Host name answered by the daemon itself (see the module docs).
pub const INTERNAL_HOST: &str = "polyph.internal";
/// Largest request body the internal host accepts.
pub const INTERNAL_MAX_BODY: usize = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct InternalRequest {
    pub method: String,
    pub path: String,
    /// Value of the `Authorization` header.
    pub auth: Option<String>,
    pub body: Vec<u8>,
}

/// Answers requests to [`INTERNAL_HOST`] with (status, JSON body).
pub type Internal = Arc<dyn Fn(InternalRequest) -> Pin<Box<dyn Future<Output = (u16, Vec<u8>)> + Send>> + Send + Sync>;

#[derive(Clone, Debug, Default)]
pub struct Allowlist {
    /// Exact hosts or `*.suffix` wildcards.
    pub hosts: Vec<String>,
}

impl Allowlist {
    pub fn default_for_agents() -> Self {
        let hosts = [
            // Model APIs.
            "api.anthropic.com",
            "statsig.anthropic.com",
            "api.openai.com",
            "chatgpt.com",
            "*.openai.com",
            "generativelanguage.googleapis.com",
            "openrouter.ai",
            // Package registries.
            "registry.npmjs.org",
            "*.npmjs.org",
            "crates.io",
            "static.crates.io",
            "index.crates.io",
            "pypi.org",
            "files.pythonhosted.org",
            "proxy.golang.org",
            "sum.golang.org",
            "repo1.maven.org",
            "rubygems.org",
            "*.yarnpkg.com",
            // GitHub.
            "github.com",
            "*.github.com",
            "*.githubusercontent.com",
            "codeload.github.com",
        ];
        Self { hosts: hosts.iter().map(|s| s.to_string()).collect() }
    }

    pub fn allows(&self, host: &str) -> bool {
        let h = host.trim_end_matches('.').to_ascii_lowercase();
        self.hosts.iter().any(|a| match a.strip_prefix("*.") {
            Some(suffix) => h.ends_with(&format!(".{suffix}")),
            None => h == a.to_ascii_lowercase(),
        })
    }
}

#[derive(Debug)]
pub struct ProxyHandle {
    pub tcp: SocketAddr,
    pub unix: Option<PathBuf>,
    tasks: Vec<JoinHandle<()>>,
}

impl Drop for ProxyHandle {
    fn drop(&mut self) {
        for t in &self.tasks {
            t.abort();
        }
        if let Some(p) = &self.unix {
            let _ = std::fs::remove_file(p);
        }
    }
}

impl ProxyHandle {
    pub fn url(&self) -> String {
        format!("http://{}", self.tcp)
    }
}

/// Starts the proxy. `unix_dir` (Unix only) is where the socket is created.
pub async fn start(allow: Allowlist, decide: Decider, unix_dir: Option<PathBuf>) -> anyhow::Result<ProxyHandle> {
    start_with(allow, decide, None, unix_dir).await
}

/// Like [`start`], with a handler for [`INTERNAL_HOST`].
pub async fn start_with(
    allow: Allowlist,
    decide: Decider,
    internal: Option<Internal>,
    unix_dir: Option<PathBuf>,
) -> anyhow::Result<ProxyHandle> {
    let tcp = TcpListener::bind("127.0.0.1:0").await?;
    let addr = tcp.local_addr()?;
    let allow = Arc::new(allow);
    let mut tasks = vec![];
    {
        let (allow, decide, internal) = (allow.clone(), decide.clone(), internal.clone());
        tasks.push(tokio::spawn(async move {
            while let Ok((s, _)) = tcp.accept().await {
                let (allow, decide, internal) = (allow.clone(), decide.clone(), internal.clone());
                tokio::spawn(async move {
                    let _ = handle(s, allow, decide, internal).await;
                });
            }
        }));
    }
    #[cfg(unix)]
    let unix_path = match unix_dir {
        Some(dir) => {
            std::fs::create_dir_all(&dir)?;
            let p = dir.join(format!("proxy-{}-{}.sock", std::process::id(), addr.port()));
            let _ = std::fs::remove_file(&p);
            let ul = tokio::net::UnixListener::bind(&p)?;
            tasks.push(tokio::spawn(async move {
                while let Ok((s, _)) = ul.accept().await {
                    let (allow, decide, internal) = (allow.clone(), decide.clone(), internal.clone());
                    tokio::spawn(async move {
                        let _ = handle(s, allow, decide, internal).await;
                    });
                }
            }));
            Some(p)
        }
        None => None,
    };
    #[cfg(not(unix))]
    let unix_path = {
        let _ = (unix_dir, allow, decide, internal);
        None
    };
    Ok(ProxyHandle { tcp: addr, unix: unix_path, tasks })
}

const MAX_HEADER: usize = 64 * 1024;

async fn handle<S: AsyncRead + AsyncWrite + Unpin>(
    stream: S,
    allow: Arc<Allowlist>,
    decide: Decider,
    internal: Option<Internal>,
) -> anyhow::Result<()> {
    let mut reader = BufReader::new(stream);
    let mut head = String::new();
    loop {
        let mut line = String::new();
        let n = tokio::time::timeout(Duration::from_secs(30), reader.read_line(&mut line)).await??;
        if n == 0 {
            return Ok(());
        }
        head.push_str(&line);
        if head.len() > MAX_HEADER {
            anyhow::bail!("header too large");
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
    }
    let first = head.lines().next().unwrap_or_default().to_string();
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or_default().to_string();
    let (host, port, rest_path) = if method.eq_ignore_ascii_case("CONNECT") {
        let (h, p) = split_host_port(&target, 443);
        (h, p, None)
    } else {
        // Absolute URI: http://host[:port]/path
        let Some(after) = target.strip_prefix("http://") else {
            reader.get_mut().write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n").await?;
            return Ok(());
        };
        let (authority, path) = after.split_once('/').map(|(a, p)| (a, format!("/{p}"))).unwrap_or((after, "/".into()));
        let (h, p) = split_host_port(authority, 80);
        (h, p, Some(path))
    };
    if let Some(path) = &rest_path
        && host.eq_ignore_ascii_case(INTERNAL_HOST)
    {
        let header = |name: &str| {
            head.lines().skip(1).find_map(|l| {
                let (k, v) = l.split_once(':')?;
                k.trim().eq_ignore_ascii_case(name).then(|| v.trim().to_string())
            })
        };
        let len: usize = header("content-length").and_then(|v| v.parse().ok()).unwrap_or(0);
        let (status, body) = match &internal {
            None => (404, br#"{"error":"not available"}"#.to_vec()),
            Some(_) if len > INTERNAL_MAX_BODY => (413, br#"{"error":"request too large"}"#.to_vec()),
            Some(f) => {
                let mut body = vec![0u8; len];
                tokio::time::timeout(
                    Duration::from_secs(30),
                    tokio::io::AsyncReadExt::read_exact(&mut reader, &mut body),
                )
                .await??;
                f(InternalRequest { method, path: path.clone(), auth: header("authorization"), body }).await
            }
        };
        let reason = match status {
            200 => "OK",
            401 => "Unauthorized",
            404 => "Not Found",
            413 => "Payload Too Large",
            _ => "Error",
        };
        let out = reader.get_mut();
        out.write_all(
            format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .as_bytes(),
        )
        .await?;
        out.write_all(&body).await?;
        out.flush().await?;
        return Ok(());
    }
    let ok = allow.allows(&host) || decide(host.clone(), port).await;
    if !ok {
        debug!(%host, port, "proxy denied");
        let body = format!("Polyph blocked a connection to {host}:{port}. Approve it in Polyph to allow it.\n");
        reader.get_mut().write_all(format!("HTTP/1.1 403 Forbidden\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await?;
        return Ok(());
    }
    let mut upstream =
        match tokio::time::timeout(Duration::from_secs(15), TcpStream::connect((host.as_str(), port))).await {
            Ok(Ok(s)) => s,
            _ => {
                reader
                    .get_mut()
                    .write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    .await?;
                return Ok(());
            }
        };
    match rest_path {
        None => {
            reader.get_mut().write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await?;
        }
        Some(path) => {
            // Rewrite the request line to origin form and pass headers through.
            let mut lines = head.lines();
            let _ = lines.next();
            let version = first.split_whitespace().nth(2).unwrap_or("HTTP/1.1");
            let mut req = format!("{method} {path} {version}\r\n");
            for l in lines {
                if l.is_empty() || l.to_ascii_lowercase().starts_with("proxy-") {
                    continue;
                }
                req.push_str(l);
                req.push_str("\r\n");
            }
            req.push_str("\r\n");
            upstream.write_all(req.as_bytes()).await?;
        }
    }
    let buffered = reader.buffer().to_vec();
    let mut client = reader.into_inner();
    if !buffered.is_empty() {
        upstream.write_all(&buffered).await?;
    }
    let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
    Ok(())
}

fn split_host_port(s: &str, default: u16) -> (String, u16) {
    if let Some(rest) = s.strip_prefix('[') {
        // [v6]:port
        if let Some((h, p)) = rest.split_once("]:") {
            return (h.to_string(), p.parse().unwrap_or(default));
        }
        return (rest.trim_end_matches(']').to_string(), default);
    }
    match s.rsplit_once(':') {
        Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) => (h.to_string(), p.parse().unwrap_or(default)),
        _ => (s.to_string(), default),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    #[test]
    fn allowlist_matching() {
        let a = Allowlist { hosts: vec!["github.com".into(), "*.npmjs.org".into()] };
        assert!(a.allows("github.com"));
        assert!(a.allows("GitHub.com."));
        assert!(!a.allows("evilgithub.com"));
        assert!(a.allows("registry.npmjs.org"));
        assert!(!a.allows("npmjs.org.evil.com"));
        assert_eq!(split_host_port("[::1]:8080", 1), ("::1".into(), 8080));
        assert_eq!(split_host_port("example.com", 443), ("example.com".into(), 443));
    }

    #[tokio::test]
    async fn connect_allowed_and_denied() {
        let echo = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let echo_port = echo.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut s, _)) = echo.accept().await {
                tokio::spawn(async move {
                    let mut b = [0u8; 5];
                    if s.read_exact(&mut b).await.is_ok() {
                        let _ = s.write_all(&b).await;
                    }
                });
            }
        });
        let asked = Arc::new(std::sync::Mutex::new(vec![]));
        let a2 = asked.clone();
        let decide: Decider = Arc::new(move |h, p| {
            a2.lock().unwrap().push(format!("{h}:{p}"));
            Box::pin(async { false })
        });
        let p = start(Allowlist { hosts: vec!["127.0.0.1".into()] }, decide, None).await.unwrap();

        let mut s = TcpStream::connect(p.tcp).await.unwrap();
        s.write_all(format!("CONNECT 127.0.0.1:{echo_port} HTTP/1.1\r\nHost: x\r\n\r\nhello").as_bytes())
            .await
            .unwrap();
        let mut buf = vec![0u8; 64];
        let mut got = String::new();
        while !got.contains("hello") {
            let n = s.read(&mut buf).await.unwrap();
            assert!(n > 0);
            got.push_str(&String::from_utf8_lossy(&buf[..n]));
        }
        assert!(got.starts_with("HTTP/1.1 200"));

        let mut s = TcpStream::connect(p.tcp).await.unwrap();
        s.write_all(b"CONNECT unlisted.example:443 HTTP/1.1\r\n\r\n").await.unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).await.unwrap();
        assert!(out.starts_with("HTTP/1.1 403"), "{out}");
        assert_eq!(asked.lock().unwrap().as_slice(), ["unlisted.example:443"]);
    }

    #[tokio::test]
    async fn internal_host_is_answered_locally() {
        let decide: Decider = Arc::new(|_, _| Box::pin(async { panic!("internal requests are never decided") }));
        let internal: Internal = Arc::new(|r: InternalRequest| {
            Box::pin(async move {
                let ok = r.auth.as_deref() == Some("Bearer t0k");
                let body = format!(r#"{{"path":"{}","len":{},"ok":{ok}}}"#, r.path, r.body.len());
                (if ok { 200 } else { 401 }, body.into_bytes())
            })
        });
        let p = start_with(Allowlist::default(), decide, Some(internal), None).await.unwrap();
        let mut s = TcpStream::connect(p.tcp).await.unwrap();
        s.write_all(b"POST http://polyph.internal/mcp HTTP/1.1\r\nHost: polyph.internal\r\nAuthorization: Bearer t0k\r\nContent-Length: 5\r\n\r\nhello")
            .await
            .unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).await.unwrap();
        assert!(out.starts_with("HTTP/1.1 200"), "{out}");
        assert!(out.ends_with(r#"{"path":"/mcp","len":5,"ok":true}"#), "{out}");
    }
}
