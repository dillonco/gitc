// Shared helpers for driving the real git CLI in throwaway repositories.
// Extracted from `git_integration_tests` so feature-stream modules can write
// their own integration tests without touching `lib.rs`.
#![allow(dead_code)]
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) static REPO_COUNTER: AtomicUsize = AtomicUsize::new(0);

pub(crate) struct TempRepo(PathBuf);

impl TempRepo {
    pub(crate) fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "gitc-test-{}-{}",
            std::process::id(),
            REPO_COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&dir).expect("create temp repo dir");
        run(&dir, &["init", "-b", "main"]);
        run(&dir, &["config", "user.email", "test@gitc.dev"]);
        run(&dir, &["config", "user.name", "Test User"]);
        run(&dir, &["config", "commit.gpgsign", "false"]);
        TempRepo(dir)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}

pub(crate) fn run(root: &Path, args: &[&str]) -> GitResult {
    let result = run_git(root, args);
    assert!(result.ok, "git {args:?} failed: {}", result.stderr);
    result
}

pub(crate) fn write_file(root: &Path, name: &str, content: &str) {
    fs::write(root.join(name), content).expect("write file");
}

pub(crate) fn commit_all(root: &Path, message: &str) {
    run(root, &["add", "-A"]);
    run(root, &["commit", "-m", message]);
}

// ---- helpers for driving the real action dispatcher ----

pub(crate) fn act(kind: &str) -> GitAction {
    GitAction {
        kind: kind.to_string(),
        path: None,
        message: None,
        branch: None,
        target: None,
        remote: None,
        mode: None,
    }
}

pub(crate) fn ok_action(root: &Path, action: &GitAction) -> GitResult {
    let result = run_action(root, action).expect("action_args must succeed");
    assert!(
        result.ok,
        "action {:?} failed: {}",
        action.kind, result.stderr
    );
    result
}

pub(crate) fn head_subject(root: &Path) -> String {
    git(root, &["log", "-1", "--format=%s"]).unwrap()
}
