//! Sandbox acceptance: on each platform, every available mechanism must
//! block writes outside the worktree and network access except through the
//! proxy. Mechanisms the machine cannot run are skipped with a message.
#![allow(clippy::unwrap_used)]

use polyph_sandbox::Mechanism;

#[tokio::test]
async fn available_mechanisms_block_escapes() {
    let helper = vec![env!("CARGO_BIN_EXE_polyph-sandbox-exec").to_string()];
    let reports = polyph_sandbox::selftest::run_all(helper).await.unwrap();
    let mut protected = 0;
    for r in &reports {
        if !r.available {
            eprintln!("skipped {:?}: {}", r.mechanism, r.detail);
            continue;
        }
        if r.mechanism == Mechanism::Container {
            eprintln!("skipped container mode: needs an image");
            continue;
        }
        if r.mechanism == Mechanism::WindowsJob {
            eprintln!("Windows job objects do not isolate files or network; see docs/sandbox.md");
            continue;
        }
        for c in &r.checks {
            assert!(c.passed, "{:?}: {} failed", r.mechanism, c.key);
        }
        protected += 1;
    }
    if protected == 0 {
        eprintln!("no sandbox mechanism could run on this machine; nothing was asserted");
    }
}
