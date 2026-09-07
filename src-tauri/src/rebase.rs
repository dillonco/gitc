// Stream F4 — Rebase UX (interactive rebase, narrow first cut).
// Stub scaffolding only; see PLAN.md section 5 for the full design. Bodies
// are filled in by the `feat/rebase` stream.
use super::*;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // fields are populated by Deserialize, not yet read until F4 lands
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

#[allow(unused_imports)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;
}
