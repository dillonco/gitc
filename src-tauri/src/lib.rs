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
    worktrees: Vec<String>,
    stashes: Vec<StashEntry>,
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
    let current_branch = git_optional(&root, &["branch", "--show-current"])?;
    let head = git_optional(&root, &["rev-parse", "--short", "HEAD"])?
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
        worktrees: parse_worktrees(&git(&root, &["worktree", "list", "--porcelain"])?),
        stashes: parse_stashes(&git(&root, &["stash", "list"])?),
    })
}

#[tauri::command]
fn get_commit_graph(state: State<'_, AppState>, limit: usize) -> Result<CommitGraph, String> {
    let root = active_repo(&state)?;
    let limit = limit.clamp(25, 1000).to_string();
    let out = git(
        &root,
        &[
            "log",
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
fn run_git_action(state: State<'_, AppState>, action: GitAction) -> Result<GitResult, String> {
    let root = active_repo(&state)?;
    let args = action_args(&action)?;
    Ok(run_git(&root, &args))
}

#[tauri::command]
fn get_conflict_file(state: State<'_, AppState>, path: String) -> Result<ConflictFile, String> {
    let root = active_repo(&state)?;
    validate_repo_relative_path(&path)?;
    let working = fs::read_to_string(root.join(&path)).map_err(|err| err.to_string())?;

    Ok(ConflictFile {
        path: path.clone(),
        base: git_optional(&root, &["show", &format!(":1:{path}")])?,
        ours: git_optional(&root, &["show", &format!(":2:{path}")])?,
        theirs: git_optional(&root, &["show", &format!(":3:{path}")])?,
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
        "commit" => vec!["commit", "-m", required(message, "message")?],
        "commitAmend" => vec!["commit", "--amend", "-m", required(message, "message")?],
        "checkoutBranch" => vec!["checkout", required(branch, "branch")?],
        "createBranch" => vec!["checkout", "-b", required(branch, "branch")?],
        "fetch" => vec!["fetch", remote],
        "pull" => vec!["pull", "--ff-only", remote],
        "push" => vec!["push", remote, "HEAD"],
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
                    let path = parts[8..].join(" ");
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
