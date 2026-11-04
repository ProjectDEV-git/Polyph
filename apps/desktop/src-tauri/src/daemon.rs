//! Finds the person's daemon, starting it when it is not running. The same
//! files the CLI uses: `daemon.json` (port) and `api.token` in the Polyph
//! home (`POLYPH_HOME`, else the platform config directory).

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

pub fn home() -> Option<PathBuf> {
    if let Some(h) = std::env::var_os("POLYPH_HOME") {
        return Some(PathBuf::from(h));
    }
    directories::ProjectDirs::from("dev", "polyph", "polyph").map(|d| d.config_dir().to_path_buf())
}

fn port(home: &Path) -> Option<u16> {
    let s = std::fs::read_to_string(home.join("daemon.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&s).ok()?;
    v.get("port")?.as_u64().and_then(|p| u16::try_from(p).ok())
}

fn healthy(port: u16) -> bool {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut s) = TcpStream::connect_timeout(&addr, Duration::from_millis(500)) else { return false };
    let _ = s.set_read_timeout(Some(Duration::from_millis(500)));
    if s.write_all(b"GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").is_err() {
        return false;
    }
    let mut buf = [0u8; 64];
    matches!(s.read(&mut buf), Ok(n) if n > 0 && buf[..n].starts_with(b"HTTP/1.1 200"))
}

/// The `polyph` binary: next to the app (bundled), else on PATH.
fn polyph_bin() -> PathBuf {
    let name = if cfg!(windows) { "polyph.exe" } else { "polyph" };
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        let p = dir.join(name);
        if p.is_file() {
            return p;
        }
    }
    PathBuf::from(name)
}

fn spawn(home: &Path) -> std::io::Result<()> {
    let mut cmd = Command::new(polyph_bin());
    cmd.args(["daemon", "run"])
        .env("POLYPH_HOME", home)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Its own process group: quitting the app leaves the daemon running.
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0000_0008 | 0x0000_0200);
    }
    cmd.spawn().map(|_| ())
}

#[derive(serde::Serialize, Clone)]
pub struct Conn {
    pub port: u16,
    pub token: String,
}

/// The daemon's port and API token, starting the daemon if needed.
pub fn ensure() -> Result<Conn, String> {
    let home = home().ok_or("no home directory")?;
    let running = port(&home).filter(|p| healthy(*p));
    let port = match running {
        Some(p) => p,
        None => {
            spawn(&home).map_err(|e| e.to_string())?;
            let mut found = None;
            for _ in 0..100 {
                std::thread::sleep(Duration::from_millis(100));
                if let Some(p) = port(&home).filter(|p| healthy(*p)) {
                    found = Some(p);
                    break;
                }
            }
            found.ok_or("the daemon did not start in time")?
        }
    };
    let token = std::fs::read_to_string(home.join("api.token")).map_err(|e| e.to_string())?;
    Ok(Conn { port, token: token.trim().to_string() })
}
