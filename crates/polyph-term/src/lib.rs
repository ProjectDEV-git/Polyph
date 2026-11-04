//! Pseudo-terminals for shared terminals (`portable-pty`: Unix PTYs and
//! Windows ConPTY). A [`Term`] runs one program; its output arrives on a
//! channel in chunks, and input, resizes and the exit status go through the
//! handle. Blocking PTY reads and writes run on their own threads.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use tokio::sync::mpsc;

/// Largest chunk read from the PTY at once.
const READ_CHUNK: usize = 16 * 1024;

pub struct Term {
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
}

impl std::fmt::Debug for Term {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Term")
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Term {
    /// Starts `program args` in a new PTY of `cols` x `rows`. Output (and then
    /// `None` at end of output) arrives on the returned receiver.
    pub fn spawn(
        program: &str,
        args: &[String],
        cwd: &Path,
        env: &[(String, String)],
        cols: u16,
        rows: u16,
    ) -> anyhow::Result<(Arc<Term>, mpsc::UnboundedReceiver<Vec<u8>>)> {
        let pty = native_pty_system().openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })?;
        let mut cmd = CommandBuilder::new(program);
        cmd.args(args);
        cmd.cwd(cwd);
        for (k, v) in env {
            cmd.env(k, v);
        }
        if std::env::var_os("TERM").is_none() {
            cmd.env("TERM", "xterm-256color");
        }
        let child = pty.slave.spawn_command(cmd)?;
        drop(pty.slave);
        let mut reader = pty.master.try_clone_reader()?;
        let writer = pty.master.take_writer()?;
        let (tx, rx) = mpsc::unbounded_channel();
        std::thread::spawn(move || {
            let mut buf = vec![0u8; READ_CHUNK];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        Ok((
            Arc::new(Term { master: Mutex::new(pty.master), writer: Mutex::new(writer), child: Mutex::new(child) }),
            rx,
        ))
    }

    pub fn write(&self, data: &[u8]) -> std::io::Result<()> {
        let mut w = lock(&self.writer);
        w.write_all(data)?;
        w.flush()
    }

    pub fn resize(&self, cols: u16, rows: u16) -> anyhow::Result<()> {
        lock(&self.master).resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })?;
        Ok(())
    }

    pub fn kill(&self) {
        let _ = lock(&self.child).kill();
    }

    /// The exit code once the program has ended.
    pub fn try_exit(&self) -> Option<u32> {
        lock(&self.child).try_wait().ok().flatten().map(|s| s.exit_code())
    }
}

impl Drop for Term {
    fn drop(&mut self) {
        self.kill();
    }
}

/// Keeps the last `cap` bytes of output for viewers who arrive later.
#[derive(Debug, Clone)]
pub struct Scrollback {
    buf: std::collections::VecDeque<u8>,
    cap: usize,
}

impl Scrollback {
    pub fn new(cap: usize) -> Self {
        Self { buf: Default::default(), cap }
    }

    pub fn push(&mut self, data: &[u8]) {
        self.buf.extend(data);
        let over = self.buf.len().saturating_sub(self.cap);
        self.buf.drain(..over);
    }

    pub fn contents(&self) -> Vec<u8> {
        self.buf.iter().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    #[cfg(unix)]
    use std::time::Duration;

    // Only the Unix tests below use a real shell.
    #[cfg(unix)]
    async fn read_until(rx: &mut mpsc::UnboundedReceiver<Vec<u8>>, want: &str) -> String {
        let mut got = String::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while !got.contains(want) {
            match tokio::time::timeout_at(deadline, rx.recv()).await {
                Ok(Some(b)) => got.push_str(&String::from_utf8_lossy(&b)),
                _ => panic!("no {want:?} in {got:?}"),
            }
        }
        got
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runs_a_shell_with_input_and_resize() {
        let dir = std::env::temp_dir();
        let (t, mut rx) = Term::spawn("sh", &[], &dir, &[("PS1".into(), "$ ".into())], 80, 24).unwrap();
        t.write(b"echo hel''lo\n").unwrap();
        read_until(&mut rx, "hello").await;
        t.resize(100, 30).unwrap();
        t.write(b"stty size\n").unwrap();
        read_until(&mut rx, "30 100").await;
        t.write(b"exit 3\n").unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while t.try_exit().is_none() {
            assert!(tokio::time::Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(t.try_exit(), Some(3));
    }

    #[test]
    fn scrollback_keeps_the_tail() {
        let mut s = Scrollback::new(5);
        s.push(b"abc");
        s.push(b"defg");
        assert_eq!(s.contents(), b"cdefg");
    }
}
