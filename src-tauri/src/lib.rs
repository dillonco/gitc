use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use tauri::State;

// ---- Feature modules (one per work stream; keep alphabetical) ----
mod cleanup;
mod compare;
mod gh;
mod rebase;
#[cfg(test)]
mod test_support;

struct AppState {
    repo_root: Mutex<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStatus {
    path: String,
    index: String,
    worktree: String,
    group: FileGroup,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FileGroup {
    Staged,
    Unstaged,
    Untracked,
    Conflicted,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Branch {
    name: String,
    current: bool,
    upstream: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StashEntry {
    name: String,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    path: String,
    head: String,
    branch: Option<String>,
    detached: bool,
    bare: bool,
    current: bool,
    main: bool,
    locked: bool,
    lock_reason: Option<String>,
    prunable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryState {
    root: String,
    current_branch: Option<String>,
    head: String,
    merging: bool,
    rebasing: bool,
    files: Vec<FileStatus>,
    branches: Vec<Branch>,
    remotes: Vec<String>,
    remote_branches: Vec<String>,
    tags: Vec<String>,
    worktrees: Vec<Worktree>,
    stashes: Vec<StashEntry>,
    user_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitNode {
    hash: String,
    short_hash: String,
    parents: Vec<String>,
    refs: Vec<String>,
    author: String,
    relative_date: String,
    subject: String,
    body_summary: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitGraph {
    commits: Vec<CommitNode>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitFileChange {
    status: String,
    path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitDetail {
    hash: String,
    short_hash: String,
    parents: Vec<String>,
    refs: Vec<String>,
    author: String,
    email: String,
    date: String,
    relative_date: String,
    subject: String,
    body: String,
    files: Vec<CommitFileChange>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitAction {
    kind: String,
    path: Option<String>,
    message: Option<String>,
    branch: Option<String>,
    target: Option<String>,
    remote: Option<String>,
    mode: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitResult {
    ok: bool,
    stdout: String,
    stderr: String,
    code: i32,
    refresh: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictFile {
    path: String,
    base: Option<String>,
    ours: Option<String>,
    theirs: Option<String>,
    working: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    path: String,
    staged: bool,
    diff: String,
    binary: bool,
}

#[tauri::command]
fn get_repository_state(state: State<'_, AppState>) -> Result<RepositoryState, String> {
    let root = active_repo(&state)?;
    repository_state(&root)
}

fn repository_state(root: &Path) -> Result<RepositoryState, String> {
    let current_branch = git_optional(root, &["branch", "--show-current"])?;
    let head = git_optional(root, &["rev-parse", "--short", "HEAD"])?
        .unwrap_or_else(|| "unborn".to_string());
    let git_dir = root.join(".git");

    Ok(RepositoryState {
        root: root.display().to_string(),
        current_branch,
        head,
        merging: git_dir.join("MERGE_HEAD").exists(),
        rebasing: git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists(),
        files: parse_status(&git(&root, &["status", "--porcelain=v2"])?),
        branches: parse_branches(&git(
            &root,
            &[
                "branch",
                "--format=%(HEAD)%09%(refname:short)%09%(upstream:short)",
            ],
        )?),
        remotes: git(&root, &["remote"])?
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(ToOwned::to_owned)
            .collect(),
        remote_branches: git(&root, &["branch", "-r", "--format=%(refname:short)"])?
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.ends_with("/HEAD"))
            .map(ToOwned::to_owned)
            .collect(),
        tags: git(&root, &["tag", "--sort=-creatordate"])?
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect(),
        worktrees: parse_worktrees(&git(&root, &["worktree", "list", "--porcelain"])?, root),
        stashes: parse_stashes(&git(&root, &["stash", "list"])?),
        user_name: git_optional(&root, &["config", "user.name"])?,
    })
}

#[tauri::command]
fn get_commit_graph(state: State<'_, AppState>, limit: usize) -> Result<CommitGraph, String> {
    let root = active_repo(&state)?;
    commit_graph(&root, limit)
}

fn commit_graph(root: &Path, limit: usize) -> Result<CommitGraph, String> {
    let limit = limit.clamp(25, 1000).to_string();
    // Exclude refs/stash: its synthetic two/three-parent commits render as bogus merges.
    let out = git(
        root,
        &[
            "log",
            "--exclude=refs/stash",
            "--all",
            "--topo-order",
            "--date=relative",
            "--format=%H%x1f%P%x1f%D%x1f%an%x1f%ar%x1f%s%x1f%b%x1e",
            "-n",
            &limit,
        ],
    )?;

    Ok(CommitGraph {
        commits: parse_commit_graph(&out),
    })
}

#[tauri::command]
fn get_commit_detail(state: State<'_, AppState>, hash: String) -> Result<CommitDetail, String> {
    let root = active_repo(&state)?;
    commit_detail(&root, &hash)
}

fn commit_detail(root: &Path, hash: &str) -> Result<CommitDetail, String> {
    validate_ref_arg(hash)?;
    let meta = git(
        root,
        &[
            "show",
            "--no-patch",
            "--date=format:%Y-%m-%d %H:%M",
            "--format=%H%x1f%P%x1f%D%x1f%an%x1f%ae%x1f%ad%x1f%ar%x1f%s%x1f%b",
            hash,
        ],
    )?;
    let fields: Vec<&str> = meta.split('\u{1f}').collect();
    if fields.len() != 9 {
        return Err("unable to parse commit metadata".to_string());
    }

    let parents: Vec<String> = fields[1]
        .split_whitespace()
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect();

    // `git show --name-status` prints nothing for merge commits, so diff
    // against the first parent explicitly; root commits keep `git show`.
    let name_status = if let Some(parent) = parents.first() {
        git(root, &["diff", "--name-status", parent, hash])?
    } else {
        git(root, &["show", "--name-status", "--format=", hash])?
    };

    Ok(CommitDetail {
        hash: fields[0].to_string(),
        short_hash: fields[0].chars().take(8).collect(),
        parents,
        refs: fields[2]
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect(),
        author: fields[3].to_string(),
        email: fields[4].to_string(),
        date: fields[5].to_string(),
        relative_date: fields[6].to_string(),
        subject: fields[7].to_string(),
        body: fields[8].trim().to_string(),
        files: parse_name_status(&name_status),
    })
}

#[tauri::command]
fn get_commit_file_diff(
    state: State<'_, AppState>,
    hash: String,
    path: String,
) -> Result<FileDiff, String> {
    let root = active_repo(&state)?;
    commit_file_diff(&root, &hash, path)
}

fn commit_file_diff(root: &Path, hash: &str, path: String) -> Result<FileDiff, String> {
    validate_ref_arg(hash)?;
    validate_repo_relative_path(&path)?;

    // Diff against the first parent so merge commits show real hunks;
    // `git show` alone suppresses merge diffs. Root commits have no parent.
    let parent = git_optional(root, &["rev-parse", &format!("{hash}^")])?;
    let diff = if let Some(parent) = parent {
        git_optional(root, &["diff", &parent, hash, "--", &path])?.unwrap_or_default()
    } else {
        git_optional(root, &["show", "--format=", hash, "--", &path])?.unwrap_or_default()
    };

    Ok(FileDiff {
        path,
        staged: false,
        binary: diff.contains("Binary files") || diff.contains("GIT binary patch"),
        diff,
    })
}

#[tauri::command]
fn run_git_action(state: State<'_, AppState>, action: GitAction) -> Result<GitResult, String> {
    let root = active_repo(&state)?;
    run_action(&root, &action)
}

fn run_action(root: &Path, action: &GitAction) -> Result<GitResult, String> {
    if matches!(action.kind.as_str(), "worktreeRemove" | "worktreeRemoveForce") {
        let target = Path::new(action.path.as_deref().unwrap_or_default());
        if canonical_or_self(root) == canonical_or_self(target) {
            return Err(
                "cannot remove the worktree that is currently open; switch to another worktree first"
                    .to_string(),
            );
        }
    }
    let args = action_args(action)?;
    Ok(run_git(root, &args))
}

fn canonical_or_self(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

#[tauri::command]
fn get_conflict_file(state: State<'_, AppState>, path: String) -> Result<ConflictFile, String> {
    let root = active_repo(&state)?;
    conflict_file(&root, path)
}

fn conflict_file(root: &Path, path: String) -> Result<ConflictFile, String> {
    validate_repo_relative_path(&path)?;
    let working = fs::read_to_string(root.join(&path)).map_err(|err| err.to_string())?;

    Ok(ConflictFile {
        path: path.clone(),
        base: git_optional(root, &["show", &format!(":1:{path}")])?,
        ours: git_optional(root, &["show", &format!(":2:{path}")])?,
        theirs: git_optional(root, &["show", &format!(":3:{path}")])?,
        working,
    })
}

#[tauri::command]
fn get_file_diff(state: State<'_, AppState>, path: String, staged: bool) -> Result<FileDiff, String> {
    let root = active_repo(&state)?;
    file_diff(&root, path, staged)
}

fn file_diff(root: &Path, path: String, staged: bool) -> Result<FileDiff, String> {
    validate_repo_relative_path(&path)?;
    let diff = if staged {
        git_optional(root, &["diff", "--cached", "--", &path])?.unwrap_or_default()
    } else {
        git_optional(root, &["diff", "--", &path])?.unwrap_or_default()
    };

    let diff = if diff.is_empty() && !staged && root.join(&path).is_file() {
        render_untracked_file_diff(root, &path)?
    } else {
        diff
    };

    Ok(FileDiff {
        path,
        staged,
        binary: diff.contains("Binary files") || diff.contains("GIT binary patch"),
        diff,
    })
}

#[tauri::command]
fn get_file_content(state: State<'_, AppState>, path: String, staged: bool) -> Result<String, String> {
    let root = active_repo(&state)?;
    file_content(&root, path, staged)
}

fn file_content(root: &Path, path: String, staged: bool) -> Result<String, String> {
    validate_repo_relative_path(&path)?;
    if staged {
        git_optional(root, &["show", &format!(":{path}")])
            .map(|content| content.unwrap_or_default())
    } else {
        fs::read_to_string(root.join(path)).map_err(|err| err.to_string())
    }
}

#[tauri::command]
fn get_file_blame(state: State<'_, AppState>, path: String) -> Result<String, String> {
    let root = active_repo(&state)?;
    file_blame(&root, &path)
}

fn file_blame(root: &Path, path: &str) -> Result<String, String> {
    validate_repo_relative_path(path)?;
    git(root, &["blame", "--date=short", "--", path])
}

#[tauri::command]
fn get_file_history(state: State<'_, AppState>, path: String) -> Result<String, String> {
    let root = active_repo(&state)?;
    file_history(&root, &path)
}

fn file_history(root: &Path, path: &str) -> Result<String, String> {
    validate_repo_relative_path(path)?;
    git(
        root,
        &[
            "log",
            "--date=relative",
            "--format=%h  %ar  %an  %s",
            "--",
            path,
        ],
    )
}

#[tauri::command]
fn apply_hunk(state: State<'_, AppState>, patch: String, mode: String) -> Result<GitResult, String> {
    let root = active_repo(&state)?;
    apply_hunk_patch(&root, &patch, &mode)
}

fn hunk_args(mode: &str) -> Result<Vec<&'static str>, String> {
    match mode {
        "stage" => Ok(vec!["apply", "--cached", "-"]),
        "unstage" => Ok(vec!["apply", "--cached", "--reverse", "-"]),
        "discard" => Ok(vec!["apply", "--reverse", "-"]),
        other => Err(format!("unsupported hunk mode: {other}")),
    }
}

fn apply_hunk_patch(root: &Path, patch: &str, mode: &str) -> Result<GitResult, String> {
    let args = hunk_args(mode)?;
    Ok(run_git_with_stdin(root, &args, patch))
}

#[tauri::command]
fn set_repository_path(state: State<'_, AppState>, path: String) -> Result<RepositoryState, String> {
    let root = discover_repo_root(Path::new(&path))?;
    *state
        .repo_root
        .lock()
        .map_err(|_| "repository state lock is poisoned".to_string())? = root;
    get_repository_state(state)
}

#[tauri::command]
fn open_terminal(state: State<'_, AppState>) -> Result<GitResult, String> {
    let root = active_repo(&state)?;
    Ok(run_command(
        Command::new("open")
            .args(["-a", "Terminal"])
            .arg(&root)
            .current_dir(&root),
        true,
    ))
}

#[tauri::command]
fn pick_repository_folder() -> Result<Option<String>, String> {
    let result = run_command(
        Command::new("osascript").args([
            "-e",
            "try\nset chosenFolder to choose folder with prompt \"Select a repository folder\"\nreturn POSIX path of chosenFolder\non error number -128\nreturn \"\"\nend try",
        ]),
        false,
    );

    if result.ok {
        let path = result.stdout.trim();
        Ok((!path.is_empty()).then_some(path.to_string()))
    } else if result.stderr.contains("User canceled.") || result.code == -128 {
        Ok(None)
    } else {
        Err(if result.stderr.trim().is_empty() {
            "unable to open folder picker".to_string()
        } else {
            result.stderr
        })
    }
}

#[tauri::command]
fn create_repository(state: State<'_, AppState>, path: String) -> Result<RepositoryState, String> {
    let root = PathBuf::from(path);
    fs::create_dir_all(&root).map_err(|err| err.to_string())?;
    let result = run_git(&root, &["init"]);
    if !result.ok {
        return Err(result.stderr);
    }
    *state
        .repo_root
        .lock()
        .map_err(|_| "repository state lock is poisoned".to_string())? = discover_repo_root(&root)?;
    get_repository_state(state)
}

#[tauri::command]
fn clone_repository(
    state: State<'_, AppState>,
    url: String,
    path: String,
) -> Result<RepositoryState, String> {
    let target = PathBuf::from(path);
    let parent = target
        .parent()
        .ok_or_else(|| "clone target must have a parent directory".to_string())?;
    fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    let target_arg = target
        .to_str()
        .ok_or_else(|| "clone target path is not valid UTF-8".to_string())?;
    // Separate options from operands so a URL beginning with `-` cannot be
    // parsed as a git option (e.g. `--upload-pack=...`).
    let result = run_git(parent, &["clone", "--", &url, target_arg]);
    if !result.ok {
        return Err(result.stderr);
    }
    *state
        .repo_root
        .lock()
        .map_err(|_| "repository state lock is poisoned".to_string())? = discover_repo_root(&target)?;
    get_repository_state(state)
}

#[tauri::command]
fn save_conflict_resolution(
    state: State<'_, AppState>,
    path: String,
    content: String,
) -> Result<GitResult, String> {
    let root = active_repo(&state)?;
    validate_repo_relative_path(&path)?;
    fs::write(root.join(path), content).map_err(|err| err.to_string())?;
    Ok(GitResult {
        ok: true,
        stdout: String::new(),
        stderr: String::new(),
        code: 0,
        refresh: true,
    })
}

pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            repo_root: Mutex::new(default_repo_root()),
        })
        .invoke_handler(tauri::generate_handler![
            get_repository_state,
            get_commit_graph,
            get_commit_detail,
            get_commit_file_diff,
            run_git_action,
            set_repository_path,
            open_terminal,
            pick_repository_folder,
            create_repository,
            clone_repository,
            apply_hunk,
            get_file_diff,
            get_file_content,
            get_file_blame,
            get_file_history,
            get_conflict_file,
            save_conflict_resolution,
            // ---- stream commands (stubs land in Stage 0; bodies per stream) ----
            cleanup::get_branch_cleanup,
            compare::get_ref_compare,
            compare::get_ref_file_diff,
            gh::gh_status,
            gh::gh_repo_list,
            rebase::get_rebase_plan,
            rebase::run_interactive_rebase,
        ])
        .run(tauri::generate_context!())
        .expect("error while running gitc");
}

fn default_repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri must live inside the repository")
        .to_path_buf()
}

fn active_repo(state: &State<'_, AppState>) -> Result<PathBuf, String> {
    state
        .repo_root
        .lock()
        .map(|root| root.clone())
        .map_err(|_| "repository state lock is poisoned".to_string())
}

fn discover_repo_root(path: &Path) -> Result<PathBuf, String> {
    let result = run_git(path, &["rev-parse", "--show-toplevel"]);
    if result.ok {
        Ok(PathBuf::from(result.stdout.trim()))
    } else {
        Err(result.stderr.trim().to_string())
    }
}

/// The branch feature work is measured against: origin/HEAD's target if the
/// remote advertises one, else a local `main`/`master`, else the current branch.
/// Unused outside tests until the F1/F2/F4 streams call it; see PLAN.md.
#[allow(dead_code)]
fn default_base_branch(root: &Path) -> Result<String, String> {
    if let Some(head) = git_optional(root, &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"])? {
        if let Some(local) = head.strip_prefix("origin/") {
            if git_optional(root, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{local}")])?.is_some() {
                return Ok(local.to_string());
            }
            return Ok(head);
        }
    }
    for candidate in ["main", "master"] {
        if git_optional(root, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{candidate}")])?.is_some() {
            return Ok(candidate.to_string());
        }
    }
    git_optional(root, &["branch", "--show-current"])?
        .ok_or_else(|| "unable to determine a base branch".to_string())
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let result = run_git(root, args);
    if result.ok {
        Ok(result.stdout.trim_end().to_string())
    } else {
        Err(if result.stderr.trim().is_empty() {
            format!("git {:?} failed with exit code {}", args, result.code)
        } else {
            result.stderr
        })
    }
}

fn git_optional(root: &Path, args: &[&str]) -> Result<Option<String>, String> {
    let result = run_git(root, args);
    if result.ok {
        let value = result.stdout.trim_end().to_string();
        Ok((!value.is_empty()).then_some(value))
    } else {
        Ok(None)
    }
}

fn run_git(root: &Path, args: &[&str]) -> GitResult {
    run_git_env(root, args, &[])
}

/// Like `run_git`, but with extra environment variables for this one call.
/// Used by interactive rebase (GIT_SEQUENCE_EDITOR / GIT_EDITOR).
fn run_git_env(root: &Path, args: &[&str], env: &[(&str, &str)]) -> GitResult {
    let mut command = Command::new("git");
    command.args(args).current_dir(root);
    for (key, value) in env {
        command.env(key, value);
    }
    run_command(&mut command, true)
}

fn run_command(command: &mut Command, refresh: bool) -> GitResult {
    match command.output() {
        Ok(output) => result_from_output(output, refresh),
        Err(err) => command_error(err),
    }
}

fn run_git_with_stdin(root: &Path, args: &[&str], stdin: &str) -> GitResult {
    let spawn = Command::new("git")
        .args(args)
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let mut child = match spawn {
        Ok(child) => child,
        Err(err) => return command_error(err),
    };

    if let Some(mut child_stdin) = child.stdin.take() {
        if let Err(err) = child_stdin.write_all(stdin.as_bytes()) {
            return command_error(err);
        }
    }

    match child.wait_with_output() {
        Ok(output) => result_from_output(output, true),
        Err(err) => command_error(err),
    }
}

fn result_from_output(output: std::process::Output, refresh: bool) -> GitResult {
    GitResult {
        ok: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        code: output.status.code().unwrap_or(-1),
        refresh,
    }
}

fn command_error(err: impl std::fmt::Display) -> GitResult {
    GitResult {
        ok: false,
        stdout: String::new(),
        stderr: err.to_string(),
        code: -1,
        refresh: false,
    }
}

fn action_args(action: &GitAction) -> Result<Vec<&str>, String> {
    let path = action.path.as_deref();
    let branch = action.branch.as_deref();
    let target = action.target.as_deref();
    let remote = action.remote.as_deref().unwrap_or("origin");
    let message = action.message.as_deref();

    let args = match action.kind.as_str() {
        "stage" => vec!["add", "--", required(path, "path")?],
        "unstage" => vec!["restore", "--staged", "--", required(path, "path")?],
        "discard" => vec!["restore", "--", required(path, "path")?],
        "cleanUntracked" => vec!["clean", "-f", "--", required(path, "path")?],
        "commit" => vec!["commit", "-m", required(message, "message")?],
        "commitAmend" => vec!["commit", "--amend", "-m", required(message, "message")?],
        "checkoutBranch" => vec!["checkout", required(branch, "branch")?],
        "createBranch" => {
            let mut args = vec!["checkout", "-b", required(branch, "branch")?];
            if let Some(target) = target.filter(|value| !value.trim().is_empty()) {
                args.push(target);
            }
            args
        }
        "deleteBranch" => vec!["branch", "-d", required(branch, "branch")?],
        "deleteBranchForce" => vec!["branch", "-D", required(branch, "branch")?],
        "checkoutRemote" => vec!["checkout", "--track", required(target, "target")?],
        "checkoutCommit" => vec!["checkout", "--detach", required(target, "target")?],
        "createTag" => {
            let mut args = vec!["tag", required(branch, "branch")?];
            if let Some(target) = target.filter(|value| !value.trim().is_empty()) {
                args.push(target);
            }
            args
        }
        "deleteTag" => vec!["tag", "-d", required(branch, "branch")?],
        "fetch" => vec!["fetch", remote],
        "fetchAll" => vec!["fetch", "--all", "--prune"],
        "pull" => vec!["pull", "--ff-only", remote],
        "push" => vec!["push", "-u", remote, "HEAD"],
        "forcePush" => vec!["push", "--force-with-lease", remote, "HEAD"],
        "stashCreate" => vec!["stash", "push", "-u", "-m", message.unwrap_or("gitc stash")],
        "stashApply" => vec!["stash", "apply", required(target, "target")?],
        "stashPop" => vec!["stash", "pop", required(target, "target")?],
        "stashDrop" => vec!["stash", "drop", required(target, "target")?],
        "merge" => vec!["merge", required(target, "target")?],
        "rebase" => vec!["rebase", required(target, "target")?],
        // No terminal is attached, so a plain `merge --continue` would block
        // on (or, non-interactively, refuse for) an editor to confirm the
        // merge commit message; accept the default message instead.
        "mergeContinue" => vec!["-c", "core.editor=true", "merge", "--continue"],
        "mergeAbort" => vec!["merge", "--abort"],
        "rebaseContinue" => vec!["rebase", "--continue"],
        "rebaseAbort" => vec!["rebase", "--abort"],
        "cherryPick" => vec!["cherry-pick", required(target, "target")?],
        "revert" => vec!["revert", "--no-edit", required(target, "target")?],
        "reset" => vec![
            "reset",
            reset_flag(action.mode.as_deref())?,
            required(target, "target")?,
        ],
        "markResolved" => vec!["add", "--", required(path, "path")?],
        "worktreeAdd" => match action.mode.as_deref().unwrap_or("checkout") {
            // Check out an existing branch into a new worktree directory.
            "checkout" => vec![
                "worktree",
                "add",
                required(path, "path")?,
                required(branch, "branch")?,
            ],
            // Create a new branch (optionally from a base ref) in the new worktree.
            "new" => {
                let mut args = vec![
                    "worktree",
                    "add",
                    "-b",
                    required(branch, "branch")?,
                    required(path, "path")?,
                ];
                if let Some(target) = target.filter(|value| !value.trim().is_empty()) {
                    args.push(target);
                }
                args
            }
            "detach" => vec![
                "worktree",
                "add",
                "--detach",
                required(path, "path")?,
                required(target, "target")?,
            ],
            other => return Err(format!("unsupported worktree add mode: {other}")),
        },
        "worktreeRemove" => vec!["worktree", "remove", required(path, "path")?],
        "worktreeRemoveForce" => {
            vec!["worktree", "remove", "--force", required(path, "path")?]
        }
        "worktreePrune" => vec!["worktree", "prune", "-v"],
        unknown => return Err(format!("unknown git action: {unknown}")),
    };

    if let Some(path) = path {
        // Worktree directories live outside the repository, so they get their
        // own validation instead of the repo-relative check.
        if matches!(
            action.kind.as_str(),
            "worktreeAdd" | "worktreeRemove" | "worktreeRemoveForce"
        ) {
            validate_worktree_path(path)?;
        } else {
            validate_repo_relative_path(path)?;
        }
    }
    for value in [branch, target, action.remote.as_deref()].into_iter().flatten() {
        validate_ref_arg(value)?;
    }
    Ok(args)
}

fn required<'a>(value: Option<&'a str>, name: &str) -> Result<&'a str, String> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{name} is required"))
}

fn reset_flag(mode: Option<&str>) -> Result<&'static str, String> {
    match mode.unwrap_or("mixed") {
        "soft" => Ok("--soft"),
        "mixed" => Ok("--mixed"),
        "hard" => Ok("--hard"),
        other => Err(format!("unsupported reset mode: {other}")),
    }
}

fn validate_ref_arg(value: &str) -> Result<(), String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.starts_with('-') {
        return Err(format!("invalid ref argument: {value:?}"));
    }
    Ok(())
}

fn parse_name_status(out: &str) -> Vec<CommitFileChange> {
    out.lines()
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let status = parts.next()?.trim();
            if status.is_empty() {
                return None;
            }
            // Renames/copies list "R100\told\tnew" — show the new path.
            let path = parts.next_back()?.trim();
            if path.is_empty() {
                return None;
            }
            Some(CommitFileChange {
                status: status.chars().take(1).collect(),
                path: path.to_string(),
            })
        })
        .collect()
}

fn validate_worktree_path(path: &str) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed.starts_with('-') {
        return Err(format!("invalid worktree path: {path:?}"));
    }
    if !Path::new(trimmed).is_absolute() {
        return Err("worktree path must be absolute".to_string());
    }
    Ok(())
}

fn validate_repo_relative_path(path: &str) -> Result<(), String> {
    let candidate = Path::new(path);
    if candidate.is_absolute()
        || candidate
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::RootDir | Component::Prefix(_)))
    {
        return Err("path must stay inside the repository".to_string());
    }
    Ok(())
}

fn render_untracked_file_diff(root: &Path, path: &str) -> Result<String, String> {
    let content = fs::read_to_string(root.join(path))
        .map_err(|_| "No text diff available for this file".to_string())?;
    let mut diff = format!("diff --git a/{path} b/{path}\nnew file mode 100644\n--- /dev/null\n+++ b/{path}\n");
    for line in content.lines() {
        diff.push('+');
        diff.push_str(line);
        diff.push('\n');
    }
    if content.ends_with('\n') {
        return Ok(diff);
    }
    diff.push_str("\\ No newline at end of file\n");
    Ok(diff)
}

fn parse_status(out: &str) -> Vec<FileStatus> {
    out.lines()
        .flat_map(|line| {
            if let Some(path) = line.strip_prefix("? ") {
                return vec![FileStatus {
                    path: path.to_string(),
                    index: "?".to_string(),
                    worktree: "?".to_string(),
                    group: FileGroup::Untracked,
                }];
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            match parts.first().copied() {
                Some("1") | Some("2") if parts.len() >= 9 => {
                    let xy = parts[1];
                    // "1" lines: path starts at field 8. "2" (rename/copy) lines carry an
                    // extra score field ("R100") at 8, then "newpath<TAB>oldpath" — show
                    // the new path only.
                    let path = if parts[0] == "2" {
                        line.split('\t')
                            .next()
                            .and_then(|head| {
                                let fields: Vec<&str> = head.split_whitespace().collect();
                                (fields.len() >= 10).then(|| fields[9..].join(" "))
                            })
                            .unwrap_or_else(|| parts[8..].join(" "))
                    } else {
                        parts[8..].join(" ")
                    };
                    let index = xy.chars().next().unwrap_or('.');
                    let worktree = xy.chars().nth(1).unwrap_or('.');
                    let mut files = Vec::new();

                    if index != '.' {
                        files.push(FileStatus {
                            path: path.clone(),
                            index: index.to_string(),
                            worktree: ".".to_string(),
                            group: FileGroup::Staged,
                        });
                    }
                    if worktree != '.' {
                        files.push(FileStatus {
                            path,
                            index: ".".to_string(),
                            worktree: worktree.to_string(),
                            group: FileGroup::Unstaged,
                        });
                    }
                    files
                }
                Some("u") if parts.len() >= 11 => vec![FileStatus {
                    path: parts[10..].join(" "),
                    index: "U".to_string(),
                    worktree: "U".to_string(),
                    group: FileGroup::Conflicted,
                }],
                _ => Vec::new(),
            }
        })
        .collect()
}

fn parse_branches(out: &str) -> Vec<Branch> {
    out.lines()
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let head = parts.next()?;
            let name = parts.next()?.to_string();
            // Detached HEAD emits a "(HEAD detached at abc1234)" pseudo-branch.
            if name.starts_with('(') {
                return None;
            }
            let upstream = parts
                .next()
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            Some(Branch {
                current: head == "*",
                name,
                upstream,
            })
        })
        .collect()
}

fn parse_stashes(out: &str) -> Vec<StashEntry> {
    out.lines()
        .filter_map(|line| {
            let (name, message) = line.split_once(':')?;
            Some(StashEntry {
                name: name.to_string(),
                message: message.trim_start().to_string(),
            })
        })
        .collect()
}

fn parse_worktrees(out: &str, current_root: &Path) -> Vec<Worktree> {
    let current = current_root
        .canonicalize()
        .unwrap_or_else(|_| current_root.to_path_buf());

    out.split("\n\n")
        .filter(|block| !block.trim().is_empty())
        .enumerate()
        .filter_map(|(index, block)| {
            let mut worktree = Worktree {
                path: String::new(),
                head: String::new(),
                branch: None,
                detached: false,
                bare: false,
                current: false,
                main: index == 0,
                locked: false,
                lock_reason: None,
                prunable: false,
            };
            for line in block.lines() {
                if let Some(path) = line.strip_prefix("worktree ") {
                    worktree.path = path.to_string();
                } else if let Some(head) = line.strip_prefix("HEAD ") {
                    worktree.head = head.chars().take(8).collect();
                } else if let Some(branch) = line.strip_prefix("branch ") {
                    worktree.branch = Some(
                        branch
                            .strip_prefix("refs/heads/")
                            .unwrap_or(branch)
                            .to_string(),
                    );
                } else if line == "detached" {
                    worktree.detached = true;
                } else if line == "bare" {
                    worktree.bare = true;
                } else if line == "locked" || line.starts_with("locked ") {
                    worktree.locked = true;
                    worktree.lock_reason = line
                        .strip_prefix("locked ")
                        .map(str::trim)
                        .filter(|reason| !reason.is_empty())
                        .map(ToOwned::to_owned);
                } else if line == "prunable" || line.starts_with("prunable ") {
                    worktree.prunable = true;
                }
            }
            if worktree.path.is_empty() {
                return None;
            }
            let path = Path::new(&worktree.path);
            worktree.current =
                path.canonicalize().unwrap_or_else(|_| path.to_path_buf()) == current;
            Some(worktree)
        })
        .collect()
}

fn parse_commit_graph(out: &str) -> Vec<CommitNode> {
    out.split('\u{1e}')
        .filter_map(|record| {
            let record = record.trim_matches('\n');
            if record.trim().is_empty() {
                return None;
            }
            let fields: Vec<&str> = record.split('\u{1f}').collect();
            if fields.len() != 7 {
                return None;
            }
            Some(CommitNode {
                hash: fields[0].to_string(),
                short_hash: fields[0].chars().take(8).collect(),
                parents: fields[1]
                    .split_whitespace()
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
                    .collect(),
                refs: fields[2]
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
                    .collect(),
                author: fields[3].to_string(),
                relative_date: fields[4].to_string(),
                subject: fields[5].to_string(),
                body_summary: summarize_commit_body(fields[6]),
            })
        })
        .collect()
}

fn summarize_commit_body(body: &str) -> String {
    body.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<&str>>()
        .join(" ")
        .chars()
        .take(180)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain_v2_status_groups() {
        let out = "\
1 .M N... 100644 100644 100644 abc abc README.md
1 A. N... 000000 100644 100644 000 abc src/main.ts
? notes.txt
u UU N... 100644 100644 100644 100644 a b c d conflicted.txt";
        let files = parse_status(out);

        assert_eq!(files.len(), 4);
        assert_eq!(files[0].group, FileGroup::Unstaged);
        assert_eq!(files[1].group, FileGroup::Staged);
        assert_eq!(files[2].group, FileGroup::Untracked);
        assert_eq!(files[3].group, FileGroup::Conflicted);
    }

    #[test]
    fn parses_porcelain_v2_rename_with_new_path() {
        let out = "2 R. N... 100644 100644 100644 df967b df967b R100 renamed.txt\told.txt";
        let files = parse_status(out);

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].group, FileGroup::Staged);
        assert_eq!(files[0].path, "renamed.txt");
        assert_eq!(files[0].index, "R");
    }

    #[test]
    fn drops_detached_head_pseudo_branch() {
        let out = "*\t(HEAD detached at 7d24eb3)\t\n \tmain\torigin/main";
        let branches = parse_branches(out);

        assert_eq!(branches.len(), 1);
        assert_eq!(branches[0].name, "main");
        assert!(!branches[0].current);
    }

    #[test]
    fn splits_partially_staged_files_into_both_sections() {
        let out = "1 MM N... 100644 100644 100644 abc def src/lib.rs";
        let files = parse_status(out);

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].group, FileGroup::Staged);
        assert_eq!(files[1].group, FileGroup::Unstaged);
    }

    #[test]
    fn serializes_file_groups_for_typescript() {
        assert_eq!(serde_json::to_string(&FileGroup::Staged).unwrap(), "\"staged\"");
        assert_eq!(
            serde_json::to_string(&FileGroup::Unstaged).unwrap(),
            "\"unstaged\""
        );
    }

    #[test]
    fn parses_branch_format() {
        let out = "*\tmain\torigin/main\n \tfeature\t";
        let branches = parse_branches(out);

        assert_eq!(branches.len(), 2);
        assert!(branches[0].current);
        assert_eq!(branches[0].upstream.as_deref(), Some("origin/main"));
        assert!(!branches[1].current);
    }

    #[test]
    fn rejects_paths_outside_repo() {
        assert!(validate_repo_relative_path("../secret").is_err());
        assert!(validate_repo_relative_path("/tmp/file").is_err());
        assert!(validate_repo_relative_path("src/main.rs").is_ok());
    }

    #[test]
    fn builds_safe_action_args() {
        let action = GitAction {
            kind: "reset".to_string(),
            path: None,
            message: None,
            branch: None,
            target: Some("HEAD~1".to_string()),
            remote: None,
            mode: Some("soft".to_string()),
        };

        assert_eq!(action_args(&action).unwrap(), vec!["reset", "--soft", "HEAD~1"]);
    }

    #[test]
    fn rejects_option_like_ref_arguments() {
        let action = GitAction {
            kind: "checkoutBranch".to_string(),
            path: None,
            message: None,
            branch: Some("--force".to_string()),
            target: None,
            remote: None,
            mode: None,
        };

        assert!(action_args(&action).is_err());
        assert!(validate_ref_arg("main").is_ok());
        assert!(validate_ref_arg("stash@{0}").is_ok());
        assert!(validate_ref_arg("-d").is_err());
        assert!(validate_ref_arg("  ").is_err());
    }

    #[test]
    fn builds_create_branch_with_start_point() {
        let action = GitAction {
            kind: "createBranch".to_string(),
            path: None,
            message: None,
            branch: Some("feature/x".to_string()),
            target: Some("abc123".to_string()),
            remote: None,
            mode: None,
        };

        assert_eq!(
            action_args(&action).unwrap(),
            vec!["checkout", "-b", "feature/x", "abc123"]
        );
    }

    #[test]
    fn parses_name_status_with_renames() {
        let out = "M\tsrc/App.svelte\nA\tsrc/lib/new.ts\nR100\told/name.ts\tnew/name.ts\nD\tgone.txt";
        let files = parse_name_status(out);

        assert_eq!(files.len(), 4);
        assert_eq!(files[0].status, "M");
        assert_eq!(files[0].path, "src/App.svelte");
        assert_eq!(files[2].status, "R");
        assert_eq!(files[2].path, "new/name.ts");
        assert_eq!(files[3].status, "D");
    }

    #[test]
    fn builds_amend_commit_args() {
        let action = GitAction {
            kind: "commitAmend".to_string(),
            path: None,
            message: Some("summary\n\ndescription".to_string()),
            branch: None,
            target: None,
            remote: None,
            mode: None,
        };

        assert_eq!(
            action_args(&action).unwrap(),
            vec!["commit", "--amend", "-m", "summary\n\ndescription"]
        );
    }

    fn action(kind: &str) -> GitAction {
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

    #[test]
    fn add_and_restore_actions_terminate_options_with_double_dash() {
        // Regression: a file whose name begins with `-` must reach git as an
        // operand, never as an option. Every add/restore path needs `--`.
        let cases = [
            ("stage", vec!["add", "--", "-rf"]),
            ("unstage", vec!["restore", "--staged", "--", "-rf"]),
            ("discard", vec!["restore", "--", "-rf"]),
            ("markResolved", vec!["add", "--", "-rf"]),
        ];
        for (kind, expected) in cases {
            let mut act = action(kind);
            act.path = Some("-rf".to_string());
            assert_eq!(action_args(&act).unwrap(), expected, "kind {kind}");
        }
    }

    #[test]
    fn clean_untracked_keeps_double_dash_separator() {
        let mut act = action("cleanUntracked");
        act.path = Some("junk.tmp".to_string());
        assert_eq!(
            action_args(&act).unwrap(),
            vec!["clean", "-f", "--", "junk.tmp"]
        );
    }

    #[test]
    fn push_and_pull_default_to_origin_and_reject_bad_remotes() {
        let mut push = action("push");
        assert_eq!(action_args(&push).unwrap(), vec!["push", "-u", "origin", "HEAD"]);
        push.remote = Some("upstream".to_string());
        assert_eq!(
            action_args(&push).unwrap(),
            vec!["push", "-u", "upstream", "HEAD"]
        );
        push.remote = Some("--exec=evil".to_string());
        assert!(action_args(&push).is_err(), "option-like remote must be rejected");

        let force = action("forcePush");
        assert_eq!(
            action_args(&force).unwrap(),
            vec!["push", "--force-with-lease", "origin", "HEAD"]
        );

        let pull = action("pull");
        assert_eq!(action_args(&pull).unwrap(), vec!["pull", "--ff-only", "origin"]);
    }

    #[test]
    fn stash_actions_require_their_target() {
        for kind in ["stashApply", "stashPop", "stashDrop"] {
            assert!(action_args(&action(kind)).is_err(), "kind {kind} needs a target");
        }
        let mut apply = action("stashApply");
        apply.target = Some("stash@{0}".to_string());
        assert_eq!(action_args(&apply).unwrap(), vec!["stash", "apply", "stash@{0}"]);
    }

    #[test]
    fn reset_rejects_unknown_mode() {
        let mut act = action("reset");
        act.target = Some("HEAD".to_string());
        act.mode = Some("nuclear".to_string());
        assert!(action_args(&act).is_err());
    }

    #[test]
    fn unknown_action_kind_is_rejected() {
        assert!(action_args(&action("rm -rf /")).is_err());
    }

    #[test]
    fn required_arguments_are_enforced() {
        // commit without a message, branch ops without a branch, etc.
        assert!(action_args(&action("commit")).is_err());
        assert!(action_args(&action("checkoutBranch")).is_err());
        assert!(action_args(&action("createTag")).is_err());
        assert!(action_args(&action("merge")).is_err());
        // whitespace-only values count as missing
        let mut blank = action("checkoutBranch");
        blank.branch = Some("   ".to_string());
        assert!(action_args(&blank).is_err());
    }

    #[test]
    fn validate_repo_relative_path_blocks_traversal_variants() {
        assert!(validate_repo_relative_path("a/../../etc/passwd").is_err());
        assert!(validate_repo_relative_path("./../secret").is_err());
        assert!(validate_repo_relative_path("nested/ok/path.rs").is_ok());
        assert!(validate_repo_relative_path("").is_ok());
    }
}

// End-to-end tests that drive the real git CLI in throwaway repositories.
#[cfg(test)]
mod git_integration_tests {
    use super::*;
    // Explicit (non-glob) imports so these shadow same-named items pulled in
    // by `use super::*` above (notably the crate's own `run()` entry point).
    use super::test_support::{
        act, commit_all, head_subject, ok_action, run, write_file, TempRepo, REPO_COUNTER,
    };
    use std::sync::atomic::Ordering;

    #[test]
    fn reports_status_groups_from_a_real_repository() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "one\n");
        commit_all(repo.path(), "init");

        write_file(repo.path(), "a.txt", "two\n");
        write_file(repo.path(), "b.txt", "new\n");
        run(repo.path(), &["add", "b.txt"]);

        let state = repository_state(repo.path()).unwrap();
        assert_eq!(state.current_branch.as_deref(), Some("main"));
        assert!(!state.merging);
        assert!(state
            .files
            .iter()
            .any(|file| file.path == "a.txt" && file.group == FileGroup::Unstaged));
        assert!(state
            .files
            .iter()
            .any(|file| file.path == "b.txt" && file.group == FileGroup::Staged));
    }

    #[test]
    fn reports_staged_rename_under_its_new_path() {
        let repo = TempRepo::new();
        write_file(repo.path(), "old.txt", "content\n");
        commit_all(repo.path(), "init");
        run(repo.path(), &["mv", "old.txt", "new.txt"]);

        let state = repository_state(repo.path()).unwrap();
        let staged: Vec<_> = state
            .files
            .iter()
            .filter(|file| file.group == FileGroup::Staged)
            .collect();
        assert_eq!(staged.len(), 1, "files: {:?}", state.files);
        assert_eq!(staged[0].path, "new.txt");
    }

    #[test]
    fn lists_stashes_tags_and_remote_state() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "one\n");
        commit_all(repo.path(), "init");
        run(repo.path(), &["tag", "v1.0.0"]);
        write_file(repo.path(), "a.txt", "dirty\n");
        run(repo.path(), &["stash", "push", "-m", "wip work"]);

        let state = repository_state(repo.path()).unwrap();
        assert_eq!(state.tags, vec!["v1.0.0".to_string()]);
        assert_eq!(state.stashes.len(), 1);
        assert_eq!(state.stashes[0].name, "stash@{0}");
        assert!(state.stashes[0].message.contains("wip work"));
        assert_eq!(state.user_name.as_deref(), Some("Test User"));
    }

    #[test]
    fn merge_commit_detail_diffs_against_first_parent() {
        let repo = TempRepo::new();
        write_file(repo.path(), "base.txt", "base\n");
        commit_all(repo.path(), "base");
        run(repo.path(), &["checkout", "-b", "feature"]);
        write_file(repo.path(), "feature.txt", "feature\n");
        commit_all(repo.path(), "feature work");
        run(repo.path(), &["checkout", "main"]);
        write_file(repo.path(), "main.txt", "main\n");
        commit_all(repo.path(), "main work");
        run(repo.path(), &["merge", "--no-ff", "feature", "-m", "merge feature"]);

        let head = git(repo.path(), &["rev-parse", "HEAD"]).unwrap();
        let detail = commit_detail(repo.path(), &head).unwrap();
        assert_eq!(detail.subject, "merge feature");
        assert_eq!(detail.parents.len(), 2);
        assert!(
            detail
                .files
                .iter()
                .any(|file| file.path == "feature.txt" && file.status == "A"),
            "merge detail files: {:?}",
            detail.files
        );

        let diff = commit_file_diff(repo.path(), &head, "feature.txt".to_string()).unwrap();
        assert!(diff.diff.contains("+feature"), "diff: {}", diff.diff);
    }

    #[test]
    fn commit_graph_skips_stash_refs_and_orders_topologically() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "one\n");
        commit_all(repo.path(), "first");
        write_file(repo.path(), "a.txt", "two\n");
        commit_all(repo.path(), "second");
        write_file(repo.path(), "a.txt", "dirty\n");
        run(repo.path(), &["stash", "push", "-m", "noise"]);

        let graph = commit_graph(repo.path(), 50).unwrap();
        assert_eq!(graph.commits.len(), 2, "stash refs must not appear");
        assert_eq!(graph.commits[0].subject, "second");
        assert_eq!(graph.commits[1].subject, "first");
        assert_eq!(graph.commits[0].parents.len(), 1);
        assert!(graph.commits[0].refs.iter().any(|r| r.contains("main")));
    }

    #[test]
    fn commit_graph_is_empty_for_a_fresh_repository() {
        let repo = TempRepo::new();
        let graph = commit_graph(repo.path(), 50).unwrap();
        assert!(graph.commits.is_empty());

        let state = repository_state(repo.path()).unwrap();
        assert_eq!(state.head, "unborn");
        assert!(state.files.is_empty());
    }

    #[test]
    fn conflict_exposes_base_ours_theirs_and_merging_state() {
        let repo = TempRepo::new();
        write_file(repo.path(), "file.txt", "base\n");
        commit_all(repo.path(), "base");
        run(repo.path(), &["checkout", "-b", "feature"]);
        write_file(repo.path(), "file.txt", "theirs\n");
        commit_all(repo.path(), "theirs");
        run(repo.path(), &["checkout", "main"]);
        write_file(repo.path(), "file.txt", "ours\n");
        commit_all(repo.path(), "ours");

        let merge = run_git(repo.path(), &["merge", "feature"]);
        assert!(!merge.ok, "merge must conflict");

        let state = repository_state(repo.path()).unwrap();
        assert!(state.merging);
        assert!(state
            .files
            .iter()
            .any(|file| file.path == "file.txt" && file.group == FileGroup::Conflicted));

        let conflict = conflict_file(repo.path(), "file.txt".to_string()).unwrap();
        assert_eq!(conflict.base.as_deref(), Some("base"));
        assert_eq!(conflict.ours.as_deref(), Some("ours"));
        assert_eq!(conflict.theirs.as_deref(), Some("theirs"));
        assert!(conflict.working.contains("<<<<<<<"));

        // Resolve, mark resolved, and confirm the conflict clears.
        fs::write(repo.path().join("file.txt"), "resolved\n").unwrap();
        run(repo.path(), &["add", "file.txt"]);
        let state = repository_state(repo.path()).unwrap();
        assert!(!state
            .files
            .iter()
            .any(|file| file.group == FileGroup::Conflicted));
    }

    #[test]
    fn detached_head_yields_no_phantom_branch() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "one\n");
        commit_all(repo.path(), "first");
        run(repo.path(), &["checkout", "--detach", "HEAD"]);

        let state = repository_state(repo.path()).unwrap();
        assert!(state.current_branch.is_none());
        assert_eq!(state.branches.len(), 1);
        assert_eq!(state.branches[0].name, "main");
        assert!(!state.branches[0].current);
    }

    #[test]
    fn untracked_file_diff_renders_synthetic_patch() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "one\n");
        commit_all(repo.path(), "init");
        write_file(repo.path(), "notes.md", "hello\nworld\n");

        let diff = render_untracked_file_diff(repo.path(), "notes.md").unwrap();
        assert!(diff.contains("--- /dev/null"));
        assert!(diff.contains("+hello"));
        assert!(diff.contains("+world"));
        assert!(!diff.contains("\\ No newline at end of file"));
    }

    // ---- success paths for the action dispatcher ----

    #[test]
    fn stage_then_commit_advances_head_and_cleans_tree() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "one\n");
        commit_all(repo.path(), "init");

        write_file(repo.path(), "b.txt", "content\n");
        let mut stage = act("stage");
        stage.path = Some("b.txt".to_string());
        ok_action(repo.path(), &stage);
        assert!(repository_state(repo.path())
            .unwrap()
            .files
            .iter()
            .any(|f| f.path == "b.txt" && f.group == FileGroup::Staged));

        let mut commit = act("commit");
        commit.message = Some("add b".to_string());
        ok_action(repo.path(), &commit);

        assert_eq!(head_subject(repo.path()), "add b");
        assert!(repository_state(repo.path()).unwrap().files.is_empty());
    }

    #[test]
    fn unstage_and_discard_return_file_to_pristine_state() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "one\n");
        commit_all(repo.path(), "init");

        write_file(repo.path(), "a.txt", "two\n");
        run(repo.path(), &["add", "a.txt"]);

        let mut unstage = act("unstage");
        unstage.path = Some("a.txt".to_string());
        ok_action(repo.path(), &unstage);
        assert!(repository_state(repo.path())
            .unwrap()
            .files
            .iter()
            .all(|f| f.group != FileGroup::Staged));

        let mut discard = act("discard");
        discard.path = Some("a.txt".to_string());
        ok_action(repo.path(), &discard);
        assert!(repository_state(repo.path()).unwrap().files.is_empty());
        assert_eq!(fs::read_to_string(repo.path().join("a.txt")).unwrap(), "one\n");
    }

    #[test]
    fn clean_untracked_removes_the_file() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "one\n");
        commit_all(repo.path(), "init");
        write_file(repo.path(), "junk.tmp", "garbage\n");

        let mut clean = act("cleanUntracked");
        clean.path = Some("junk.tmp".to_string());
        ok_action(repo.path(), &clean);
        assert!(!repo.path().join("junk.tmp").exists());
    }

    #[test]
    fn amend_rewrites_the_head_commit_message() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "one\n");
        commit_all(repo.path(), "typo");

        let mut amend = act("commitAmend");
        amend.message = Some("fixed message".to_string());
        ok_action(repo.path(), &amend);
        assert_eq!(head_subject(repo.path()), "fixed message");
    }

    #[test]
    fn branch_lifecycle_create_checkout_and_delete() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "one\n");
        commit_all(repo.path(), "init");

        let mut create = act("createBranch");
        create.branch = Some("feature".to_string());
        ok_action(repo.path(), &create);
        assert_eq!(
            repository_state(repo.path()).unwrap().current_branch.as_deref(),
            Some("feature")
        );

        let mut checkout = act("checkoutBranch");
        checkout.branch = Some("main".to_string());
        ok_action(repo.path(), &checkout);

        let mut delete = act("deleteBranch");
        delete.branch = Some("feature".to_string());
        ok_action(repo.path(), &delete);
        assert!(repository_state(repo.path())
            .unwrap()
            .branches
            .iter()
            .all(|b| b.name != "feature"));
    }

    #[test]
    fn tag_create_and_delete_round_trip() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "one\n");
        commit_all(repo.path(), "init");

        let mut create = act("createTag");
        create.branch = Some("v1.0.0".to_string());
        ok_action(repo.path(), &create);
        assert_eq!(repository_state(repo.path()).unwrap().tags, vec!["v1.0.0"]);

        let mut delete = act("deleteTag");
        delete.branch = Some("v1.0.0".to_string());
        ok_action(repo.path(), &delete);
        assert!(repository_state(repo.path()).unwrap().tags.is_empty());
    }

    #[test]
    fn stash_create_apply_and_drop_round_trip() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "one\n");
        commit_all(repo.path(), "init");
        write_file(repo.path(), "a.txt", "dirty\n");

        let mut create = act("stashCreate");
        create.message = Some("wip".to_string());
        ok_action(repo.path(), &create);
        assert!(repository_state(repo.path()).unwrap().files.is_empty());
        assert_eq!(repository_state(repo.path()).unwrap().stashes.len(), 1);

        let mut apply = act("stashApply");
        apply.target = Some("stash@{0}".to_string());
        ok_action(repo.path(), &apply);
        assert_eq!(fs::read_to_string(repo.path().join("a.txt")).unwrap(), "dirty\n");

        let mut drop = act("stashDrop");
        drop.target = Some("stash@{0}".to_string());
        ok_action(repo.path(), &drop);
        assert!(repository_state(repo.path()).unwrap().stashes.is_empty());
    }

    #[test]
    fn cherry_pick_and_revert_apply_and_undo_a_change() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "base\n");
        commit_all(repo.path(), "base");
        run(repo.path(), &["checkout", "-b", "feature"]);
        write_file(repo.path(), "feature.txt", "feature\n");
        commit_all(repo.path(), "feature commit");
        let feature_hash = git(repo.path(), &["rev-parse", "HEAD"]).unwrap();
        run(repo.path(), &["checkout", "main"]);

        let mut pick = act("cherryPick");
        pick.target = Some(feature_hash);
        ok_action(repo.path(), &pick);
        assert!(repo.path().join("feature.txt").exists());

        let head = git(repo.path(), &["rev-parse", "HEAD"]).unwrap();
        let mut revert = act("revert");
        revert.target = Some(head);
        ok_action(repo.path(), &revert);
        assert!(!repo.path().join("feature.txt").exists());
    }

    #[test]
    fn reset_modes_move_head_and_control_the_index() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "one\n");
        commit_all(repo.path(), "first");
        write_file(repo.path(), "a.txt", "two\n");
        commit_all(repo.path(), "second");

        // soft: HEAD moves back, change stays staged.
        let mut soft = act("reset");
        soft.target = Some("HEAD~1".to_string());
        soft.mode = Some("soft".to_string());
        ok_action(repo.path(), &soft);
        assert_eq!(head_subject(repo.path()), "first");
        assert!(repository_state(repo.path())
            .unwrap()
            .files
            .iter()
            .any(|f| f.group == FileGroup::Staged));

        // hard: index and worktree return to HEAD.
        let mut hard = act("reset");
        hard.target = Some("HEAD".to_string());
        hard.mode = Some("hard".to_string());
        ok_action(repo.path(), &hard);
        assert!(repository_state(repo.path()).unwrap().files.is_empty());
    }

    #[test]
    fn merge_fast_forward_advances_head() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "one\n");
        commit_all(repo.path(), "base");
        run(repo.path(), &["checkout", "-b", "feature"]);
        write_file(repo.path(), "b.txt", "two\n");
        commit_all(repo.path(), "feature work");
        run(repo.path(), &["checkout", "main"]);

        let mut merge = act("merge");
        merge.target = Some("feature".to_string());
        ok_action(repo.path(), &merge);
        assert_eq!(head_subject(repo.path()), "feature work");
    }

    #[test]
    fn rebase_replays_commits_onto_target() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "one\n");
        commit_all(repo.path(), "base");
        run(repo.path(), &["checkout", "-b", "feature"]);
        write_file(repo.path(), "feature.txt", "f\n");
        commit_all(repo.path(), "feature work");
        run(repo.path(), &["checkout", "main"]);
        write_file(repo.path(), "main.txt", "m\n");
        commit_all(repo.path(), "main work");
        run(repo.path(), &["checkout", "feature"]);

        let mut rebase = act("rebase");
        rebase.target = Some("main".to_string());
        ok_action(repo.path(), &rebase);
        // feature now sits on top of main work.
        let parents = git(repo.path(), &["log", "--format=%s"]).unwrap();
        assert!(parents.contains("main work"));
        assert_eq!(head_subject(repo.path()), "feature work");
    }

    #[test]
    fn checkout_remote_tracks_and_checkout_commit_detaches() {
        // Build an "origin" by cloning so a remote-tracking branch exists.
        let origin = TempRepo::new();
        write_file(origin.path(), "a.txt", "one\n");
        commit_all(origin.path(), "init");
        run(origin.path(), &["checkout", "-b", "released"]);
        write_file(origin.path(), "b.txt", "two\n");
        commit_all(origin.path(), "release work");
        run(origin.path(), &["checkout", "main"]);

        let clone_dir =
            std::env::temp_dir().join(format!("gitc-clone-{}", std::process::id()));
        fs::remove_dir_all(&clone_dir).ok();
        run(
            std::env::temp_dir().as_path(),
            &[
                "clone",
                origin.path().to_str().unwrap(),
                clone_dir.to_str().unwrap(),
            ],
        );

        let mut track = act("checkoutRemote");
        track.target = Some("origin/released".to_string());
        ok_action(&clone_dir, &track);
        assert_eq!(
            repository_state(&clone_dir).unwrap().current_branch.as_deref(),
            Some("released")
        );

        let head = git(&clone_dir, &["rev-parse", "HEAD"]).unwrap();
        let mut detach = act("checkoutCommit");
        detach.target = Some(head);
        ok_action(&clone_dir, &detach);
        assert!(repository_state(&clone_dir).unwrap().current_branch.is_none());

        fs::remove_dir_all(&clone_dir).ok();
    }

    // ---- hunk staging, file content, blame, history, conflict save ----

    #[test]
    fn apply_hunk_stages_then_unstages_a_patch() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "one\ntwo\nthree\n");
        commit_all(repo.path(), "init");
        write_file(repo.path(), "a.txt", "one\nCHANGED\nthree\n");

        let patch = git(repo.path(), &["diff", "--", "a.txt"]).unwrap() + "\n";

        let staged = apply_hunk_patch(repo.path(), &patch, "stage").unwrap();
        assert!(staged.ok, "stage failed: {}", staged.stderr);
        assert!(repository_state(repo.path())
            .unwrap()
            .files
            .iter()
            .any(|f| f.group == FileGroup::Staged));

        let staged_patch = git(repo.path(), &["diff", "--cached", "--", "a.txt"]).unwrap() + "\n";
        let unstaged = apply_hunk_patch(repo.path(), &staged_patch, "unstage").unwrap();
        assert!(unstaged.ok, "unstage failed: {}", unstaged.stderr);
        assert!(repository_state(repo.path())
            .unwrap()
            .files
            .iter()
            .all(|f| f.group != FileGroup::Staged));
    }

    #[test]
    fn apply_hunk_rejects_unknown_mode() {
        assert!(hunk_args("frobnicate").is_err());
    }

    #[test]
    fn file_content_reads_staged_and_working_versions() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "committed\n");
        commit_all(repo.path(), "init");
        write_file(repo.path(), "a.txt", "working\n");

        assert_eq!(
            file_content(repo.path(), "a.txt".to_string(), true).unwrap(),
            "committed"
        );
        assert_eq!(
            file_content(repo.path(), "a.txt".to_string(), false).unwrap(),
            "working\n"
        );
    }

    #[test]
    fn blame_and_history_report_the_authoring_commit() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "line\n");
        commit_all(repo.path(), "first change");
        write_file(repo.path(), "a.txt", "line\nmore\n");
        commit_all(repo.path(), "second change");

        let blame = file_blame(repo.path(), "a.txt").unwrap();
        assert!(blame.contains("Test User"));
        assert!(blame.contains("line"));

        let history = file_history(repo.path(), "a.txt").unwrap();
        assert!(history.contains("first change"));
        assert!(history.contains("second change"));
    }

    #[test]
    fn file_diff_shows_staged_and_untracked_changes() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "one\n");
        commit_all(repo.path(), "init");

        // Untracked file: synthetic new-file patch on the unstaged side.
        write_file(repo.path(), "new.txt", "fresh\n");
        let untracked = file_diff(repo.path(), "new.txt".to_string(), false).unwrap();
        assert!(untracked.diff.contains("+fresh"));
        assert!(!untracked.binary);

        // Staged edit shows on the staged side.
        write_file(repo.path(), "a.txt", "two\n");
        run(repo.path(), &["add", "a.txt"]);
        let staged = file_diff(repo.path(), "a.txt".to_string(), true).unwrap();
        assert!(staged.diff.contains("+two"));
    }

    #[test]
    fn save_conflict_resolution_writes_working_file() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "one\n");
        commit_all(repo.path(), "init");

        conflict_write(repo.path(), "a.txt", "resolved contents\n");
        assert_eq!(
            fs::read_to_string(repo.path().join("a.txt")).unwrap(),
            "resolved contents\n"
        );
    }

    // Mirror of save_conflict_resolution's core without the Tauri State.
    fn conflict_write(root: &Path, path: &str, content: &str) {
        validate_repo_relative_path(path).unwrap();
        fs::write(root.join(path), content).unwrap();
    }

    // ---- repository discovery, creation, cloning ----

    #[test]
    fn discover_repo_root_finds_toplevel_from_subdirectory() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "one\n");
        commit_all(repo.path(), "init");
        fs::create_dir_all(repo.path().join("nested/deep")).unwrap();

        let found = discover_repo_root(&repo.path().join("nested/deep")).unwrap();
        // Compare canonicalized paths (temp dirs may be symlinked, e.g. /var vs /private/var).
        assert_eq!(
            fs::canonicalize(&found).unwrap(),
            fs::canonicalize(repo.path()).unwrap()
        );
    }

    #[test]
    fn discover_repo_root_errors_outside_any_repository() {
        let dir = std::env::temp_dir().join(format!("gitc-notrepo-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        assert!(discover_repo_root(&dir).is_err());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn clone_from_a_local_source_reproduces_history() {
        let origin = TempRepo::new();
        write_file(origin.path(), "a.txt", "one\n");
        commit_all(origin.path(), "seed");

        let target = std::env::temp_dir().join(format!("gitc-clonetgt-{}", std::process::id()));
        fs::remove_dir_all(&target).ok();
        let parent = target.parent().unwrap();
        let result = run_git(
            parent,
            &["clone", "--", origin.path().to_str().unwrap(), target.to_str().unwrap()],
        );
        assert!(result.ok, "clone failed: {}", result.stderr);

        let state = repository_state(&target).unwrap();
        assert!(state.remotes.iter().any(|r| r == "origin"));
        assert_eq!(head_subject(&target), "seed");
        fs::remove_dir_all(&target).ok();
    }

    // ---- failure paths: "what happens if they fail" ----

    #[test]
    fn commit_with_nothing_staged_fails_cleanly() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "one\n");
        commit_all(repo.path(), "init");

        let mut commit = act("commit");
        commit.message = Some("empty".to_string());
        let result = run_action(repo.path(), &commit).unwrap();
        assert!(!result.ok, "expected failure with a clean tree");
        assert!(!result.stdout.is_empty() || !result.stderr.is_empty());
        // A failed action still asks the UI to refresh so state stays accurate.
        assert!(result.refresh);
    }

    #[test]
    fn deleting_an_unmerged_branch_fails_until_forced() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "one\n");
        commit_all(repo.path(), "init");
        run(repo.path(), &["checkout", "-b", "feature"]);
        write_file(repo.path(), "b.txt", "two\n");
        commit_all(repo.path(), "unmerged work");
        run(repo.path(), &["checkout", "main"]);

        let mut delete = act("deleteBranch");
        delete.branch = Some("feature".to_string());
        let safe = run_action(repo.path(), &delete).unwrap();
        assert!(!safe.ok, "unmerged branch must not delete with -d");
        assert!(repository_state(repo.path())
            .unwrap()
            .branches
            .iter()
            .any(|b| b.name == "feature"));

        let mut force = act("deleteBranchForce");
        force.branch = Some("feature".to_string());
        ok_action(repo.path(), &force);
        assert!(repository_state(repo.path())
            .unwrap()
            .branches
            .iter()
            .all(|b| b.name != "feature"));
    }

    #[test]
    fn checking_out_a_missing_branch_reports_an_error() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "one\n");
        commit_all(repo.path(), "init");

        let mut checkout = act("checkoutBranch");
        checkout.branch = Some("does-not-exist".to_string());
        let result = run_action(repo.path(), &checkout).unwrap();
        assert!(!result.ok);
        assert!(result.stderr.to_lowercase().contains("did not match")
            || result.stderr.to_lowercase().contains("invalid reference")
            || !result.stderr.is_empty());
    }

    #[test]
    fn fast_forward_pull_fails_when_histories_diverge() {
        let origin = TempRepo::new();
        write_file(origin.path(), "a.txt", "one\n");
        commit_all(origin.path(), "seed");

        let clone_dir = std::env::temp_dir().join(format!("gitc-pull-{}", std::process::id()));
        fs::remove_dir_all(&clone_dir).ok();
        run(
            std::env::temp_dir().as_path(),
            &["clone", origin.path().to_str().unwrap(), clone_dir.to_str().unwrap()],
        );
        // A plain clone has no local identity; this test commits into it directly.
        run(&clone_dir, &["config", "user.email", "test@gitc.dev"]);
        run(&clone_dir, &["config", "user.name", "Test User"]);
        run(&clone_dir, &["config", "commit.gpgsign", "false"]);

        // Diverge: origin gets a commit, clone gets a different one.
        write_file(origin.path(), "a.txt", "origin change\n");
        commit_all(origin.path(), "origin work");
        write_file(&clone_dir, "a.txt", "local change\n");
        commit_all(&clone_dir, "local work");

        let pull = act("pull");
        let result = run_action(&clone_dir, &pull).unwrap();
        assert!(!result.ok, "ff-only pull must refuse to merge divergent history");

        fs::remove_dir_all(&clone_dir).ok();
    }

    #[test]
    fn conflicting_merge_can_be_aborted() {
        let repo = TempRepo::new();
        write_file(repo.path(), "file.txt", "base\n");
        commit_all(repo.path(), "base");
        run(repo.path(), &["checkout", "-b", "feature"]);
        write_file(repo.path(), "file.txt", "theirs\n");
        commit_all(repo.path(), "theirs");
        run(repo.path(), &["checkout", "main"]);
        write_file(repo.path(), "file.txt", "ours\n");
        commit_all(repo.path(), "ours");

        let mut merge = act("merge");
        merge.target = Some("feature".to_string());
        let result = run_action(repo.path(), &merge).unwrap();
        assert!(!result.ok, "merge should conflict");
        assert!(repository_state(repo.path()).unwrap().merging);

        let abort = act("mergeAbort");
        ok_action(repo.path(), &abort);
        assert!(!repository_state(repo.path()).unwrap().merging);
        assert_eq!(fs::read_to_string(repo.path().join("file.txt")).unwrap(), "ours\n");
    }

    #[test]
    fn conflicting_merge_can_be_resolved_and_continued() {
        let repo = TempRepo::new();
        write_file(repo.path(), "file.txt", "base\n");
        commit_all(repo.path(), "base");
        run(repo.path(), &["checkout", "-b", "feature"]);
        write_file(repo.path(), "file.txt", "theirs\n");
        commit_all(repo.path(), "theirs");
        run(repo.path(), &["checkout", "main"]);
        write_file(repo.path(), "file.txt", "ours\n");
        commit_all(repo.path(), "ours");

        let mut merge = act("merge");
        merge.target = Some("feature".to_string());
        assert!(!run_action(repo.path(), &merge).unwrap().ok);

        // Resolve via markResolved, then continue.
        fs::write(repo.path().join("file.txt"), "resolved\n").unwrap();
        let mut resolved = act("markResolved");
        resolved.path = Some("file.txt".to_string());
        ok_action(repo.path(), &resolved);

        let cont = act("mergeContinue");
        let result = run_action(repo.path(), &cont).unwrap();
        assert!(result.ok, "merge --continue failed: {}", result.stderr);
        assert!(!repository_state(repo.path()).unwrap().merging);
        assert_eq!(repository_state(repo.path()).unwrap().head.len() >= 4, true);
    }

    #[test]
    fn applying_a_corrupt_hunk_fails_without_touching_the_index() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "one\n");
        commit_all(repo.path(), "init");

        let bogus = "diff --git a/a.txt b/a.txt\n@@ -99,3 +99,3 @@\n-nonexistent\n+garbage\n";
        let result = apply_hunk_patch(repo.path(), bogus, "stage").unwrap();
        assert!(!result.ok, "corrupt patch must not apply");
        assert!(repository_state(repo.path()).unwrap().files.is_empty());
    }

    #[test]
    fn actions_reject_option_injection_and_traversal_before_running() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "one\n");
        commit_all(repo.path(), "init");

        // Ref argument that looks like an option is rejected up front.
        let mut evil_branch = act("checkoutBranch");
        evil_branch.branch = Some("--orphan".to_string());
        assert!(run_action(repo.path(), &evil_branch).is_err());

        // Path escaping the repo is rejected up front.
        let mut evil_path = act("stage");
        evil_path.path = Some("../outside.txt".to_string());
        assert!(run_action(repo.path(), &evil_path).is_err());
    }

    #[test]
    fn a_file_named_like_an_option_is_staged_as_a_path() {
        // Regression for the `--` separator fix: a real file called `-a.txt`
        // must be added as a pathspec, not interpreted as `git add -a`.
        let repo = TempRepo::new();
        write_file(repo.path(), "keep.txt", "keep\n");
        commit_all(repo.path(), "init");
        write_file(repo.path(), "-a.txt", "tricky\n");

        let mut stage = act("stage");
        stage.path = Some("-a.txt".to_string());
        let result = run_action(repo.path(), &stage).unwrap();
        assert!(result.ok, "staging a dash-named file failed: {}", result.stderr);
        assert!(repository_state(repo.path())
            .unwrap()
            .files
            .iter()
            .any(|f| f.path == "-a.txt" && f.group == FileGroup::Staged));
    }

    #[test]
    fn parses_worktree_porcelain_metadata() {
        let out = "\
worktree /repos/gitc
HEAD 1111111111111111111111111111111111111111
branch refs/heads/main

worktree /repos/gitc-review
HEAD 2222222222222222222222222222222222222222
branch refs/heads/feature/review
locked demo machine

worktree /repos/gitc-old
HEAD 3333333333333333333333333333333333333333
detached
prunable gitdir file points to non-existent location
";
        let worktrees = parse_worktrees(out, Path::new("/repos/gitc-review"));

        assert_eq!(worktrees.len(), 3);
        assert_eq!(worktrees[0].path, "/repos/gitc");
        assert_eq!(worktrees[0].branch.as_deref(), Some("main"));
        assert_eq!(worktrees[0].head, "11111111");
        assert!(worktrees[0].main);
        assert!(!worktrees[0].current);

        assert_eq!(worktrees[1].branch.as_deref(), Some("feature/review"));
        assert!(worktrees[1].locked);
        assert_eq!(worktrees[1].lock_reason.as_deref(), Some("demo machine"));
        assert!(worktrees[1].current);
        assert!(!worktrees[1].main);

        assert!(worktrees[2].detached);
        assert!(worktrees[2].branch.is_none());
        assert!(worktrees[2].prunable);
        assert!(!worktrees[2].locked);
    }

    #[test]
    fn builds_worktree_action_args() {
        let mut add = act("worktreeAdd");
        add.path = Some("/repos/gitc-fix".to_string());
        add.branch = Some("fix/lanes".to_string());
        assert_eq!(
            action_args(&add).unwrap(),
            vec!["worktree", "add", "/repos/gitc-fix", "fix/lanes"]
        );

        add.mode = Some("new".to_string());
        add.target = Some("main".to_string());
        assert_eq!(
            action_args(&add).unwrap(),
            vec!["worktree", "add", "-b", "fix/lanes", "/repos/gitc-fix", "main"]
        );

        let mut detach = act("worktreeAdd");
        detach.mode = Some("detach".to_string());
        detach.path = Some("/repos/gitc-v1".to_string());
        detach.target = Some("v0.1.0".to_string());
        assert_eq!(
            action_args(&detach).unwrap(),
            vec!["worktree", "add", "--detach", "/repos/gitc-v1", "v0.1.0"]
        );

        let mut remove = act("worktreeRemove");
        remove.path = Some("/repos/gitc-fix".to_string());
        assert_eq!(
            action_args(&remove).unwrap(),
            vec!["worktree", "remove", "/repos/gitc-fix"]
        );
        remove.kind = "worktreeRemoveForce".to_string();
        assert_eq!(
            action_args(&remove).unwrap(),
            vec!["worktree", "remove", "--force", "/repos/gitc-fix"]
        );

        assert_eq!(
            action_args(&act("worktreePrune")).unwrap(),
            vec!["worktree", "prune", "-v"]
        );
    }

    #[test]
    fn rejects_unsafe_worktree_paths() {
        let mut add = act("worktreeAdd");
        add.branch = Some("fix/lanes".to_string());

        add.path = Some("relative/dir".to_string());
        assert!(action_args(&add).is_err(), "relative paths must be rejected");

        add.path = Some("--force".to_string());
        assert!(action_args(&add).is_err(), "option-like paths must be rejected");

        add.path = Some("  ".to_string());
        assert!(action_args(&add).is_err(), "blank paths must be rejected");

        let mut add = act("worktreeAdd");
        add.path = Some("/repos/gitc-fix".to_string());
        assert!(action_args(&add).is_err(), "branch is required");
    }

    #[test]
    fn worktree_add_list_remove_and_prune_roundtrip() {
        let repo = TempRepo::new();
        write_file(repo.path(), "README.md", "hello\n");
        commit_all(repo.path(), "init");

        let linked = std::env::temp_dir().join(format!(
            "gitc-test-wt-{}-{}",
            std::process::id(),
            REPO_COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let linked_str = linked.display().to_string();

        // Add a worktree on a new branch cut from main.
        let mut add = act("worktreeAdd");
        add.mode = Some("new".to_string());
        add.branch = Some("feature/wt".to_string());
        add.path = Some(linked_str.clone());
        add.target = Some("main".to_string());
        let result = run_action(repo.path(), &add).unwrap();
        assert!(result.ok, "worktree add failed: {}", result.stderr);

        let state = repository_state(repo.path()).unwrap();
        assert_eq!(state.worktrees.len(), 2);
        let entry = state
            .worktrees
            .iter()
            .find(|w| w.branch.as_deref() == Some("feature/wt"))
            .expect("linked worktree listed");
        assert!(!entry.main);
        assert!(!entry.current);

        // The linked worktree is itself a usable repository root.
        let linked_state = repository_state(&linked).unwrap();
        assert_eq!(linked_state.current_branch.as_deref(), Some("feature/wt"));
        assert!(
            linked_state.worktrees.iter().any(|w| w.current && !w.main),
            "linked worktree should mark itself current"
        );

        // Removing the worktree that is currently open must be refused.
        let mut remove_self = act("worktreeRemove");
        remove_self.path = Some(linked_str.clone());
        assert!(run_action(&linked, &remove_self).is_err());

        // Removing it from the main worktree succeeds.
        let result = run_action(repo.path(), &remove_self).unwrap();
        assert!(result.ok, "worktree remove failed: {}", result.stderr);
        assert_eq!(repository_state(repo.path()).unwrap().worktrees.len(), 1);

        // A worktree whose directory vanished shows up as prunable, and prune clears it.
        let mut add = act("worktreeAdd");
        add.branch = Some("feature/wt".to_string());
        add.path = Some(linked_str.clone());
        let result = run_action(repo.path(), &add).unwrap();
        assert!(result.ok, "re-add failed: {}", result.stderr);
        fs::remove_dir_all(&linked).expect("delete worktree dir");

        let state = repository_state(repo.path()).unwrap();
        assert!(
            state.worktrees.iter().any(|w| w.prunable),
            "missing directory should be prunable"
        );

        let result = run_action(repo.path(), &act("worktreePrune")).unwrap();
        assert!(result.ok, "prune failed: {}", result.stderr);
        assert_eq!(repository_state(repo.path()).unwrap().worktrees.len(), 1);
    }

    // ---- default_base_branch ----

    #[test]
    fn default_base_branch_is_main_on_a_fresh_repo() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "one\n");
        commit_all(repo.path(), "init");

        assert_eq!(default_base_branch(repo.path()).unwrap(), "main");
    }

    #[test]
    fn default_base_branch_stays_main_after_switching_branches() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "one\n");
        commit_all(repo.path(), "init");
        run(repo.path(), &["checkout", "-b", "other"]);

        assert_eq!(default_base_branch(repo.path()).unwrap(), "main");
    }

    #[test]
    fn default_base_branch_falls_back_to_current_branch_without_main_or_master() {
        let dir = std::env::temp_dir().join(format!("gitc-test-trunk-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("create temp repo dir");
        run(&dir, &["init", "-b", "trunk"]);
        run(&dir, &["config", "user.email", "test@gitc.dev"]);
        run(&dir, &["config", "user.name", "Test User"]);
        run(&dir, &["config", "commit.gpgsign", "false"]);
        write_file(&dir, "a.txt", "one\n");
        commit_all(&dir, "init");

        assert_eq!(default_base_branch(&dir).unwrap(), "trunk");

        fs::remove_dir_all(&dir).ok();
    }
}
