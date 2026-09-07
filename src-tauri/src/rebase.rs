// Stream F4 — Rebase UX (interactive rebase, narrow first cut).
// See PLAN.md section 5, REVIEW-PERF.md, and REVIEW-UX.md section 6 for the
// full design this implements incrementally (see the ship order in this
// stream's task notes: the `rebaseContinue` editor fix landed first; this
// commit adds discoverability and plan viewing; interactive rebase execution
// itself lands last).
//
// One deliberate deviation from the literal PLAN.md text: the "is a
// rebase/merge already in progress" check below uses the *per-worktree* git
// dir (`git rev-parse --absolute-git-dir`), not `root.join(".git")`. In a
// linked worktree the latter is a *file*, not a directory, so checks against
// it are silently always-false. Using the correct git dir avoids that trap.
use super::*;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // fields are populated by Deserialize, not yet read until interactive rebase execution lands
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
    let _ = (state, base, steps);
    Err("not implemented".to_string())
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

#[cfg(test)]
mod tests {
    use super::*;
    // Explicit (non-glob) imports — a glob import of `test_support::*` would
    // collide with the crate's own `pub fn run()` (the Tauri entry point),
    // which `use super::*` above already brings into scope.
    use crate::test_support::{act, commit_all, head_subject, ok_action, run, write_file, TempRepo};

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

    // ---- ship-order item 1: `rebaseContinue` must not try to launch an editor ----
    //
    // In the packaged app there is no TTY, so `git rebase --continue` after a
    // conflict on any message-bearing step used to try to launch `vi` and
    // hang. `run_action` in lib.rs now routes `rebaseContinue` (and
    // `mergeContinue`) through `run_git_env` with `GIT_EDITOR=true`.
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
}
