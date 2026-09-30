// Cloning: a streaming, cancellable `git clone` plus the destination checks
// the clone dialog runs before it offers the Clone button.
use super::*;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::ipc::Channel;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneRequest {
    pub url: String,
    pub path: String,
    #[serde(default)]
    pub branch: Option<String>,
    /// `--depth N`; `None` or 0 is a full clone.
    #[serde(default)]
    pub depth: Option<u32>,
    #[serde(default)]
    pub recurse_submodules: bool,
    /// For forks: added as the `upstream` remote once the clone lands.
    #[serde(default)]
    pub upstream_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CloneProgress {
    /// e.g. "Receiving objects", "Resolving deltas", "Updating files".
    pub phase: String,
    pub percent: Option<u8>,
    /// The raw progress line, for the throughput / object counts.
    pub line: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CloneTarget {
    /// The path with a leading `~` expanded.
    pub path: String,
    pub exists: bool,
    pub is_dir: bool,
    pub is_empty: bool,
    pub is_repo: bool,
    pub origin_url: Option<String>,
}

const CANCELLED: &str = "Clone cancelled.";

#[tauri::command(async)]
pub fn clone_repository(
    state: State<'_, AppState>,
    request: CloneRequest,
    on_progress: Channel<CloneProgress>,
) -> Result<RepositoryState, String> {
    let cancel = Arc::new(AtomicBool::new(false));
    *state
        .clone_cancel
        .lock()
        .map_err(|_| "clone state lock is poisoned".to_string())? = Some(cancel.clone());

    let result = run_clone(&request, &cancel, |progress| {
        on_progress.send(progress).ok();
    });

    if let Ok(mut slot) = state.clone_cancel.lock() {
        *slot = None;
    }
    let target = result?;
    *state
        .repo_root
        .lock()
        .map_err(|_| "repository state lock is poisoned".to_string())? = discover_repo_root(&target)?;
    get_repository_state(state)
}

#[tauri::command(async)]
pub fn cancel_clone(state: State<'_, AppState>) -> Result<bool, String> {
    let slot = state
        .clone_cancel
        .lock()
        .map_err(|_| "clone state lock is poisoned".to_string())?;
    Ok(match slot.as_ref() {
        Some(flag) => {
            flag.store(true, Ordering::SeqCst);
            true
        }
        None => false,
    })
}

#[tauri::command(async)]
pub fn inspect_clone_target(path: String) -> Result<CloneTarget, String> {
    Ok(inspect_target(&expand_home(&path)))
}

/// Which of `paths` already hold a git checkout. Filesystem checks only (no
/// git process per path) so the whole repo list can be marked at once.
#[tauri::command(async)]
pub fn existing_checkouts(paths: Vec<String>) -> Result<Vec<String>, String> {
    Ok(paths
        .into_iter()
        .filter(|path| expand_home(path).join(".git").exists())
        .collect())
}

/// The clone itself, independent of Tauri so tests can drive it. Returns the
/// checkout's path on success.
pub(crate) fn run_clone(
    request: &CloneRequest,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(CloneProgress),
) -> Result<PathBuf, String> {
    let url = request.url.trim();
    validate_clone_operand(url, "repository url")?;
    let target = expand_home(request.path.trim());
    if request.path.trim().is_empty() {
        return Err("choose a folder to clone into".to_string());
    }
    let branch = request.branch.as_deref().map(str::trim).filter(|b| !b.is_empty());
    if let Some(branch) = branch {
        validate_clone_operand(branch, "branch")?;
    }
    let upstream = request
        .upstream_url
        .as_deref()
        .map(str::trim)
        .filter(|u| !u.is_empty());
    if let Some(upstream) = upstream {
        validate_clone_operand(upstream, "upstream url")?;
    }

    let before = inspect_target(&target);
    if before.exists && !(before.is_dir && before.is_empty) {
        return Err(if before.is_repo {
            format!("{} already contains a git repository.", before.path)
        } else {
            format!("{} already exists and is not an empty folder.", before.path)
        });
    }
    let parent = target
        .parent()
        .ok_or_else(|| "clone target must have a parent directory".to_string())?;
    fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    let target_arg = target
        .to_str()
        .ok_or_else(|| "clone target path is not valid UTF-8".to_string())?;

    let mut args = github_credential_args(url);
    args.extend(["clone", "--progress"].map(String::from));
    if let Some(branch) = branch {
        args.push("--branch".to_string());
        args.push(branch.to_string());
    }
    if let Some(depth) = request.depth.filter(|d| *d > 0) {
        args.push("--depth".to_string());
        args.push(depth.to_string());
    }
    if request.recurse_submodules {
        args.push("--recurse-submodules".to_string());
        if request.depth.is_some_and(|d| d > 0) {
            args.push("--shallow-submodules".to_string());
        }
    }
    // Separate options from operands so a URL beginning with `-` cannot be
    // parsed as a git option (e.g. `--upload-pack=...`).
    args.push("--".to_string());
    args.push(url.to_string());
    args.push(target_arg.to_string());

    let mut command = Command::new("git");
    command
        .args(&args)
        .current_dir(parent)
        // A GUI app has no terminal to answer a username/password prompt;
        // without this git would wait on one forever.
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Own process group, so a cancel reaches git-remote-https,
        // index-pack and the rest of the pipeline, not just `git`.
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(|err| format!("could not start git: {err}"))?;

    let stderr = child.stderr.take().expect("stderr is piped");
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let reader = std::thread::spawn(move || read_progress_lines(stderr, tx));

    let mut transcript: Vec<String> = Vec::new();
    let mut last_sent: Option<CloneProgress> = None;
    let mut cancelled = false;
    let mut term_sent_at: Option<Instant> = None;
    let status = loop {
        for line in rx.try_iter() {
            if let Some(progress) = parse_progress(&line) {
                if last_sent.as_ref() != Some(&progress) {
                    on_progress(progress.clone());
                    last_sent = Some(progress);
                }
            }
            transcript.push(line);
        }
        if cancel.load(Ordering::SeqCst) && !cancelled {
            cancelled = true;
            term_sent_at = Some(Instant::now());
            // SIGTERM lets git run its own cleanup of the half-written checkout.
            signal_group(&child, "-TERM");
        }
        if let Some(sent) = term_sent_at {
            if sent.elapsed() > Duration::from_secs(3) {
                signal_group(&child, "-KILL");
                child.kill().ok();
                term_sent_at = None;
            }
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(err) => return Err(err.to_string()),
        }
    };
    reader.join().ok();
    transcript.extend(rx.try_iter());

    if cancelled || !status.success() {
        remove_partial_clone(&target, &before);
        if cancelled {
            return Err(CANCELLED.to_string());
        }
        return Err(clone_failure_message(&transcript, status.code()));
    }

    if let Some(upstream) = upstream {
        // Best effort: a missing upstream remote should not fail a clone
        // that has already succeeded.
        run_git(&target, &["remote", "add", "upstream", "--", upstream]);
    }
    Ok(target)
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct RemoteBranches {
    pub default_branch: Option<String>,
    /// Default branch first, then the rest alphabetically.
    pub branches: Vec<String>,
}

/// The branches a URL offers, for the clone dialog's branch dropdown.
#[tauri::command(async)]
pub fn list_remote_branches(url: String) -> Result<RemoteBranches, String> {
    let url = url.trim();
    validate_clone_operand(url, "repository url")?;
    let mut args = github_credential_args(url);
    args.extend(["ls-remote", "--symref", "--", url, "HEAD", "refs/heads/*"].map(String::from));
    let result = run_command(
        Command::new("git")
            .args(&args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null()),
        false,
    );
    if !result.ok {
        return Err(clone_failure_message(
            &result.stderr.lines().map(str::to_string).collect::<Vec<_>>(),
            Some(result.code),
        ));
    }
    Ok(parse_ls_remote(&result.stdout))
}

pub(crate) fn parse_ls_remote(stdout: &str) -> RemoteBranches {
    let mut default_branch = None;
    let mut branches = Vec::new();
    for line in stdout.lines() {
        let Some((left, right)) = line.split_once('\t') else {
            continue;
        };
        if right == "HEAD" {
            if let Some(target) = left.strip_prefix("ref: refs/heads/") {
                default_branch = Some(target.to_string());
            }
        } else if let Some(name) = right.strip_prefix("refs/heads/") {
            branches.push(name.to_string());
        }
    }
    branches.sort_by_key(|name| name.to_lowercase());
    branches.dedup();
    if let Some(default) = default_branch.as_deref() {
        if let Some(index) = branches.iter().position(|name| name == default) {
            let name = branches.remove(index);
            branches.insert(0, name);
        }
    }
    RemoteBranches {
        default_branch,
        branches,
    }
}

/// Let `gh` answer HTTPS credential requests for github.com, as `gh repo
/// clone` does, so private repos work without `gh auth setup-git`. It is
/// appended after any configured helpers and scoped to this one command.
fn github_credential_args(url: &str) -> Vec<String> {
    match gh::resolve_gh() {
        Some(gh) if url.starts_with("https://github.com/") => vec![
            "-c".to_string(),
            format!(
                "credential.https://github.com.helper=!{} auth git-credential",
                shell_quote(&gh.display().to_string())
            ),
        ],
        _ => Vec::new(),
    }
}

/// Undo whatever a failed or cancelled clone left behind, but only what it
/// could have created: the folder if it did not exist, or the contents of a
/// folder that was empty.
fn remove_partial_clone(target: &Path, before: &CloneTarget) {
    if !before.exists {
        fs::remove_dir_all(target).ok();
    } else if before.is_dir && before.is_empty {
        if let Ok(entries) = fs::read_dir(target) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    fs::remove_dir_all(&path).ok();
                } else {
                    fs::remove_file(&path).ok();
                }
            }
        }
    }
}

#[cfg(unix)]
fn signal_group(child: &std::process::Child, signal: &str) {
    Command::new("kill")
        .args([signal, "--", &format!("-{}", child.id())])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok();
}

#[cfg(not(unix))]
fn signal_group(child: &std::process::Child, _signal: &str) {
    let _ = child;
}

/// Split git's stderr on both `\r` (in-place progress updates) and `\n`.
fn read_progress_lines(mut stderr: impl Read, tx: std::sync::mpsc::Sender<String>) {
    let mut buf = [0u8; 4096];
    let mut pending: Vec<u8> = Vec::new();
    loop {
        let read = match stderr.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        for &byte in &buf[..read] {
            if byte == b'\r' || byte == b'\n' {
                if !pending.is_empty() {
                    tx.send(String::from_utf8_lossy(&pending).into_owned()).ok();
                    pending.clear();
                }
            } else {
                pending.push(byte);
            }
        }
    }
    if !pending.is_empty() {
        tx.send(String::from_utf8_lossy(&pending).into_owned()).ok();
    }
}

/// Parse one line of `git clone --progress` output, e.g.
/// `Receiving objects:  45% (450/1000), 1.20 MiB | 2.30 MiB/s` or
/// `remote: Counting objects: 100% (12/12), done.`
pub(crate) fn parse_progress(line: &str) -> Option<CloneProgress> {
    let trimmed = line.trim();
    let body = trimmed.strip_prefix("remote:").map(str::trim).unwrap_or(trimmed);
    if body.starts_with("Cloning into") {
        return Some(CloneProgress {
            phase: "Connecting".to_string(),
            percent: None,
            line: trimmed.to_string(),
        });
    }
    let (phase, rest) = body.split_once(':')?;
    let phase = phase.trim();
    if phase.is_empty() || phase.len() > 40 || !phase.chars().all(|c| c.is_ascii_alphabetic() || c == ' ') {
        return None;
    }
    let percent = rest.split_once('%').and_then(|(before, _)| {
        before
            .trim()
            .rsplit(' ')
            .next()
            .and_then(|n| n.parse::<u8>().ok())
            .filter(|n| *n <= 100)
    });
    // "Enumerating objects: 1234, done." has no percentage but is a real
    // phase; anything else without one is not progress.
    if percent.is_none() && !phase.ends_with("objects") {
        return None;
    }
    Some(CloneProgress {
        phase: phase.to_string(),
        percent,
        line: trimmed.to_string(),
    })
}

/// Turn a failed clone's stderr into a short message: drop the progress
/// chatter and add a hint for the failures people actually hit.
pub(crate) fn clone_failure_message(transcript: &[String], code: Option<i32>) -> String {
    let lines: Vec<&str> = transcript
        .iter()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && parse_progress(line).is_none())
        .collect();
    let mut message = if lines.is_empty() {
        format!("git clone failed with exit code {}", code.unwrap_or(-1))
    } else {
        lines[lines.len().saturating_sub(12)..].join("\n")
    };
    let text = message.to_lowercase();
    let hint = if text.contains("repository not found") || text.contains("not found") && text.contains("remote:") {
        Some("The repository does not exist, or this account cannot see it.")
    } else if text.contains("permission denied (publickey)") {
        Some("SSH rejected your key. Add an SSH key to GitHub, or clone over HTTPS instead.")
    } else if text.contains("could not read username") || text.contains("authentication failed") {
        Some("Git needs credentials for this URL. Sign in with `gh auth login`, or clone over SSH.")
    } else if text.contains("remote branch") && text.contains("not found") {
        Some("That branch does not exist on the remote.")
    } else if text.contains("could not resolve host") {
        Some("Check your network connection.")
    } else {
        None
    };
    if let Some(hint) = hint {
        message.push_str("\n\n");
        message.push_str(hint);
    }
    message
}

pub(crate) fn inspect_target(path: &Path) -> CloneTarget {
    let metadata = fs::metadata(path).ok();
    let exists = metadata.is_some();
    let is_dir = metadata.as_ref().is_some_and(|m| m.is_dir());
    let is_empty = is_dir
        && fs::read_dir(path)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false);
    let is_repo = is_dir && path.join(".git").exists();
    let origin_url = if is_repo {
        let result = run_command(
            Command::new("git")
                .args(["config", "--get", "remote.origin.url"])
                .current_dir(path),
            false,
        );
        Some(result.stdout.trim().to_string()).filter(|url| result.ok && !url.is_empty())
    } else {
        None
    };
    CloneTarget {
        path: path.display().to_string(),
        exists,
        is_dir,
        is_empty,
        is_repo,
        origin_url,
    }
}

/// Expand a leading `~` so a clone directory setting like `~/dev` works.
pub(crate) fn expand_home(path: &str) -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    match (path.strip_prefix('~'), home) {
        (Some(""), Some(home)) => home,
        (Some(rest), Some(home)) if rest.starts_with('/') => home.join(rest.trim_start_matches('/')),
        _ => PathBuf::from(path),
    }
}

fn validate_clone_operand(value: &str, what: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err(format!("{what} must not be empty"));
    }
    if value.starts_with('-') {
        return Err(format!("{what} must not start with '-'"));
    }
    if value.chars().any(char::is_control) {
        return Err(format!("{what} must not contain control characters"));
    }
    Ok(())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;

    fn request(url: &str, path: &Path) -> CloneRequest {
        CloneRequest {
            url: url.to_string(),
            path: path.display().to_string(),
            branch: None,
            depth: None,
            recurse_submodules: false,
            upstream_url: None,
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "gitc-clone-{name}-{}-{}",
            std::process::id(),
            REPO_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        ));
        fs::remove_dir_all(&dir).ok();
        dir
    }

    #[test]
    fn parses_receiving_objects() {
        let progress = parse_progress("Receiving objects:  45% (450/1000), 1.20 MiB | 2.30 MiB/s").unwrap();
        assert_eq!(progress.phase, "Receiving objects");
        assert_eq!(progress.percent, Some(45));
    }

    #[test]
    fn parses_remote_phases_and_done_lines() {
        let progress = parse_progress("remote: Counting objects: 100% (12/12), done.").unwrap();
        assert_eq!(progress.phase, "Counting objects");
        assert_eq!(progress.percent, Some(100));
        let enumerating = parse_progress("remote: Enumerating objects: 1234, done.").unwrap();
        assert_eq!(enumerating.phase, "Enumerating objects");
        assert_eq!(enumerating.percent, None);
        assert_eq!(parse_progress("Cloning into 'gitc'...").unwrap().phase, "Connecting");
    }

    #[test]
    fn ignores_non_progress_lines() {
        assert_eq!(parse_progress("fatal: repository 'x' not found"), None);
        assert_eq!(parse_progress("remote: Repository not found."), None);
        assert_eq!(parse_progress("warning: You appear to have cloned an empty repository."), None);
    }

    #[test]
    fn failure_message_drops_progress_and_adds_a_hint() {
        let transcript = vec![
            "Cloning into 'x'...".to_string(),
            "remote: Repository not found.".to_string(),
            "fatal: repository 'https://github.com/a/x/' not found".to_string(),
        ];
        let message = clone_failure_message(&transcript, Some(128));
        assert!(!message.contains("Cloning into"));
        assert!(message.contains("fatal: repository"));
        assert!(message.contains("cannot see it"));
    }

    #[test]
    fn parses_ls_remote_with_default_first() {
        let out = "ref: refs/heads/main\tHEAD\nabc\tHEAD\nabc\trefs/heads/zeta\nabc\trefs/heads/main\nabc\trefs/heads/Alpha\n";
        let parsed = parse_ls_remote(out);
        assert_eq!(parsed.default_branch.as_deref(), Some("main"));
        assert_eq!(parsed.branches, vec!["main", "Alpha", "zeta"]);
    }

    #[test]
    fn lists_branches_of_a_local_remote() {
        let origin = TempRepo::new();
        write_file(origin.path(), "a.txt", "one\n");
        commit_all(origin.path(), "first");
        crate::test_support::run(origin.path(), &["branch", "feature"]);
        let listed = list_remote_branches(origin.path().display().to_string()).unwrap();
        assert_eq!(listed.default_branch.as_deref(), Some("main"));
        assert_eq!(listed.branches, vec!["main", "feature"]);
        assert!(list_remote_branches("--upload-pack=x".to_string()).is_err());
    }

    #[test]
    fn expands_a_leading_tilde_only() {
        let home = PathBuf::from(std::env::var("HOME").unwrap());
        assert_eq!(expand_home("~/dev/x"), home.join("dev/x"));
        assert_eq!(expand_home("~"), home);
        assert_eq!(expand_home("/a/~b"), PathBuf::from("/a/~b"));
        assert_eq!(expand_home("~other/x"), PathBuf::from("~other/x"));
    }

    #[test]
    fn rejects_option_like_operands() {
        let dir = scratch("opts");
        let cancel = AtomicBool::new(false);
        assert!(run_clone(&request("--upload-pack=touch /tmp/x", &dir), &cancel, |_| {}).is_err());
        let mut with_branch = request("/nonexistent", &dir);
        with_branch.branch = Some("--foo".to_string());
        assert!(run_clone(&with_branch, &cancel, |_| {}).is_err());
        assert!(!dir.exists());
    }

    #[test]
    fn clones_with_progress_options_and_upstream() {
        let origin = TempRepo::new();
        write_file(origin.path(), "a.txt", "one\n");
        commit_all(origin.path(), "first");
        crate::test_support::run(origin.path(), &["checkout", "-b", "feature"]);
        write_file(origin.path(), "b.txt", "two\n");
        commit_all(origin.path(), "second");
        crate::test_support::run(origin.path(), &["checkout", "main"]);

        let target = scratch("ok").join("nested/checkout");
        let mut req = request(&format!("file://{}", origin.path().display()), &target);
        req.branch = Some("feature".to_string());
        req.depth = Some(1);
        req.upstream_url = Some("https://example.invalid/upstream.git".to_string());
        let cancel = AtomicBool::new(false);
        let mut phases = Vec::new();
        let result = run_clone(&req, &cancel, |p| phases.push(p.phase));
        assert_eq!(result.as_deref(), Ok(target.as_path()));
        assert!(!phases.is_empty(), "progress was reported");
        assert!(target.join("b.txt").exists(), "cloned the requested branch");
        let depth = git(&target, &["rev-list", "--count", "HEAD"]).unwrap();
        assert_eq!(depth.trim(), "1");
        let upstream = git(&target, &["remote", "get-url", "upstream"]).unwrap();
        assert_eq!(upstream.trim(), "https://example.invalid/upstream.git");

        let inspected = inspect_target(&target);
        assert!(inspected.is_repo && !inspected.is_empty);
        assert!(inspected.origin_url.unwrap().starts_with("file://"));
        fs::remove_dir_all(target.parent().unwrap().parent().unwrap()).ok();
    }

    #[test]
    fn refuses_a_non_empty_destination_without_touching_it() {
        let origin = TempRepo::new();
        write_file(origin.path(), "a.txt", "one\n");
        commit_all(origin.path(), "first");
        let target = scratch("busy");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("keep.txt"), "mine").unwrap();
        let cancel = AtomicBool::new(false);
        let err = run_clone(&request(origin.path().to_str().unwrap(), &target), &cancel, |_| {}).unwrap_err();
        assert!(err.contains("not an empty folder"), "{err}");
        assert!(target.join("keep.txt").exists());
        fs::remove_dir_all(&target).ok();
    }

    #[test]
    fn a_failed_clone_leaves_no_folder_behind() {
        let target = scratch("fail");
        let cancel = AtomicBool::new(false);
        let err = run_clone(&request("/definitely/not/a/repo", &target), &cancel, |_| {}).unwrap_err();
        assert!(!err.is_empty());
        assert!(!target.exists());
    }

    #[test]
    fn a_cancelled_clone_reports_cancelled_and_cleans_up() {
        let origin = TempRepo::new();
        write_file(origin.path(), "a.txt", "one\n");
        commit_all(origin.path(), "first");
        let target = scratch("cancel");
        let cancel = AtomicBool::new(true);
        let result = run_clone(&request(origin.path().to_str().unwrap(), &target), &cancel, |_| {});
        // A tiny local clone can win the race against the signal; either
        // way nothing half-written may remain.
        match result {
            Err(err) => {
                assert_eq!(err, CANCELLED);
                assert!(!target.exists());
            }
            Ok(path) => {
                fs::remove_dir_all(path).ok();
            }
        }
    }
}
