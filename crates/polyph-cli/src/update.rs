//! `polyph update`: replaces an install made from a release archive
//! (`scripts/install.sh`, `scripts/install.ps1`) with a newer release, in
//! place, the same way the install scripts do. Texts in `locales/*/update.ftl`.
//!
//! It downloads `SHA256SUMS` and this system's archive from the release,
//! checks the archive against the sum, unpacks it next to the install and
//! swaps the files in by renaming them: a running daemon keeps its old
//! binary until it restarts. Afterwards the daemon is restarted on the new
//! version, unless agents are working in it.
//!
//! `polyph uninstall` removes the same files (and, with `--purge`, this
//! device's Polyph data and keychain entry).

use std::path::{Path, PathBuf};
use std::time::Duration;

use polyph_i18n::tr;
use serde_json::json;
use sha2::{Digest, Sha256};

use super::Ctx;
use crate::client::{self, Api};

/// The repository whose releases this build follows (`POLYPH_REPO` at build
/// time for a fork).
const REPO: &str = match option_env!("POLYPH_REPO") {
    Some(v) if !v.is_empty() => v,
    _ => "ProjectDEV-git/Polyph",
};

/// Release target for this build, as in the archive names.
fn target() -> Option<String> {
    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        return None;
    };
    let os = if cfg!(target_os = "linux") {
        "unknown-linux-gnu"
    } else if cfg!(target_os = "macos") {
        "apple-darwin"
    } else if cfg!(target_os = "windows") {
        "pc-windows-msvc"
    } else {
        return None;
    };
    Some(format!("{arch}-{os}"))
}

/// An install laid out like a release archive: `<prefix>/bin/polyph` and
/// `<prefix>/share/polyph/web`.
#[derive(Debug, Clone, PartialEq)]
pub struct Install {
    pub prefix: PathBuf,
}

impl Install {
    pub fn from_exe(exe: &Path) -> Option<Self> {
        let bin = exe.parent()?;
        if bin.file_name()? != "bin" {
            return None;
        }
        let prefix = bin.parent()?.to_path_buf();
        prefix.join("share").join("polyph").join("web").is_dir().then_some(Self { prefix })
    }

    fn bin(&self) -> PathBuf {
        self.prefix.join("bin")
    }

    fn share(&self) -> PathBuf {
        self.prefix.join("share").join("polyph")
    }
}

/// The archive for `target` in a `SHA256SUMS` file: its sum, file name and
/// version.
pub fn pick(sums: &str, target: &str) -> Option<(String, String, String)> {
    let ext = if target.contains("windows") { ".zip" } else { ".tar.gz" };
    let tail = format!("-{target}{ext}");
    sums.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let hash = it.next()?;
        let name = it.next()?.trim_start_matches('*');
        let version = name.strip_prefix("polyph-")?.strip_suffix(tail.as_str())?;
        (!version.is_empty()).then(|| (hash.to_lowercase(), name.to_string(), version.to_string()))
    })
}

/// Orders versions like `0.2.0` and `0.2.0-rc.1` (a pre-release sorts
/// before its release).
fn key(v: &str) -> (u64, u64, u64, bool, String) {
    let v = v.trim_start_matches('v');
    let (core, pre) = v.split_once('-').unwrap_or((v, ""));
    let mut n = core.split('.').map(|p| p.parse::<u64>().unwrap_or(0));
    (n.next().unwrap_or(0), n.next().unwrap_or(0), n.next().unwrap_or(0), pre.is_empty(), pre.to_string())
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    key(candidate) > key(current)
}

/// Where a release's files come from: GitHub, or `POLYPH_UPDATE_BASE` (a URL
/// or a folder holding `SHA256SUMS` and the archives, as for the install
/// scripts' `POLYPH_INSTALL_BASE`).
#[derive(Debug)]
enum Source {
    Http(String),
    Dir(PathBuf),
}

impl Source {
    async fn get(&self, http: &reqwest::Client, name: &str) -> anyhow::Result<Vec<u8>> {
        match self {
            Source::Http(base) => {
                let url = format!("{}/{name}", base.trim_end_matches('/'));
                let r = http.get(&url).send().await?.error_for_status()?;
                Ok(r.bytes().await?.to_vec())
            }
            Source::Dir(dir) => Ok(std::fs::read(dir.join(name))?),
        }
    }
}

fn http() -> anyhow::Result<reqwest::Client> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    Ok(reqwest::Client::builder()
        .user_agent(concat!("polyph/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(600))
        .build()?)
}

/// The newest release's tag (`v0.2.0`), from where GitHub's "latest
/// release" link leads.
async fn latest_tag(http: &reqwest::Client) -> anyhow::Result<Option<String>> {
    let r = http.head(format!("https://github.com/{REPO}/releases/latest")).send().await?;
    let tag = r.url().path_segments().and_then(|mut s| s.next_back()).unwrap_or_default().to_string();
    Ok(tag.starts_with('v').then_some(tag))
}

/// `tar` that reads both formats: on Windows the system's bsdtar, not a
/// GNU tar from Git that cannot open zip files.
fn tar() -> PathBuf {
    if cfg!(windows)
        && let Ok(root) = std::env::var("SystemRoot")
    {
        let t = Path::new(&root).join("System32").join("tar.exe");
        if t.is_file() {
            return t;
        }
    }
    PathBuf::from("tar")
}

fn unpack(archive: &Path, into: &Path) -> anyhow::Result<()> {
    let flags = if archive.extension().is_some_and(|e| e == "zip") { "-xf" } else { "-xzf" };
    let status = std::process::Command::new(tar()).arg(flags).arg(archive).arg("-C").arg(into).status()?;
    anyhow::ensure!(status.success(), "tar could not unpack {}", archive.display());
    Ok(())
}

/// Moves the unpacked release (`src`, holding `bin/` and `share/polyph/`)
/// into the install. Every step is a rename within the install's folder.
fn swap_in(install: &Install, src: &Path) -> anyhow::Result<()> {
    let exe = std::env::consts::EXE_SUFFIX;
    let bin = install.bin();
    for b in ["polyph", "polyph-relay"] {
        let name = format!("{b}{exe}");
        let new = src.join("bin").join(&name);
        anyhow::ensure!(new.is_file(), "the release archive has no bin/{name}");
        let dest = bin.join(&name);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755))?;
        }
        // Windows cannot replace a running program, but it can rename it;
        // the old file is removed by the next update.
        #[cfg(windows)]
        if dest.exists() {
            let old = bin.join(format!("{name}.old"));
            let _ = std::fs::remove_file(&old);
            std::fs::rename(&dest, &old)?;
        }
        std::fs::rename(&new, &dest)?;
    }
    let share = install.share();
    let old = install.prefix.join("share").join("polyph.old");
    let _ = std::fs::remove_dir_all(&old);
    if share.exists() {
        std::fs::rename(&share, &old)?;
    }
    std::fs::rename(src.join("share").join("polyph"), &share)?;
    let _ = std::fs::remove_dir_all(&old);
    Ok(())
}

/// Removes what earlier updates could not (Windows keeps a running
/// program's file until it exits).
fn tidy(install: &Install) {
    for b in ["polyph", "polyph-relay"] {
        let _ = std::fs::remove_file(install.bin().join(format!("{b}{}.old", std::env::consts::EXE_SUFFIX)));
    }
    let _ = std::fs::remove_dir_all(install.prefix.join("share").join("polyph.old"));
}

/// What a check found: the release and its archive for this system.
#[derive(Debug)]
pub struct Found {
    version: String,
    file: String,
    sha256: String,
    source: Source,
}

async fn find(http: &reqwest::Client, target: &str, version: Option<&str>) -> anyhow::Result<Option<Found>> {
    let source = match std::env::var("POLYPH_UPDATE_BASE").ok().filter(|v| !v.is_empty()) {
        Some(b) if b.starts_with("http://") || b.starts_with("https://") => Source::Http(b),
        Some(b) => Source::Dir(PathBuf::from(b)),
        None => {
            let tag = match version {
                Some(v) => format!("v{}", v.trim_start_matches('v')),
                None => match latest_tag(http).await? {
                    Some(t) => t,
                    None => return Ok(None),
                },
            };
            Source::Http(format!("https://github.com/{REPO}/releases/download/{tag}"))
        }
    };
    let sums = String::from_utf8(source.get(http, "SHA256SUMS").await?)?;
    let Some((sha256, file, found)) = pick(&sums, target) else {
        anyhow::bail!("no-build");
    };
    Ok(Some(Found { version: found, file, sha256, source }))
}

/// Downloads, checks and installs `found` into `install`.
async fn apply(http: &reqwest::Client, install: &Install, found: &Found) -> anyhow::Result<Result<(), String>> {
    let bytes = found.source.get(http, &found.file).await?;
    let got = format!("{:x}", Sha256::digest(&bytes));
    if got != found.sha256 {
        return Ok(Err(found.file.clone()));
    }
    tidy(install);
    // Unpacked inside the install's folder so the swap is renames only.
    let work = tempfile::Builder::new().prefix(".polyph-update-").tempdir_in(&install.prefix)?;
    let archive = work.path().join(&found.file);
    std::fs::write(&archive, &bytes)?;
    unpack(&archive, work.path())?;
    let stem = found.file.trim_end_matches(".tar.gz").trim_end_matches(".zip");
    swap_in(install, &work.path().join(stem))?;
    Ok(Ok(()))
}

/// The daemon's answer to `ping`, when it is running.
async fn daemon_state(ctx: &Ctx) -> Option<u64> {
    let mut api = Api::connect(&ctx.paths, false).await.ok()?;
    let v = api.call("ping", json!({})).await.ok()?;
    Some(v["agents_running"].as_u64().unwrap_or(0))
}

/// Stops the daemon if it is running and waits until it has.
async fn stop_daemon(ctx: &Ctx) {
    if let Ok(mut api) = Api::connect(&ctx.paths, false).await {
        let _ = api.call("shutdown", json!({})).await;
        for _ in 0..50 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if client::daemon_port(&ctx.paths).is_none() {
                break;
            }
        }
    }
}

/// Stops the daemon and starts the new binary's.
async fn restart_daemon(ctx: &Ctx, polyph: &Path) -> anyhow::Result<()> {
    stop_daemon(ctx).await;
    // The new file, by path: this process may no longer find its own.
    let status = tokio::process::Command::new(polyph)
        .args(["daemon", "start"])
        .env("POLYPH_HOME", &ctx.paths.home)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await?;
    anyhow::ensure!(status.success(), "the new daemon did not start");
    Ok(())
}

pub async fn run(ctx: &Ctx, check: bool, version: Option<String>, force: bool) -> anyhow::Result<()> {
    let l = &ctx.l;
    let current = env!("CARGO_PKG_VERSION");
    let Some(target) = target() else {
        anyhow::bail!(l.t("update-unsupported"));
    };
    let exe = std::env::current_exe().and_then(|e| polyph_git::canonical(&e))?;
    let install = Install::from_exe(&exe);
    if install.is_none() && !check {
        anyhow::bail!(tr!(l, "update-not-archive", path = exe.display().to_string()));
    }
    let http = http()?;
    let found = match find(&http, &target, version.as_deref()).await {
        Ok(Some(f)) => f,
        Ok(None) => anyhow::bail!(tr!(l, "update-no-release", repo = REPO)),
        Err(e) if e.to_string() == "no-build" => anyhow::bail!(tr!(l, "update-no-build", target = target)),
        Err(e) => anyhow::bail!(tr!(l, "update-unreachable", error = e.to_string())),
    };
    let newer = is_newer(&found.version, current);
    if check || (!newer && !force && version.is_none()) {
        if ctx.json {
            ctx.print_json(&json!({ "current": current, "latest": found.version, "newer": newer, "updated": false }));
        } else if newer {
            ctx.out(tr!(l, "update-available", version = found.version.as_str(), current = current));
        } else {
            ctx.out(tr!(l, "update-current", version = current));
        }
        return Ok(());
    }
    let Some(install) = install else {
        anyhow::bail!(tr!(l, "update-not-archive", path = exe.display().to_string()));
    };
    let agents = daemon_state(ctx).await;
    if !ctx.json {
        ctx.out(tr!(l, "update-downloading", file = found.file.as_str()));
    }
    match apply(&http, &install, &found).await {
        Ok(Ok(())) => {}
        Ok(Err(file)) => anyhow::bail!(tr!(l, "update-checksum", file = file)),
        Err(e) => {
            anyhow::bail!(tr!(l, "update-failed", dir = install.prefix.display().to_string(), error = e.to_string()))
        }
    }
    let polyph = install.bin().join(format!("polyph{}", std::env::consts::EXE_SUFFIX));
    let restarted = match agents {
        Some(0) => restart_daemon(ctx, &polyph).await.is_ok(),
        _ => false,
    };
    if ctx.json {
        ctx.print_json(&json!({
            "current": current,
            "latest": found.version,
            "updated": true,
            "daemon_restarted": restarted,
        }));
        return Ok(());
    }
    ctx.out(tr!(l, "update-done", from = current, to = found.version.as_str()));
    match agents {
        Some(0) if restarted => ctx.out(l.t("update-daemon-restarted")),
        Some(0) => ctx.out(l.t("update-daemon-restart-failed")),
        Some(n) => ctx.out(tr!(l, "update-daemon-busy", count = n)),
        None => {}
    }
    Ok(())
}

/// Removes the install's files. Returns files Windows could not delete
/// because they are still running (renamed aside for [`delete_later`]).
fn remove_files(install: &Install) -> anyhow::Result<Vec<PathBuf>> {
    let exe = std::env::consts::EXE_SUFFIX;
    let mut busy = vec![];
    tidy(install);
    let share = install.share();
    if share.exists() {
        std::fs::remove_dir_all(&share)?;
    }
    for b in ["polyph-relay", "polyph"] {
        let file = install.bin().join(format!("{b}{exe}"));
        if !file.exists() {
            continue;
        }
        if let Err(e) = std::fs::remove_file(&file) {
            // A running program on Windows: move it aside, delete it later.
            let aside = install.bin().join(format!("{b}{exe}.old"));
            if cfg!(windows) && std::fs::rename(&file, &aside).is_ok() {
                busy.push(aside);
            } else {
                return Err(e.into());
            }
        }
    }
    Ok(busy)
}

/// Folders the install made for itself (the Windows default,
/// `%LOCALAPPDATA%\Programs\Polyph`), which go once empty. A shared prefix
/// such as `~/.local` is left alone.
fn own_dirs(install: &Install) -> Vec<PathBuf> {
    let own = install.prefix.file_name().is_some_and(|n| n.eq_ignore_ascii_case("polyph"));
    if !own {
        return vec![];
    }
    vec![install.bin(), install.prefix.join("share"), install.prefix.clone()]
}

/// Deletes files and empty folders a few seconds after this program exits,
/// for what Windows keeps while it runs.
#[cfg(windows)]
fn delete_later(files: &[PathBuf], dirs: &[PathBuf]) {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut script = String::from("ping -n 3 127.0.0.1 >nul");
    for f in files {
        script.push_str(&format!(" & del /f /q \"{}\"", f.display()));
    }
    for d in dirs {
        script.push_str(&format!(" & rmdir \"{}\"", d.display()));
    }
    polyph_sandbox::lowil::stop_std_handle_inheritance();
    let _ = std::process::Command::new("cmd")
        .raw_arg(format!("/d /s /c \"{script}\""))
        .creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

/// Takes the install's `bin` folder off the user's PATH, which the Windows
/// installer added.
#[cfg(windows)]
fn remove_from_path(bin: &Path) {
    let b = bin.display().to_string().replace('\'', "''");
    let script = format!(
        "$b='{b}'; $p=[Environment]::GetEnvironmentVariable('Path','User'); if ($p) {{ [Environment]::SetEnvironmentVariable('Path', ((($p -split ';') | Where-Object {{ $_ -and $_ -ne $b }}) -join ';'), 'User') }}"
    );
    let _ = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

fn confirm(question: &str) -> anyhow::Result<bool> {
    use std::io::Write;
    eprint!("{question} ");
    std::io::stderr().flush()?;
    let mut s = String::new();
    std::io::stdin().read_line(&mut s)?;
    Ok(matches!(s.trim().to_lowercase().as_str(), "y" | "yes"))
}

pub async fn uninstall(ctx: &Ctx, purge: bool, yes: bool) -> anyhow::Result<()> {
    use std::io::IsTerminal;
    let l = &ctx.l;
    let exe = std::env::current_exe().and_then(|e| polyph_git::canonical(&e))?;
    let Some(install) = Install::from_exe(&exe) else {
        anyhow::bail!(tr!(l, "uninstall-not-archive", path = exe.display().to_string()));
    };
    let home = ctx.paths.home.clone();
    if !yes {
        if !std::io::stdin().is_terminal() {
            anyhow::bail!(l.t("uninstall-needs-yes"));
        }
        ctx.out(tr!(l, "uninstall-what", dir = install.prefix.display().to_string()));
        if purge {
            ctx.out(tr!(l, "uninstall-purge-what", dir = home.display().to_string()));
        }
        if !confirm(&l.t("uninstall-confirm"))? {
            ctx.out(l.t("uninstall-cancelled"));
            return Ok(());
        }
    }
    stop_daemon(ctx).await;
    let busy = match remove_files(&install) {
        Ok(b) => b,
        Err(e) => {
            anyhow::bail!(tr!(l, "uninstall-failed", dir = install.prefix.display().to_string(), error = e.to_string()))
        }
    };
    let dirs = own_dirs(&install);
    for d in &dirs {
        // Only empty folders go; anything else someone put there stays.
        let _ = std::fs::remove_dir(d);
    }
    #[cfg(windows)]
    {
        remove_from_path(&install.bin());
        if !busy.is_empty() {
            delete_later(&busy, &dirs);
        }
    }
    #[cfg(not(windows))]
    let _ = busy;
    if purge {
        polyph_daemon::secrets::forget_keychain(&ctx.paths);
        if home.exists() {
            std::fs::remove_dir_all(&home)?;
        }
    }
    if ctx.json {
        ctx.print_json(&json!({ "removed": install.prefix, "purged": purge }));
        return Ok(());
    }
    ctx.out(l.t("uninstall-done"));
    if !purge && home.exists() {
        ctx.out(tr!(l, "uninstall-kept-data", dir = home.display().to_string()));
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn picks_this_systems_archive() {
        let sums = "\
aa11  Polyph_0.2.0_amd64.deb
BB22  polyph-0.2.0-x86_64-unknown-linux-gnu.tar.gz
cc33 *polyph-0.2.0-x86_64-pc-windows-msvc.zip
dd44  polyph-0.2.0-aarch64-unknown-linux-gnu.tar.gz
";
        let (sum, file, v) = pick(sums, "x86_64-unknown-linux-gnu").unwrap();
        assert_eq!(
            (sum.as_str(), file.as_str(), v.as_str()),
            ("bb22", "polyph-0.2.0-x86_64-unknown-linux-gnu.tar.gz", "0.2.0")
        );
        assert_eq!(pick(sums, "x86_64-pc-windows-msvc").unwrap().1, "polyph-0.2.0-x86_64-pc-windows-msvc.zip");
        assert!(pick(sums, "aarch64-apple-darwin").is_none());
    }

    #[test]
    fn versions_compare_as_releases() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("v0.10.0", "0.9.9"));
        assert!(is_newer("0.2.0", "0.2.0-rc.1"));
        assert!(!is_newer("0.2.0-rc.1", "0.2.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
    }

    #[test]
    fn only_archive_installs_update_themselves() {
        let d = tempfile::tempdir().unwrap();
        let bin = d.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let exe = bin.join("polyph");
        assert!(Install::from_exe(&exe).is_none(), "no web files next to it");
        std::fs::create_dir_all(d.path().join("share/polyph/web")).unwrap();
        assert_eq!(Install::from_exe(&exe), Some(Install { prefix: d.path().to_path_buf() }));
        assert!(Install::from_exe(&d.path().join("polyph")).is_none(), "not in a bin folder");
    }

    #[test]
    fn removes_only_its_own_files() {
        let d = tempfile::tempdir().unwrap();
        let exe = std::env::consts::EXE_SUFFIX;
        // A shared prefix: other programs' files stay, and so do the folders.
        let install = Install { prefix: d.path().join("local") };
        std::fs::create_dir_all(install.share().join("web")).unwrap();
        std::fs::create_dir_all(install.bin()).unwrap();
        for b in ["polyph", "polyph-relay", "other-tool"] {
            std::fs::write(install.bin().join(format!("{b}{exe}")), b).unwrap();
        }
        assert!(remove_files(&install).unwrap().is_empty());
        assert!(!install.share().exists());
        assert!(!install.bin().join(format!("polyph{exe}")).exists());
        assert!(install.bin().join(format!("other-tool{exe}")).exists());
        assert!(own_dirs(&install).is_empty());
        // A folder of its own (the Windows default) goes once empty.
        let own = Install { prefix: d.path().join("Programs").join("Polyph") };
        assert_eq!(own_dirs(&own).len(), 3);
    }

    #[test]
    fn swaps_binaries_and_web_files_in() {
        let d = tempfile::tempdir().unwrap();
        let install = Install { prefix: d.path().to_path_buf() };
        let exe = std::env::consts::EXE_SUFFIX;
        std::fs::create_dir_all(install.share().join("web")).unwrap();
        std::fs::create_dir_all(install.bin()).unwrap();
        std::fs::write(install.bin().join(format!("polyph{exe}")), "old").unwrap();
        std::fs::write(install.share().join("web/old.html"), "old").unwrap();
        let src = d.path().join("unpacked");
        std::fs::create_dir_all(src.join("bin")).unwrap();
        std::fs::create_dir_all(src.join("share/polyph/web")).unwrap();
        for b in ["polyph", "polyph-relay"] {
            std::fs::write(src.join("bin").join(format!("{b}{exe}")), format!("new {b}")).unwrap();
        }
        std::fs::write(src.join("share/polyph/web/index.html"), "new").unwrap();
        swap_in(&install, &src).unwrap();
        tidy(&install);
        assert_eq!(std::fs::read_to_string(install.bin().join(format!("polyph{exe}"))).unwrap(), "new polyph");
        assert_eq!(
            std::fs::read_to_string(install.bin().join(format!("polyph-relay{exe}"))).unwrap(),
            "new polyph-relay"
        );
        assert!(install.share().join("web/index.html").is_file());
        assert!(!install.share().join("web/old.html").exists(), "old web files are gone");
        assert!(!d.path().join("share/polyph.old").exists());
    }
}
