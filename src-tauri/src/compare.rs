// Stream F2 — Ref-to-ref compare.
// Stub scaffolding only; see PLAN.md section 3 for the full design. Bodies
// are filled in by the `feat/compare` stream.
use super::*;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefCompare {
    pub base: String,
    pub head: String,
    pub merge_base: Option<String>,
    pub three_dot: bool,
    pub ahead: u32, // rev-list --left-right --count base...head
    pub behind: u32,
    pub files: Vec<CommitFileChange>,
    pub commits: Vec<CommitNode>, // git log base..head, newest first
    pub commits_truncated: bool,
}

#[tauri::command(async)]
pub fn get_ref_compare(
    state: State<'_, AppState>,
    base: Option<String>,
    head: String,
    three_dot: bool,
) -> Result<RefCompare, String> {
    let _ = (state, base, head, three_dot);
    Err("not implemented".to_string())
}

#[tauri::command(async)]
pub fn get_ref_file_diff(
    state: State<'_, AppState>,
    base: Option<String>,
    head: String,
    path: String,
    three_dot: bool,
) -> Result<FileDiff, String> {
    let _ = (state, base, head, path, three_dot);
    Err("not implemented".to_string())
}

#[allow(unused_imports)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;
}
