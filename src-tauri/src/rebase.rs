// Stream F4 — Rebase UX (interactive rebase, narrow first cut).
// Stub scaffolding only; see PLAN.md section 5 for the full design. Bodies
// are filled in incrementally by the `feat/rebase` stream (see the ship
// order in this stream's task notes: the `rebaseContinue` editor fix lands
// first, then discoverability + plan viewing, then interactive rebase).
use super::*;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // fields are populated by Deserialize, not yet read until the next commit in this stream
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
    let _ = (state, base);
    Err("not implemented".to_string())
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

#[cfg(test)]
mod tests {
    use super::*;
    // Explicit (non-glob) imports — a glob import of `test_support::*` would
    // collide with the crate's own `pub fn run()` (the Tauri entry point),
    // which `use super::*` above already brings into scope.
    use crate::test_support::{act, commit_all, head_subject, ok_action, run, write_file, TempRepo};

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
