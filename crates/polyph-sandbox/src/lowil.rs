//! Windows file isolation: runs a command at low integrity level.
//!
//! Windows' mandatory integrity control stops a low-integrity process from
//! writing to anything labelled at a higher level, which is every ordinary
//! file and folder a user owns (they are medium by default). The helper
//! labels the policy's writable folders (the worktree, extra writable paths
//! and the agent's TMPDIR) low, with inheritance, then starts the command
//! with a low-integrity copy of its own token. No administrator rights are
//! needed: the token is a restricted version of the caller's, and the
//! caller owns the folders it labels.
//!
//! The command also goes into a job object that kills it when the helper
//! exits, so stopping the helper stops the whole agent.
//!
//! What this does not do: it does not stop reads (as on the other
//! platforms, reads are allowed), and it does not isolate the network; the
//! proxy variables still route HTTP through the allowlisting proxy, but a
//! program can open other connections. The low label stays on the folders
//! afterwards, so other low-integrity programs can write to them too.
#![allow(unsafe_code)] // Win32 calls; each one is checked and its handles closed.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, HANDLE, HANDLE_FLAG_INHERIT, LocalFree, SetHandleInformation,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, ConvertStringSidToSidW, SDDL_REVISION_1, SE_FILE_OBJECT,
    SetNamedSecurityInfoW,
};
use windows_sys::Win32::Security::{
    ACL, CreateRestrictedToken, GetLengthSid, GetSecurityDescriptorSacl, LABEL_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR, PSID, SID_AND_ATTRIBUTES, SetTokenInformation, TOKEN_ADJUST_DEFAULT, TOKEN_ASSIGN_PRIMARY,
    TOKEN_DUPLICATE, TOKEN_MANDATORY_LABEL, TOKEN_QUERY, TokenIntegrityLevel,
};
use windows_sys::Win32::System::Console::{GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE};
use windows_sys::Win32::System::Threading::{
    CREATE_SUSPENDED, CreateProcessAsUserW, GetCurrentProcess, GetExitCodeProcess, INFINITE, OpenProcessToken,
    PROCESS_INFORMATION, ResumeThread, STARTF_USESTDHANDLES, STARTUPINFOW, WaitForSingleObject,
};

/// `SE_GROUP_INTEGRITY`: the SID in a mandatory label is an integrity level.
const SE_GROUP_INTEGRITY: u32 = 0x20;
/// The low integrity level.
const LOW_INTEGRITY_SID: &str = "S-1-16-4096";
/// A low mandatory label that inherits to files and folders, blocking writes from below.
const LOW_LABEL_SDDL: &str = "S:(ML;OICI;NW;;;LW)";

fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

fn last_error(what: &str) -> String {
    format!("{what} failed (Windows error {})", unsafe { GetLastError() })
}

/// Closes a handle when dropped.
struct Owned(HANDLE);

impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CloseHandle(self.0) };
        }
    }
}

/// Labels `dir` (and, by inheritance, everything in it) low integrity, so a
/// low-integrity process may write there.
pub fn label_low(dir: &Path) -> Result<(), String> {
    let sddl = wide(OsStr::new(LOW_LABEL_SDDL));
    let mut sd: PSECURITY_DESCRIPTOR = null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), SDDL_REVISION_1, &mut sd, null_mut())
    } == 0
    {
        return Err(last_error("building the low label"));
    }
    let mut present = 0;
    let mut defaulted = 0;
    let mut sacl: *mut ACL = null_mut();
    let got = unsafe { GetSecurityDescriptorSacl(sd, &mut present, &mut sacl, &mut defaulted) };
    let result = if got == 0 || present == 0 {
        Err(last_error("reading the low label"))
    } else {
        let path = wide(dir.as_os_str());
        let code = unsafe {
            SetNamedSecurityInfoW(
                path.as_ptr(),
                SE_FILE_OBJECT,
                LABEL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                null(),
                sacl,
            )
        };
        if code == 0 { Ok(()) } else { Err(format!("labelling {} failed (Windows error {code})", dir.display())) }
    };
    unsafe { LocalFree(sd) };
    result
}

/// A primary token like this process's, at low integrity level.
fn low_token() -> Result<Owned, String> {
    let mut own: HANDLE = null_mut();
    let access = TOKEN_DUPLICATE | TOKEN_ADJUST_DEFAULT | TOKEN_QUERY | TOKEN_ASSIGN_PRIMARY;
    if unsafe { OpenProcessToken(GetCurrentProcess(), access, &mut own) } == 0 {
        return Err(last_error("opening this process's token"));
    }
    let own = Owned(own);
    let mut restricted: HANDLE = null_mut();
    // No SIDs disabled or restricted: a restricted version of our own token
    // can be given to a child without extra privileges.
    let made = unsafe { CreateRestrictedToken(own.0, 0, 0, null(), 0, null(), 0, null(), &mut restricted) };
    if made == 0 {
        return Err(last_error("copying this process's token"));
    }
    let token = Owned(restricted);
    let sid_text = wide(OsStr::new(LOW_INTEGRITY_SID));
    let mut sid: PSID = null_mut();
    if unsafe { ConvertStringSidToSidW(sid_text.as_ptr(), &mut sid) } == 0 {
        return Err(last_error("building the low integrity SID"));
    }
    let label = TOKEN_MANDATORY_LABEL { Label: SID_AND_ATTRIBUTES { Sid: sid, Attributes: SE_GROUP_INTEGRITY } };
    let size = std::mem::size_of::<TOKEN_MANDATORY_LABEL>() as u32 + unsafe { GetLengthSid(sid) };
    let set = unsafe {
        SetTokenInformation(token.0, TokenIntegrityLevel, (&label as *const TOKEN_MANDATORY_LABEL).cast(), size)
    };
    unsafe { LocalFree(sid) };
    if set == 0 {
        return Err(last_error("lowering the token's integrity level"));
    }
    Ok(token)
}

/// Quotes one argument the way Windows programs split their command line.
pub fn quote_arg(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
        return arg.to_string();
    }
    let mut out = String::from("\"");
    let mut slashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => slashes += 1,
            '"' => {
                out.extend(std::iter::repeat_n('\\', slashes * 2 + 1));
                out.push('"');
                slashes = 0;
            }
            _ => {
                out.extend(std::iter::repeat_n('\\', slashes));
                out.push(c);
                slashes = 0;
            }
        }
    }
    out.extend(std::iter::repeat_n('\\', slashes * 2));
    out.push('"');
    out
}

/// The program to start and its full command line. Batch files (npm's
/// `.cmd` shims, for example) run through `cmd.exe`.
fn command_line(cmd: &[String]) -> Result<(PathBuf, String), String> {
    let Some(prog) = cmd.first() else { return Err("no command given".into()) };
    let resolved = which::which(prog).map_err(|e| format!("cannot find {prog}: {e}"))?;
    let quoted: Vec<String> = cmd[1..].iter().map(|a| quote_arg(a)).collect();
    let is_batch = resolved
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"));
    if is_batch {
        let comspec =
            std::env::var_os("ComSpec").map(PathBuf::from).unwrap_or_else(|| r"C:\Windows\System32\cmd.exe".into());
        let inner = std::iter::once(quote_arg(&resolved.to_string_lossy())).chain(quoted).collect::<Vec<_>>().join(" ");
        return Ok((comspec, format!("cmd.exe /d /s /c \"{inner}\"")));
    }
    let line = std::iter::once(quote_arg(&resolved.to_string_lossy())).chain(quoted).collect::<Vec<_>>().join(" ");
    Ok((resolved, line))
}

/// Labels `writable` low, runs `cmd` at low integrity with this process's
/// standard handles and environment, and returns its exit code.
pub fn run(writable: &[PathBuf], cmd: &[String]) -> Result<i32, String> {
    for dir in writable {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        label_low(dir)?;
    }
    let token = low_token()?;
    let (app, line) = command_line(cmd)?;
    let app_w = wide(app.as_os_str());
    let mut line_w = wide(OsStr::new(&line));

    let mut si: STARTUPINFOW = unsafe { std::mem::zeroed() };
    si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    si.dwFlags = STARTF_USESTDHANDLES;
    unsafe {
        si.hStdInput = GetStdHandle(STD_INPUT_HANDLE);
        si.hStdOutput = GetStdHandle(STD_OUTPUT_HANDLE);
        si.hStdError = GetStdHandle(STD_ERROR_HANDLE);
        for h in [si.hStdInput, si.hStdOutput, si.hStdError] {
            if !h.is_null() {
                SetHandleInformation(h, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT);
            }
        }
    }
    let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    let created = unsafe {
        CreateProcessAsUserW(
            token.0,
            app_w.as_ptr(),
            line_w.as_mut_ptr(),
            null(),
            null(),
            1,
            CREATE_SUSPENDED,
            null(),
            null(),
            &si,
            &mut pi,
        )
    };
    if created == 0 {
        return Err(last_error(&format!("starting {}", app.display())));
    }
    let process = Owned(pi.hProcess);
    let thread = Owned(pi.hThread);
    // The job dies with this helper, and takes the command with it.
    let job = win32job::Job::create().map_err(|e| format!("creating a job object failed: {e}"))?;
    let mut info = job.query_extended_limit_info().map_err(|e| format!("job object: {e}"))?;
    info.limit_kill_on_job_close();
    job.set_extended_limit_info(&info).map_err(|e| format!("job object: {e}"))?;
    job.assign_process(process.0 as isize).map_err(|e| format!("job object: {e}"))?;
    unsafe { ResumeThread(thread.0) };
    unsafe { WaitForSingleObject(process.0, INFINITE) };
    let mut code: u32 = 1;
    if unsafe { GetExitCodeProcess(process.0, &mut code) } == 0 {
        return Err(last_error("reading the exit code"));
    }
    drop(job);
    Ok(code as i32)
}

#[cfg(test)]
mod tests {
    use super::quote_arg;

    #[test]
    fn arguments_are_quoted_for_windows() {
        assert_eq!(quote_arg("plain"), "plain");
        assert_eq!(quote_arg(""), "\"\"");
        assert_eq!(quote_arg("two words"), "\"two words\"");
        assert_eq!(quote_arg(r#"say "hi""#), r#""say \"hi\"""#);
        assert_eq!(quote_arg(r"C:\dir with space\"), r#""C:\dir with space\\""#);
    }
}
