// Stream F1 — Branch & worktree cleanup.
// Stub scaffolding only; see PLAN.md section 2 for the full design. Bodies
// are filled in by the `feat/cleanup` stream.
use super::*;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchAudit {
    pub name: String,
    pub current: bool,
    pub is_base: bool,
    pub upstream: Option<String>,
    pub upstream_gone: bool,
    pub ahead: u32,
    pub behind: u32, // vs upstream
    pub ahead_of_base: u32,
    pub behind_base: u32, // vs base (three-dot counts)
    pub merged: bool,
    pub squash_merged: bool,
    pub stale: bool,
    pub last_commit_unix: i64,
    pub last_commit_relative: String,
    pub worktree_path: Option<String>, // checked out in a linked worktree
    pub classification: String, // "current" | "base" | "merged" | "squashMerged" | "gone" | "stale" | "active"
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchCleanupReport {
    pub base: String,
    pub stale_days: u32,
    pub branches: Vec<BranchAudit>,
}

#[tauri::command]
pub fn get_branch_cleanup(
    state: State<'_, AppState>,
    base: Option<String>,
    stale_days: Option<u32>,
) -> Result<BranchCleanupReport, String> {
    let _ = (state, base, stale_days);
    Err("not implemented".to_string())
}

#[allow(unused_imports)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;
}
