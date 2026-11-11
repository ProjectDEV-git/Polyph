//! The helper that runs inside the sandbox (`polyph-sandbox-exec`, also
//! reachable as `polyph __sandbox`). Modes:
//!
//! - `forward --sock S --port P -- cmd...`: inside a private network
//!   namespace, listen on 127.0.0.1:P and forward to the daemon's proxy at
//!   Unix socket S, then run `cmd` and exit with its status.
//! - `landlock --proxy-port P --write DIR... -- cmd...`: restrict this
//!   process with Landlock (writes only to DIRs, TCP connects only to port
//!   P, and, from Landlock ABI 6, no abstract Unix sockets or signals
//!   outside the sandbox) and a seccomp filter (no Unix sockets but
//!   connected pairs, see [`apply_seccomp`]), then run `cmd`; the
//!   restriction is inherited by everything it spawns.
//! - `lowil --write DIR... -- cmd...` (Windows): label DIRs low integrity
//!   and run `cmd` at low integrity level, so it can write only there.
//! - `probe write PATH | connect HOST:PORT | proxy HOST:PORT | unix PATH |
//!   socketpair [stream|seqpacket|dgram|raw] | landlock | seccomp`: self-test
//!   probes; exit 0 when the action succeeded, 3 when blocked. `unix @NAME`
//!   is an abstract socket.
//!
//! `forward` and `landlock` run `cmd` without the variables in
//! [`crate::IPC_ENV`], which point at the person's session bus, agents and
//! display servers (SEC-14).

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

/// Marks every descriptor above 2 close-on-exec (SEC-14): a descriptor the
/// daemon or its parent left inheritable (a socket, a credential file) must
/// not reach the command. The helper's own stay usable until it starts the
/// command; std opens all of them close-on-exec anyway. Bubblewrap passes
/// such descriptors on to the helper, so `forward` does this too.
#[cfg(unix)]
#[allow(unsafe_code)]
fn close_inherited_fds() {
    #[cfg(target_os = "linux")]
    // SAFETY: close_range with CLOSE_RANGE_CLOEXEC only sets a flag; nothing
    // is closed under code that owns a descriptor.
    if unsafe { libc::syscall(libc::SYS_close_range, 3_u32, u32::MAX, libc::CLOSE_RANGE_CLOEXEC) } == 0 {
        return;
    }
    // Kernels before 5.11, and macOS: each open descriptor in turn.
    let dir = if cfg!(target_os = "linux") { "/proc/self/fd" } else { "/dev/fd" };
    let fds: Vec<libc::c_int> = std::fs::read_dir(dir)
        .map(|rd| rd.flatten().filter_map(|e| e.file_name().to_str()?.parse().ok()).filter(|fd| *fd > 2).collect())
        .unwrap_or_default();
    for fd in fds {
        // SAFETY: F_SETFD only sets a flag; a descriptor closed meanwhile
        // (the directory listing's own) gives EBADF, which is fine.
        unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
    }
}

#[cfg(not(unix))]
fn close_inherited_fds() {}

/// Detach from the controlling terminal so an agent cannot inject input
/// (TIOCSTI) into the person's session (SEC-14/KAO-106).
#[cfg(unix)]
#[allow(unsafe_code)]
fn detach_session() {
    // SAFETY: setsid(2) has no preconditions that Rust can violate here.
    unsafe { libc::setsid() };
}

#[cfg(not(unix))]
fn detach_session() {}
fn run_child(cmd: &[String]) -> i32 {
    let Some(prog) = cmd.first() else {
        eprintln!("no command given");
        return 2;
    };
    let mut c = std::process::Command::new(prog);
    c.args(&cmd[1..]);
    for k in crate::IPC_ENV {
        c.env_remove(k);
    }
    match c.status() {
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
    close_inherited_fds();
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
    detach_session();
    if let Err(e) = apply_seccomp() {
        eprintln!("sandbox: {e}; refusing to run without the seccomp filter");
        return BLOCKED;
    }
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
        RulesetStatus, Scope,
    };
    let abi = ABI::V5;
    // Landlock ABI 6 (Linux 6.12): no connecting to abstract Unix sockets
    // and no signals outside the sandbox. Older kernels skip this (best
    // effort); the seccomp filter refuses Unix sockets there too.
    let mut rs = Ruleset::default()
        .handle_access(AccessFs::from_all(abi))
        .and_then(|r| r.scope(Scope::from_all(ABI::V6)))
        .map_err(|e| e.to_string())?;
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

/// A seccomp filter for sandboxed modes (SEC-14/KAO-106). It keeps an agent
/// from host IPC and terminal injection: `socket(AF_UNIX)` and
/// `socketpair(AF_UNIX)` of any type but `SOCK_STREAM` and `SOCK_SEQPACKET`
/// fail with `EACCES`, `io_uring_setup` (which can create sockets without
/// `socket`) with `ENOSYS`, and `ioctl` requests `TIOCSTI`/`TIOCLINUX` fail
/// with `EPERM`. Seccomp cannot read the address a socket connects to, so
/// this refuses every Unix socket, not only those outside: connected stream
/// and seqpacket pairs (`socketpair`, which pipes between processes use)
/// still work. Other architectures' system calls (32-bit `int 0x80`) end the
/// process.
#[cfg(all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "riscv64")))]
pub fn apply_seccomp() -> Result<(), String> {
    use std::collections::BTreeMap;

    use seccompiler::{
        BpfProgram, SeccompAction, SeccompCmpArgLen, SeccompCmpOp, SeccompCondition, SeccompFilter, SeccompRule,
        TargetArch,
    };
    let e = |e: seccompiler::BackendError| e.to_string();
    let unix = || SeccompCondition::new(0, SeccompCmpArgLen::Dword, SeccompCmpOp::Eq, libc::AF_UNIX as u64);
    // A Unix pair of any type but stream and seqpacket: Linux turns
    // `SOCK_RAW` into a datagram socket, which can `sendto` any datagram
    // socket by path (journald, `/dev/log`, `$NOTIFY_SOCKET`). The type is
    // the low four bits (`SOCK_TYPE_MASK`); the rest are flags.
    let mut pairs = vec![];
    for ty in (0..=0xf_u64).filter(|t| ![libc::SOCK_STREAM as u64, libc::SOCK_SEQPACKET as u64].contains(t)) {
        let kind = SeccompCondition::new(1, SeccompCmpArgLen::Dword, SeccompCmpOp::MaskedEq(0xf), ty).map_err(e)?;
        pairs.push(SeccompRule::new(vec![unix().map_err(e)?, kind]).map_err(e)?);
    }
    let arch = TargetArch::try_from(std::env::consts::ARCH).map_err(e)?;
    let build = |rules: Vec<(i64, Vec<SeccompRule>)>, errno: i32| -> Result<BpfProgram, String> {
        let mut map: BTreeMap<i64, Vec<SeccompRule>> = BTreeMap::new();
        for (nr, r) in rules {
            // x32 calls (x86_64 numbers with bit 30 set) are checked too.
            if cfg!(target_arch = "x86_64") {
                map.insert(nr | 0x4000_0000, r.clone());
            }
            map.insert(nr, r);
        }
        let f = SeccompFilter::new(map, SeccompAction::Allow, SeccompAction::Errno(errno as u32), arch).map_err(e)?;
        f.try_into().map_err(|x: seccompiler::BackendError| x.to_string())
    };
    let sockets = build(
        vec![
            (libc::SYS_socket, vec![SeccompRule::new(vec![unix().map_err(e)?]).map_err(e)?]),
            (libc::SYS_socketpair, pairs),
        ],
        libc::EACCES,
    )?;
    let uring = build(vec![(libc::SYS_io_uring_setup, vec![])], libc::ENOSYS)?;
    // TIOCSTI (0x5412) types into the controlling terminal; TIOCLINUX (0x541C)
    // can paste/select in the console. Deny only these requests.
    #[allow(clippy::unnecessary_cast)]
    let tiocsti_req = libc::TIOCSTI as u64;
    let tiocsti = SeccompCondition::new(1, SeccompCmpArgLen::Dword, SeccompCmpOp::Eq, tiocsti_req).map_err(e)?;
    #[allow(clippy::unnecessary_cast)]
    let tioclinux_req = libc::TIOCLINUX as u64;
    let tioclinux = SeccompCondition::new(1, SeccompCmpArgLen::Dword, SeccompCmpOp::Eq, tioclinux_req).map_err(e)?;
    let ioctls = build(
        vec![(
            libc::SYS_ioctl,
            vec![SeccompRule::new(vec![tiocsti]).map_err(e)?, SeccompRule::new(vec![tioclinux]).map_err(e)?],
        )],
        libc::EPERM,
    )?;
    for p in [&sockets, &uring, &ioctls] {
        seccompiler::apply_filter(p).map_err(|x| x.to_string())?;
    }
    Ok(())
}

#[cfg(not(all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "riscv64"))))]
pub fn apply_seccomp() -> Result<(), String> {
    Err("no seccomp filter for this platform".into())
}

#[cfg(not(target_os = "linux"))]
fn apply_landlock(_writable: &[String], _proxy_port: Option<u16>) -> Result<bool, String> {
    Err("Landlock exists only on Linux".into())
}

fn landlock(args: Vec<String>) -> i32 {
    let (flags, cmd) = split_cmd(&args);
    let writable: Vec<String> = flags.windows(2).filter(|w| w[0] == "--write").map(|w| w[1].clone()).collect();
    let port = opt(&flags, "--proxy-port").and_then(|p| p.parse().ok());
    close_inherited_fds();
    detach_session();
    match apply_landlock(&writable, port) {
        Ok(true) => {
            if let Err(e) = apply_seccomp() {
                eprintln!(
                    "sandbox: no seccomp filter ({e}); the agent can still reach host IPC (the session bus, \
                     ssh-agent, docker.sock) over Unix sockets"
                );
            }
            run_child(&cmd)
        }
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
        "unix" => unix_connect(&target),
        "socketpair" => match target.as_str() {
            "" | "stream" => socket_pair(),
            kind => unix_pair(kind),
        },
        "landlock" => apply_landlock(&[], None).unwrap_or(false),
        // The filter applies and refuses a Unix socket and datagram pairs
        // (`SOCK_RAW` pairs are datagram pairs) afterwards; stream and
        // seqpacket pairs still work.
        "seccomp" => {
            apply_seccomp().is_ok()
                && unix_socket_refused()
                && socket_pair()
                && unix_pair("seqpacket")
                && !unix_pair("dgram")
                && !unix_pair("raw")
        }
        _ => return 2,
    };
    if ok { 0 } else { BLOCKED }
}

/// Connects to the Unix socket at `target` (`@name`: abstract, Linux only).
#[cfg(unix)]
fn unix_connect(target: &str) -> bool {
    use std::os::unix::net::UnixStream;
    #[cfg(any(target_os = "linux", target_os = "android"))]
    if let Some(name) = target.strip_prefix('@') {
        use std::os::linux::net::SocketAddrExt;
        return std::os::unix::net::SocketAddr::from_abstract_name(name)
            .and_then(|a| UnixStream::connect_addr(&a))
            .is_ok();
    }
    UnixStream::connect(target).is_ok()
}

#[cfg(not(unix))]
fn unix_connect(_target: &str) -> bool {
    false
}

/// Whether this process may not create a Unix socket.
#[cfg(unix)]
fn unix_socket_refused() -> bool {
    std::os::unix::net::UnixDatagram::unbound().is_err()
}

#[cfg(not(unix))]
fn unix_socket_refused() -> bool {
    false
}

/// A connected pair of Unix stream sockets, as pipes between processes use.
#[cfg(unix)]
fn socket_pair() -> bool {
    std::os::unix::net::UnixStream::pair()
        .and_then(|(mut a, mut b)| {
            a.write_all(b"x")?;
            let mut buf = [0u8; 1];
            b.read_exact(&mut buf)
        })
        .is_ok()
}

#[cfg(not(unix))]
fn socket_pair() -> bool {
    false
}

/// A connected pair of Unix sockets of type `kind` (`stream`, `seqpacket`,
/// `dgram` or `raw`), one byte sent through it.
#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
fn unix_pair(kind: &str) -> bool {
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    let ty = match kind {
        "stream" => libc::SOCK_STREAM,
        "seqpacket" => libc::SOCK_SEQPACKET,
        "dgram" => libc::SOCK_DGRAM,
        "raw" => libc::SOCK_RAW,
        _ => return false,
    };
    let mut fds = [-1 as libc::c_int; 2];
    // SAFETY: `fds` is a valid array of two ints for the kernel to fill.
    if unsafe { libc::socketpair(libc::AF_UNIX, ty | libc::SOCK_CLOEXEC, 0, fds.as_mut_ptr()) } != 0 {
        return false;
    }
    // SAFETY: socketpair succeeded, so both are new descriptors owned here.
    let (a, b) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
    let mut buf = [0u8; 1];
    // SAFETY: valid descriptors and one-byte buffers that outlive the calls.
    let (sent, got) = unsafe {
        (
            libc::send(a.as_raw_fd(), b"x".as_ptr().cast(), 1, 0),
            libc::recv(b.as_raw_fd(), buf.as_mut_ptr().cast(), 1, 0),
        )
    };
    sent == 1 && got == 1
}

#[cfg(not(target_os = "linux"))]
fn unix_pair(kind: &str) -> bool {
    match kind {
        "stream" => socket_pair(),
        _ => false,
    }
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
