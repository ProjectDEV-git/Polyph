//! The helper that runs inside the sandbox (`polyph-sandbox-exec`, also
//! reachable as `polyph __sandbox`). Modes:
//!
//! - `forward --sock S --port P -- cmd...`: inside a private network
//!   namespace, listen on 127.0.0.1:P and forward to the daemon's proxy at
//!   Unix socket S, then run `cmd` and exit with its status.
//! - `landlock --proxy-port P --write DIR... -- cmd...`: restrict this
//!   process with Landlock (writes only to DIRs, TCP connects only to port
//!   P), then run `cmd`; the restriction is inherited by everything it spawns.
//! - `lowil --write DIR... -- cmd...` (Windows): label DIRs low integrity
//!   and run `cmd` at low integrity level, so it can write only there.
//! - `probe write PATH | connect HOST:PORT | proxy HOST:PORT | landlock`:
//!   self-test probes; exit 0 when the action succeeded, 3 when blocked.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

pub const BLOCKED: i32 = 3;

pub fn main_with_args(args: Vec<String>) -> i32 {
    let mode = args.first().map(String::as_str).unwrap_or_default();
    let rest = args.get(1..).unwrap_or_default().to_vec();
    match mode {
        "forward" => forward(rest),
        "landlock" => landlock(rest),
        "lowil" => lowil(rest),
        "probe" => probe(rest),
        _ => {
            eprintln!("usage: polyph-sandbox-exec forward|landlock|lowil|probe ...");
            2
        }
    }
}

fn split_cmd(args: &[String]) -> (Vec<String>, Vec<String>) {
    match args.iter().position(|a| a == "--") {
        Some(i) => (args[..i].to_vec(), args[i + 1..].to_vec()),
        None => (args.to_vec(), vec![]),
    }
}

fn opt(flags: &[String], name: &str) -> Option<String> {
    flags.iter().position(|a| a == name).and_then(|i| flags.get(i + 1)).cloned()
}

fn run_child(cmd: &[String]) -> i32 {
    let Some(prog) = cmd.first() else {
        eprintln!("no command given");
        return 2;
    };
    match std::process::Command::new(prog).args(&cmd[1..]).status() {
        Ok(s) => s.code().unwrap_or(1),
        Err(e) => {
            eprintln!("cannot run {prog}: {e}");
            127
        }
    }
}

fn lowil(args: Vec<String>) -> i32 {
    let (flags, cmd) = split_cmd(&args);
    let writable: Vec<std::path::PathBuf> = flags
        .iter()
        .enumerate()
        .filter(|(_, f)| *f == "--write")
        .filter_map(|(i, _)| flags.get(i + 1).map(std::path::PathBuf::from))
        .collect();
    #[cfg(windows)]
    {
        match crate::lowil::run(&writable, &cmd) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("sandbox: {e}");
                126
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (writable, cmd);
        eprintln!("lowil works only on Windows");
        2
    }
}

#[cfg(unix)]
fn forward(args: Vec<String>) -> i32 {
    use std::net::TcpListener;
    use std::os::unix::net::UnixStream;
    let (flags, cmd) = split_cmd(&args);
    let (Some(sock), Some(port)) = (opt(&flags, "--sock"), opt(&flags, "--port").and_then(|p| p.parse::<u16>().ok()))
    else {
        eprintln!("forward needs --sock and --port");
        return 2;
    };
    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("cannot listen on 127.0.0.1:{port}: {e}");
            return 2;
        }
    };
    std::thread::spawn(move || {
        for c in listener.incoming().flatten() {
            let sock = sock.clone();
            std::thread::spawn(move || {
                let Ok(u) = UnixStream::connect(&sock) else { return };
                let (Ok(mut c2), Ok(mut u2)) = (c.try_clone(), u.try_clone()) else { return };
                let mut c = c;
                let mut u = u;
                let t = std::thread::spawn(move || {
                    let _ = std::io::copy(&mut c2, &mut u2);
                    let _ = u2.shutdown(std::net::Shutdown::Write);
                });
                let _ = std::io::copy(&mut u, &mut c);
                let _ = c.shutdown(std::net::Shutdown::Write);
                let _ = t.join();
            });
        }
    });
    run_child(&cmd)
}

#[cfg(not(unix))]
fn forward(_args: Vec<String>) -> i32 {
    eprintln!("forward mode needs Unix sockets");
    2
}

#[cfg(target_os = "linux")]
fn apply_landlock(writable: &[String], proxy_port: Option<u16>) -> Result<bool, String> {
    use landlock::{
        ABI, Access, AccessFs, AccessNet, NetPort, PathBeneath, PathFd, Ruleset, RulesetAttr, RulesetCreatedAttr,
        RulesetStatus,
    };
    let abi = ABI::V5;
    let mut rs = Ruleset::default().handle_access(AccessFs::from_all(abi)).map_err(|e| e.to_string())?;
    if proxy_port.is_some() {
        rs = rs.handle_access(AccessNet::ConnectTcp | AccessNet::BindTcp).map_err(|e| e.to_string())?;
    }
    let mut created = rs.create().map_err(|e| e.to_string())?;
    let ro = PathFd::new("/").map_err(|e| e.to_string())?;
    created = created.add_rule(PathBeneath::new(ro, AccessFs::from_read(abi))).map_err(|e| e.to_string())?;
    for w in writable.iter().map(String::as_str).chain(["/dev"]) {
        if let Ok(fd) = PathFd::new(w) {
            created = created.add_rule(PathBeneath::new(fd, AccessFs::from_all(abi))).map_err(|e| e.to_string())?;
        }
    }
    if let Some(p) = proxy_port {
        created = created.add_rule(NetPort::new(p, AccessNet::ConnectTcp)).map_err(|e| e.to_string())?;
    }
    let status = created.restrict_self().map_err(|e| e.to_string())?;
    Ok(!matches!(status.ruleset, RulesetStatus::NotEnforced))
}

#[cfg(not(target_os = "linux"))]
fn apply_landlock(_writable: &[String], _proxy_port: Option<u16>) -> Result<bool, String> {
    Err("Landlock exists only on Linux".into())
}

fn landlock(args: Vec<String>) -> i32 {
    let (flags, cmd) = split_cmd(&args);
    let writable: Vec<String> = flags.windows(2).filter(|w| w[0] == "--write").map(|w| w[1].clone()).collect();
    let port = opt(&flags, "--proxy-port").and_then(|p| p.parse().ok());
    match apply_landlock(&writable, port) {
        Ok(true) => run_child(&cmd),
        Ok(false) => {
            eprintln!("this kernel does not enforce Landlock; refusing to run unsandboxed");
            BLOCKED
        }
        Err(e) => {
            eprintln!("Landlock setup failed: {e}");
            BLOCKED
        }
    }
}

fn probe(args: Vec<String>) -> i32 {
    let what = args.first().map(String::as_str).unwrap_or_default();
    let target = args.get(1).cloned().unwrap_or_default();
    let ok = match what {
        "write" => std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&target)
            .and_then(|mut f| f.write_all(b"probe"))
            .is_ok(),
        "connect" => TcpStream::connect_timeout(
            &match target.parse() {
                Ok(a) => a,
                Err(_) => return 2,
            },
            Duration::from_secs(3),
        )
        .is_ok(),
        "proxy" => proxy_connect(&target),
        "landlock" => apply_landlock(&[], None).unwrap_or(false),
        _ => return 2,
    };
    if ok { 0 } else { BLOCKED }
}

/// CONNECT through the proxy named in HTTPS_PROXY; true when it answered 200.
fn proxy_connect(target: &str) -> bool {
    let Ok(url) = std::env::var("HTTPS_PROXY") else { return false };
    let addr = url.trim_start_matches("http://").trim_end_matches('/');
    let Ok(mut s) = TcpStream::connect(addr) else { return false };
    let _ = s.set_read_timeout(Some(Duration::from_secs(10)));
    if s.write_all(format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n\r\n").as_bytes()).is_err() {
        return false;
    }
    let mut buf = [0u8; 32];
    let n = s.read(&mut buf).unwrap_or(0);
    buf[..n].starts_with(b"HTTP/1.1 200")
}
