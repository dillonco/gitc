// Stream F4 — Rebase UX (interactive rebase, narrow first cut).
// See PLAN.md section 5, REVIEW-PERF.md, and REVIEW-UX.md section 6 for the
// full design this implements.
//
// Scope, deliberately narrow: pick / reword / squash / fixup / drop, plus
// reorder. No `edit`, no `break`, no user `exec`, no autosquash, no `--onto`
// with a different upstream, no `rebase.updateRefs`.
//
// Mechanism: a generated todo file plus `GIT_SEQUENCE_EDITOR` pointing at a
// tiny helper script that just copies the todo into place, and `GIT_EDITOR`
// forced to `true` so nothing tries to open an interactive editor (there is
// no TTY in the packaged app). Message files for `reword` and message-bearing
// `squash` steps live under the git dir (not a temp dir) so they survive a
// conflict stop; they are cleaned at the *start* of the next run, once we've
// confirmed no rebase/merge is currently in progress.
//
// One deliberate deviation from the literal PLAN.md text: the "is a
// rebase/merge already in progress" preflight check here uses the
// *per-worktree* git dir (`git rev-parse --absolute-git-dir`), not
// `root.join(".git")`. In a linked worktree the latter is a *file*, not a
// directory, so checks against it are silently always-false. Using the
// correct git dir avoids that trap for every check this module makes, not
// just message-file placement.
use super::*;
use std::collections::HashSet;

const MAX_REBASE_STEPS: usize = 500;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RebaseStep {
    pub action: String,
    pub hash: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RebasePlan {
    pub base: String,
    pub merge_base: Option<String>,
    pub commits: Vec<CommitNode>, // oldest first
    pub clean: bool,
    pub in_progress: bool,
    pub current_branch: Option<String>,
    pub upstream: Option<String>,
}

#[tauri::command(async)]
pub fn get_rebase_plan(state: State<'_, AppState>, base: Option<String>) -> Result<RebasePlan, String> {
    let root = active_repo(&state)?;
    rebase_plan(&root, base.as_deref())
}

#[tauri::command(async)]
pub fn run_interactive_rebase(
    state: State<'_, AppState>,
    base: String,
    steps: Vec<RebaseStep>,
) -> Result<GitResult, String> {
    let root = active_repo(&state)?;
    interactive_rebase(&root, &base, &steps)
}

/// The per-worktree git dir — `git rev-parse --absolute-git-dir`. Differs
/// from `root.join(".git")` inside a linked worktree, where `.git` is a file
/// pointing at `main/.git/worktrees/<name>`, not a directory.
fn worktree_git_dir(root: &Path) -> Result<PathBuf, String> {
    git(root, &["rev-parse", "--absolute-git-dir"]).map(PathBuf::from)
}

fn rebase_or_merge_in_progress(git_dir: &Path) -> bool {
    git_dir.join("MERGE_HEAD").exists()
        || git_dir.join("rebase-merge").exists()
        || git_dir.join("rebase-apply").exists()
}

pub(crate) fn rebase_plan(root: &Path, base: Option<&str>) -> Result<RebasePlan, String> {
    let base = match base {
        Some(value) if !value.trim().is_empty() => value.to_string(),
        _ => default_base_branch(root)?,
    };
    validate_ref_arg(&base)?;

    let merge_base = git_optional(root, &["merge-base", &base, "HEAD"])?;

    let range = format!("{base}..HEAD");
    // Cap well above the interactive-rebase step limit so a mis-chosen base
    // (e.g. an unrelated branch) fails validation cheaply instead of
    // serialising thousands of commits just to display an empty-ish plan.
    let out = git(
        root,
        &[
            "log",
            "--reverse",
            "--format=%H%x1f%P%x1f%D%x1f%an%x1f%ar%x1f%s%x1f%b%x1e",
            "-n",
            "501",
            &range,
        ],
    )?;
    let commits = parse_commit_graph(&out);

    let status = git(root, &["status", "--porcelain"])?;
    let clean = status.trim().is_empty();

    let git_dir = worktree_git_dir(root)?;
    let in_progress = rebase_or_merge_in_progress(&git_dir);

    let current_branch = git_optional(root, &["branch", "--show-current"])?;
    let upstream = match current_branch.as_deref() {
        Some(branch) => git_optional(root, &["rev-parse", "--abbrev-ref", &format!("{branch}@{{upstream}}")])?,
        None => None,
    };

    Ok(RebasePlan {
        base,
        merge_base,
        commits,
        clean,
        in_progress,
        current_branch,
        upstream,
    })
}

pub(crate) fn interactive_rebase(root: &Path, base: &str, steps: &[RebaseStep]) -> Result<GitResult, String> {
    validate_ref_arg(base)?;

    let git_dir = worktree_git_dir(root)?;
    if rebase_or_merge_in_progress(&git_dir) {
        return Err("a rebase or merge is already in progress".to_string());
    }

    let status = git(root, &["status", "--porcelain"])?;
    if !status.trim().is_empty() {
        return Err("commit or stash your changes before rebasing".to_string());
    }

    let range = format!("{base}..HEAD");
    let expected: Vec<String> = git(root, &["rev-list", "--reverse", &range])?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    if expected.is_empty() {
        return Err(format!("there are no commits between {base} and HEAD"));
    }
    validate_steps(steps, &expected)?;

    // Message files must live under the git dir (not a temp dir) so they
    // survive a conflict stop; clean up anything left from a previous run —
    // safe now, since the checks above already confirmed nothing is in
    // progress.
    let msg_dir = git_dir.join("gitc-rebase");
    fs::remove_dir_all(&msg_dir).ok();
    fs::create_dir_all(&msg_dir).map_err(|err| err.to_string())?;

    let todo = build_todo(steps, &msg_dir)?;
    let todo_path = msg_dir.join("todo");
    fs::write(&todo_path, &todo).map_err(|err| err.to_string())?;

    let script_path = msg_dir.join("sequence-editor.sh");
    fs::write(&script_path, "#!/bin/sh\ncp \"$GITC_REBASE_TODO\" \"$1\"\n").map_err(|err| err.to_string())?;
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).map_err(|err| err.to_string())?;
    }

    // Repo paths with spaces are common on macOS, and git argv-splits an
    // unquoted `GIT_SEQUENCE_EDITOR` string — single-quote the script path.
    let script_arg = shell_single_quote(&script_path.to_string_lossy());
    let todo_arg = todo_path.to_string_lossy().into_owned();

    Ok(run_git_env(
        root,
        &["rebase", "-i", "--no-autosquash", base],
        &[
            ("GIT_SEQUENCE_EDITOR", script_arg.as_str()),
            ("GITC_REBASE_TODO", todo_arg.as_str()),
            ("GIT_EDITOR", "true"),
        ],
    ))
}

/// Validate a client-submitted plan against the server's own view of what
/// commits actually sit between `base` and `HEAD`. Pure.
pub(crate) fn validate_steps(steps: &[RebaseStep], expected: &[String]) -> Result<(), String> {
    if steps.is_empty() {
        return Err("rebase plan is empty".to_string());
    }
    if steps.len() > MAX_REBASE_STEPS {
        return Err(format!("rebase plan has too many steps (max {MAX_REBASE_STEPS})"));
    }
    if steps.len() != expected.len() {
        return Err(
            "rebase plan must include every commit between the base and HEAD exactly once (drops must be explicit)"
                .to_string(),
        );
    }

    let expected_set: HashSet<&str> = expected.iter().map(String::as_str).collect();
    let mut seen: HashSet<&str> = HashSet::new();
    for step in steps {
        if !expected_set.contains(step.hash.as_str()) {
            return Err(format!("commit {} is not part of this rebase", step.hash));
        }
        if !seen.insert(step.hash.as_str()) {
            return Err(format!("commit {} appears more than once in the rebase plan", step.hash));
        }
        match step.action.as_str() {
            "pick" | "squash" | "fixup" | "drop" => {}
            "reword" => {
                if step.message.as_deref().map(str::trim).unwrap_or("").is_empty() {
                    return Err(format!("reword for commit {} needs a message", step.hash));
                }
            }
            other => return Err(format!("unsupported rebase action: {other}")),
        }
    }
    if seen.len() != expected_set.len() {
        return Err("rebase plan is missing one or more commits (drops must be explicit)".to_string());
    }

    if let Some(first_live) = steps.iter().find(|step| step.action != "drop") {
        if matches!(first_live.action.as_str(), "squash" | "fixup") {
            return Err(
                "the first commit in a rebase can't be squashed or fixed up — there is nothing before it to fold into"
                    .to_string(),
            );
        }
    }

    Ok(())
}

fn write_step_message(msg_dir: &Path, index: usize, message: &str) -> Result<PathBuf, String> {
    let path = msg_dir.join(format!("{index}.msg"));
    fs::write(&path, message).map_err(|err| err.to_string())?;
    Ok(path)
}

/// Build the `git rebase -i` todo file contents, writing any message files
/// (for `reword`, and `squash` with a custom message) into `msg_dir` as it
/// goes. `steps` is oldest-first, matching the todo file's own top-to-bottom
/// application order.
pub(crate) fn build_todo(steps: &[RebaseStep], msg_dir: &Path) -> Result<String, String> {
    let mut todo = String::new();
    for (index, step) in steps.iter().enumerate() {
        match step.action.as_str() {
            "drop" => todo.push_str(&format!("drop {}\n", step.hash)),
            "pick" => todo.push_str(&format!("pick {}\n", step.hash)),
            "fixup" => todo.push_str(&format!("fixup {}\n", step.hash)),
            "squash" => {
                todo.push_str(&format!("squash {}\n", step.hash));
                if let Some(message) = step.message.as_deref().filter(|value| !value.trim().is_empty()) {
                    let msg_path = write_step_message(msg_dir, index, message)?;
                    todo.push_str(&format!(
                        "exec git commit --amend -F {}\n",
                        shell_single_quote(&msg_path.to_string_lossy())
                    ));
                }
            }
            "reword" => {
                let message = step.message.as_deref().unwrap_or_default();
                todo.push_str(&format!("pick {}\n", step.hash));
                let msg_path = write_step_message(msg_dir, index, message)?;
                todo.push_str(&format!(
                    "exec git commit --amend -F {}\n",
                    shell_single_quote(&msg_path.to_string_lossy())
                ));
            }
            other => return Err(format!("unsupported rebase action: {other}")),
        }
    }
    Ok(todo)
}

/// POSIX single-quote escaping: close the quote, emit an escaped quote,
/// reopen it — `it's` -> `'it'\''s'`.
pub(crate) fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    // Explicit (non-glob) imports — a glob import of `test_support::*` would
    // collide with the crate's own `pub fn run()` (the Tauri entry point),
    // which `use super::*` above already brings into scope.
    use crate::test_support::{act, commit_all, head_subject, ok_action, run, write_file, TempRepo, REPO_COUNTER};
    use std::sync::atomic::Ordering;

    fn step(action: &str, hash: &str, message: Option<&str>) -> RebaseStep {
        RebaseStep {
            action: action.to_string(),
            hash: hash.to_string(),
            message: message.map(ToOwned::to_owned),
        }
    }

    // ---- pure helpers ----

    #[test]
    fn shell_single_quote_escapes_embedded_quotes() {
        assert_eq!(shell_single_quote("simple"), "'simple'");
        assert_eq!(shell_single_quote("a b"), "'a b'");
        assert_eq!(shell_single_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn validate_steps_accepts_a_full_reorder_and_reword_plan() {
        let expected = vec!["h1".to_string(), "h2".to_string(), "h3".to_string()];
        let steps = vec![
            step("pick", "h2", None),
            step("reword", "h1", Some("new subject")),
            step("drop", "h3", None),
        ];
        assert!(validate_steps(&steps, &expected).is_ok());
    }

    #[test]
    fn validate_steps_rejects_unknown_hash() {
        let expected = vec!["h1".to_string()];
        let steps = vec![step("pick", "h9", None)];
        assert!(validate_steps(&steps, &expected).is_err());
    }

    #[test]
    fn validate_steps_rejects_duplicate_hash() {
        let expected = vec!["h1".to_string(), "h2".to_string()];
        let steps = vec![step("pick", "h1", None), step("pick", "h1", None)];
        assert!(validate_steps(&steps, &expected).is_err());
    }

    #[test]
    fn validate_steps_rejects_a_missing_commit() {
        let expected = vec!["h1".to_string(), "h2".to_string()];
        let steps = vec![step("pick", "h1", None)];
        assert!(validate_steps(&steps, &expected).is_err());
    }

    #[test]
    fn validate_steps_rejects_squash_as_the_first_live_step() {
        let expected = vec!["h1".to_string(), "h2".to_string()];
        let steps = vec![step("squash", "h1", None), step("pick", "h2", None)];
        assert!(validate_steps(&steps, &expected).is_err());
    }

    #[test]
    fn validate_steps_rejects_squash_that_becomes_first_live_after_a_leading_drop() {
        let expected = vec!["h1".to_string(), "h2".to_string()];
        let steps = vec![step("drop", "h1", None), step("squash", "h2", None)];
        assert!(validate_steps(&steps, &expected).is_err());
    }

    #[test]
    fn validate_steps_rejects_reword_without_message() {
        let expected = vec!["h1".to_string()];
        let steps = vec![step("reword", "h1", None)];
        assert!(validate_steps(&steps, &expected).is_err());
        let blank = vec![step("reword", "h1", Some("   "))];
        assert!(validate_steps(&blank, &expected).is_err());
    }

    #[test]
    fn validate_steps_rejects_an_unknown_action() {
        let expected = vec!["h1".to_string()];
        let steps = vec![step("edit", "h1", None)];
        assert!(validate_steps(&steps, &expected).is_err());
    }

    #[test]
    fn validate_steps_rejects_more_than_the_cap() {
        let expected: Vec<String> = (0..(MAX_REBASE_STEPS + 1)).map(|i| format!("h{i}")).collect();
        let steps: Vec<RebaseStep> = expected.iter().map(|hash| step("pick", hash, None)).collect();
        assert!(validate_steps(&steps, &expected).is_err());
    }

    #[test]
    fn build_todo_writes_message_files_and_exec_lines() {
        let dir = tempfile::tempdir().unwrap();
        let steps = vec![
            step("pick", "aaa111", None),
            step("reword", "bbb222", Some("new subject\n\nnew body")),
            step("squash", "ccc333", Some("combined message")),
            step("fixup", "ddd444", None),
            step("drop", "eee555", None),
        ];
        let todo = build_todo(&steps, dir.path()).unwrap();
        let lines: Vec<&str> = todo.lines().collect();

        assert_eq!(lines[0], "pick aaa111");
        assert_eq!(lines[1], "pick bbb222");
        assert!(lines[2].starts_with("exec git commit --amend -F "));
        assert!(lines[2].contains("1.msg"));
        assert_eq!(lines[3], "squash ccc333");
        assert!(lines[4].starts_with("exec git commit --amend -F "));
        assert!(lines[4].contains("2.msg"));
        assert_eq!(lines[5], "fixup ddd444");
        assert_eq!(lines[6], "drop eee555");
        assert_eq!(lines.len(), 7);

        assert_eq!(
            fs::read_to_string(dir.path().join("1.msg")).unwrap(),
            "new subject\n\nnew body"
        );
        assert_eq!(fs::read_to_string(dir.path().join("2.msg")).unwrap(), "combined message");
    }

    #[test]
    fn build_todo_squash_without_a_message_has_no_exec_line() {
        let dir = tempfile::tempdir().unwrap();
        let steps = vec![step("pick", "aaa111", None), step("squash", "bbb222", None)];
        let todo = build_todo(&steps, dir.path()).unwrap();
        assert_eq!(todo, "pick aaa111\nsquash bbb222\n");
    }

    // ---- rebase_plan ----

    #[test]
    fn rebase_plan_lists_commits_oldest_first_and_reports_metadata() {
        let repo = TempRepo::new();
        write_file(repo.path(), "base.txt", "base\n");
        commit_all(repo.path(), "base");
        run(repo.path(), &["checkout", "-b", "feature"]);
        write_file(repo.path(), "a.txt", "a\n");
        commit_all(repo.path(), "first");
        write_file(repo.path(), "b.txt", "b\n");
        commit_all(repo.path(), "second");

        let plan = rebase_plan(repo.path(), Some("main")).unwrap();
        assert_eq!(plan.base, "main");
        assert_eq!(plan.commits.len(), 2);
        assert_eq!(plan.commits[0].subject, "first");
        assert_eq!(plan.commits[1].subject, "second");
        assert!(plan.clean);
        assert!(!plan.in_progress);
        assert_eq!(plan.current_branch.as_deref(), Some("feature"));
        assert!(plan.upstream.is_none());
    }

    #[test]
    fn rebase_plan_is_empty_but_not_an_error_when_up_to_date() {
        let repo = TempRepo::new();
        write_file(repo.path(), "base.txt", "base\n");
        commit_all(repo.path(), "base");

        let plan = rebase_plan(repo.path(), Some("main")).unwrap();
        assert!(plan.commits.is_empty());
        assert!(plan.clean);
    }

    // ---- interactive_rebase: the five actions + reorder ----

    fn feature_over_main_with_two_commits() -> (TempRepo, Vec<String>) {
        let repo = TempRepo::new();
        write_file(repo.path(), "base.txt", "base\n");
        commit_all(repo.path(), "base");
        run(repo.path(), &["checkout", "-b", "feature"]);
        write_file(repo.path(), "a.txt", "a\n");
        commit_all(repo.path(), "first");
        write_file(repo.path(), "b.txt", "b\n");
        commit_all(repo.path(), "second");
        let hashes = git(repo.path(), &["rev-list", "--reverse", "main..HEAD"])
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect();
        (repo, hashes)
    }

    #[test]
    fn interactive_rebase_reorders_and_rewords() {
        let (repo, hashes) = feature_over_main_with_two_commits();
        assert_eq!(hashes.len(), 2);

        let steps = vec![
            step("reword", &hashes[1], Some("second (reworded)")),
            step("pick", &hashes[0], None),
        ];
        let result = interactive_rebase(repo.path(), "main", &steps).unwrap();
        assert!(result.ok, "rebase failed: {}", result.stderr);

        let subjects = git(repo.path(), &["log", "--format=%s"]).unwrap();
        assert_eq!(
            subjects.lines().collect::<Vec<_>>(),
            vec!["first", "second (reworded)", "base"]
        );
    }

    #[test]
    fn interactive_rebase_squashes_with_a_custom_message() {
        let (repo, hashes) = feature_over_main_with_two_commits();

        let steps = vec![step("pick", &hashes[0], None), step("squash", &hashes[1], Some("combined work"))];
        let result = interactive_rebase(repo.path(), "main", &steps).unwrap();
        assert!(result.ok, "rebase failed: {}", result.stderr);

        let subjects = git(repo.path(), &["log", "--format=%s"]).unwrap();
        assert_eq!(subjects.lines().collect::<Vec<_>>(), vec!["combined work", "base"]);
        assert!(repo.path().join("a.txt").exists());
        assert!(repo.path().join("b.txt").exists());
    }

    #[test]
    fn interactive_rebase_fixup_keeps_the_earlier_message() {
        let repo = TempRepo::new();
        write_file(repo.path(), "base.txt", "base\n");
        commit_all(repo.path(), "base");
        run(repo.path(), &["checkout", "-b", "feature"]);
        write_file(repo.path(), "a.txt", "a\n");
        commit_all(repo.path(), "keep this message");
        write_file(repo.path(), "b.txt", "b\n");
        commit_all(repo.path(), "fixup me");
        let hashes: Vec<String> = git(repo.path(), &["rev-list", "--reverse", "main..HEAD"])
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect();

        let steps = vec![step("pick", &hashes[0], None), step("fixup", &hashes[1], None)];
        let result = interactive_rebase(repo.path(), "main", &steps).unwrap();
        assert!(result.ok, "rebase failed: {}", result.stderr);

        let subjects = git(repo.path(), &["log", "--format=%s"]).unwrap();
        assert_eq!(subjects.lines().collect::<Vec<_>>(), vec!["keep this message", "base"]);
        assert!(repo.path().join("a.txt").exists());
        assert!(repo.path().join("b.txt").exists());
    }

    #[test]
    fn interactive_rebase_drop_removes_the_commit_and_its_file() {
        let (repo, hashes) = feature_over_main_with_two_commits();

        let steps = vec![step("drop", &hashes[0], None), step("pick", &hashes[1], None)];
        let result = interactive_rebase(repo.path(), "main", &steps).unwrap();
        assert!(result.ok, "rebase failed: {}", result.stderr);

        let subjects = git(repo.path(), &["log", "--format=%s"]).unwrap();
        assert_eq!(subjects, "second\nbase");
        assert!(!repo.path().join("a.txt").exists());
        assert!(repo.path().join("b.txt").exists());
    }

    #[test]
    fn interactive_rebase_rejects_a_dirty_tree() {
        let (repo, hashes) = feature_over_main_with_two_commits();
        write_file(repo.path(), "a.txt", "dirty\n");

        let steps = vec![step("pick", &hashes[0], None), step("pick", &hashes[1], None)];
        let err = interactive_rebase(repo.path(), "main", &steps).unwrap_err();
        assert!(err.contains("stash"), "error should mention stashing: {err}");
    }

    #[test]
    fn interactive_rebase_conflict_can_be_aborted_back_to_the_original_state() {
        let repo = TempRepo::new();
        write_file(repo.path(), "file.txt", "base\n");
        commit_all(repo.path(), "base");
        run(repo.path(), &["checkout", "-b", "feature"]);
        write_file(repo.path(), "file.txt", "feature change\n");
        commit_all(repo.path(), "feature work");
        let original_head = git(repo.path(), &["rev-parse", "HEAD"]).unwrap();

        run(repo.path(), &["checkout", "main"]);
        write_file(repo.path(), "file.txt", "main change\n");
        commit_all(repo.path(), "main work");
        run(repo.path(), &["checkout", "feature"]);

        let hashes: Vec<String> = git(repo.path(), &["rev-list", "--reverse", "main..HEAD"])
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect();
        assert_eq!(hashes.len(), 1);

        let steps = vec![step("pick", &hashes[0], None)];
        let result = interactive_rebase(repo.path(), "main", &steps).unwrap();
        assert!(!result.ok, "rebase should conflict");
        assert!(repository_state(repo.path()).unwrap().rebasing);

        ok_action(repo.path(), &act("rebaseAbort"));

        assert!(!repository_state(repo.path()).unwrap().rebasing);
        let head = git(repo.path(), &["rev-parse", "HEAD"]).unwrap();
        assert_eq!(head, original_head);
        assert_eq!(
            fs::read_to_string(repo.path().join("file.txt")).unwrap(),
            "feature change\n"
        );
    }

    #[test]
    fn interactive_rebase_rejects_starting_a_second_run_while_one_is_in_progress() {
        let repo = TempRepo::new();
        write_file(repo.path(), "file.txt", "base\n");
        commit_all(repo.path(), "base");
        run(repo.path(), &["checkout", "-b", "feature"]);
        write_file(repo.path(), "file.txt", "feature change\n");
        commit_all(repo.path(), "feature work");
        run(repo.path(), &["checkout", "main"]);
        write_file(repo.path(), "file.txt", "main change\n");
        commit_all(repo.path(), "main work");
        run(repo.path(), &["checkout", "feature"]);

        let hashes: Vec<String> = git(repo.path(), &["rev-list", "--reverse", "main..HEAD"])
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect();
        let steps = vec![step("pick", &hashes[0], None)];
        assert!(!interactive_rebase(repo.path(), "main", &steps).unwrap().ok);

        let retry = interactive_rebase(repo.path(), "main", &steps);
        assert!(retry.is_err());
        assert!(retry.unwrap_err().contains("in progress"));

        ok_action(repo.path(), &act("rebaseAbort"));
    }

    // ---- ship-order item 1: `rebaseContinue` must not try to launch an editor ----

    #[test]
    fn rebase_continue_does_not_hang_on_an_editor_after_conflict_resolution() {
        let repo = TempRepo::new();
        write_file(repo.path(), "file.txt", "base\n");
        commit_all(repo.path(), "base");
        run(repo.path(), &["checkout", "-b", "feature"]);
        write_file(repo.path(), "file.txt", "feature change\n");
        commit_all(repo.path(), "feature work");
        // A second, message-bearing commit so `rebase --continue` has to
        // replay another pick after the conflict — exactly the path that
        // used to try to launch `vi` with no TTY attached.
        write_file(repo.path(), "other.txt", "more\n");
        commit_all(repo.path(), "more feature work");
        run(repo.path(), &["checkout", "main"]);
        write_file(repo.path(), "file.txt", "main change\n");
        commit_all(repo.path(), "main work");
        run(repo.path(), &["checkout", "feature"]);

        let mut rebase = act("rebase");
        rebase.target = Some("main".to_string());
        let result = run_action(repo.path(), &rebase).unwrap();
        assert!(!result.ok, "rebase should conflict on file.txt");
        assert!(repository_state(repo.path()).unwrap().rebasing);

        fs::write(repo.path().join("file.txt"), "resolved\n").unwrap();
        let mut resolved = act("markResolved");
        resolved.path = Some("file.txt".to_string());
        ok_action(repo.path(), &resolved);

        let cont = act("rebaseContinue");
        let result = run_action(repo.path(), &cont).unwrap();
        assert!(result.ok, "rebase --continue failed: {}", result.stderr);
        assert!(!repository_state(repo.path()).unwrap().rebasing);
        assert_eq!(head_subject(repo.path()), "more feature work");
    }

    // ---- the linked-worktree trap ----

    #[test]
    fn interactive_rebase_works_inside_a_linked_worktree() {
        let repo = TempRepo::new();
        write_file(repo.path(), "base.txt", "base\n");
        commit_all(repo.path(), "base");
        run(repo.path(), &["checkout", "-b", "feature"]);
        write_file(repo.path(), "a.txt", "a\n");
        commit_all(repo.path(), "first");
        write_file(repo.path(), "b.txt", "b\n");
        commit_all(repo.path(), "second");
        run(repo.path(), &["checkout", "main"]);

        let worktree_dir = std::env::temp_dir().join(format!(
            "gitc-rebase-worktree-{}-{}",
            std::process::id(),
            REPO_COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        run(repo.path(), &["worktree", "add", worktree_dir.to_str().unwrap(), "feature"]);

        let hashes: Vec<String> = git(&worktree_dir, &["rev-list", "--reverse", "main..HEAD"])
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect();
        assert_eq!(hashes.len(), 2);

        let steps = vec![
            step("pick", &hashes[0], None),
            step("reword", &hashes[1], Some("second (reworded in worktree)")),
        ];
        let result = interactive_rebase(&worktree_dir, "main", &steps).unwrap();
        assert!(result.ok, "rebase failed: {}", result.stderr);

        let subjects = git(&worktree_dir, &["log", "--format=%s"]).unwrap();
        assert_eq!(
            subjects.lines().collect::<Vec<_>>(),
            vec!["second (reworded in worktree)", "first", "base"]
        );

        // The message/todo files must land under the worktree's own git dir
        // (`.git/worktrees/<name>/...`), not the main repo's `.git` — prove
        // the two resolve differently and that the worktree one looks right.
        let worktree_git_dir = git(&worktree_dir, &["rev-parse", "--absolute-git-dir"]).unwrap();
        let main_git_dir = git(repo.path(), &["rev-parse", "--absolute-git-dir"]).unwrap();
        assert_ne!(worktree_git_dir, main_git_dir, "linked worktree must resolve its own git dir");
        assert!(
            worktree_git_dir.contains("worktrees"),
            "expected a worktrees/ path, got {worktree_git_dir}"
        );

        fs::remove_dir_all(&worktree_dir).ok();
        run(repo.path(), &["worktree", "prune"]);
    }
}
