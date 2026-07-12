use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use tauri::State;

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
    worktrees: Vec<String>,
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
        worktrees: parse_worktrees(&git(&root, &["worktree", "list", "--porcelain"])?),
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
    let args = action_args(&action)?;
    Ok(run_git(&root, &args))
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
    validate_repo_relative_path(&path)?;
    let diff = if staged {
        git_optional(&root, &["diff", "--cached", "--", &path])?.unwrap_or_default()
    } else {
        git_optional(&root, &["diff", "--", &path])?.unwrap_or_default()
    };

    let diff = if diff.is_empty() && !staged && root.join(&path).is_file() {
        render_untracked_file_diff(&root, &path)?
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
    validate_repo_relative_path(&path)?;
    if staged {
        git_optional(&root, &["show", &format!(":{path}")])
            .map(|content| content.unwrap_or_default())
    } else {
        fs::read_to_string(root.join(path)).map_err(|err| err.to_string())
    }
}

#[tauri::command]
fn get_file_blame(state: State<'_, AppState>, path: String) -> Result<String, String> {
    let root = active_repo(&state)?;
    validate_repo_relative_path(&path)?;
    git(&root, &["blame", "--date=short", "--", &path])
}

#[tauri::command]
fn get_file_history(state: State<'_, AppState>, path: String) -> Result<String, String> {
    let root = active_repo(&state)?;
    validate_repo_relative_path(&path)?;
    git(
        &root,
        &[
            "log",
            "--date=relative",
            "--format=%h  %ar  %an  %s",
            "--",
            &path,
        ],
    )
}

#[tauri::command]
fn apply_hunk(state: State<'_, AppState>, patch: String, mode: String) -> Result<GitResult, String> {
    let root = active_repo(&state)?;
    let args: Vec<&str> = match mode.as_str() {
        "stage" => vec!["apply", "--cached", "-"],
        "unstage" => vec!["apply", "--cached", "--reverse", "-"],
        "discard" => vec!["apply", "--reverse", "-"],
        other => return Err(format!("unsupported hunk mode: {other}")),
    };
    Ok(run_git_with_stdin(&root, &args, &patch))
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
    let result = run_git(parent, &["clone", &url, target_arg]);
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
            save_conflict_resolution
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
    run_command(Command::new("git").args(args).current_dir(root), true)
}

fn run_command(command: &mut Command, refresh: bool) -> GitResult {
    match command.output() {
        Ok(output) => GitResult {
            ok: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            code: output.status.code().unwrap_or(-1),
            refresh,
        },
        Err(err) => GitResult {
            ok: false,
            stdout: String::new(),
            stderr: err.to_string(),
            code: -1,
            refresh: false,
        },
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

    match spawn {
        Ok(mut child) => {
            if let Some(mut child_stdin) = child.stdin.take() {
                if let Err(err) = child_stdin.write_all(stdin.as_bytes()) {
                    return GitResult {
                        ok: false,
                        stdout: String::new(),
                        stderr: err.to_string(),
                        code: -1,
                        refresh: false,
                    };
                }
            }
            match child.wait_with_output() {
                Ok(output) => GitResult {
                    ok: output.status.success(),
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                    code: output.status.code().unwrap_or(-1),
                    refresh: true,
                },
                Err(err) => GitResult {
                    ok: false,
                    stdout: String::new(),
                    stderr: err.to_string(),
                    code: -1,
                    refresh: false,
                },
            }
        }
        Err(err) => GitResult {
            ok: false,
            stdout: String::new(),
            stderr: err.to_string(),
            code: -1,
            refresh: false,
        },
    }
}

fn action_args(action: &GitAction) -> Result<Vec<&str>, String> {
    let path = action.path.as_deref();
    let branch = action.branch.as_deref();
    let target = action.target.as_deref();
    let remote = action.remote.as_deref().unwrap_or("origin");
    let message = action.message.as_deref();

    let args = match action.kind.as_str() {
        "stage" => vec!["add", required(path, "path")?],
        "unstage" => vec!["restore", "--staged", required(path, "path")?],
        "discard" => vec!["restore", required(path, "path")?],
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
        "mergeContinue" => vec!["merge", "--continue"],
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
        "markResolved" => vec!["add", required(path, "path")?],
        unknown => return Err(format!("unknown git action: {unknown}")),
    };

    if let Some(path) = path {
        validate_repo_relative_path(path)?;
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

fn parse_worktrees(out: &str) -> Vec<String> {
    out.lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .map(ToOwned::to_owned)
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
}

// End-to-end tests that drive the real git CLI in throwaway repositories.
#[cfg(test)]
mod git_integration_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static REPO_COUNTER: AtomicUsize = AtomicUsize::new(0);

    struct TempRepo(PathBuf);

    impl TempRepo {
        fn new() -> Self {
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

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempRepo {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).ok();
        }
    }

    fn run(root: &Path, args: &[&str]) -> GitResult {
        let result = run_git(root, args);
        assert!(result.ok, "git {args:?} failed: {}", result.stderr);
        result
    }

    fn write_file(root: &Path, name: &str, content: &str) {
        fs::write(root.join(name), content).expect("write file");
    }

    fn commit_all(root: &Path, message: &str) {
        run(root, &["add", "-A"]);
        run(root, &["commit", "-m", message]);
    }

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
}
