// Stream F3 — GitHub clone browser via `gh`.
// Stub scaffolding only; see PLAN.md section 4 for the full design. Bodies
// are filled in by the `feat/gh-clone` stream. Note: these commands take no
// `AppState` — no secrets, no OAuth.
use super::*;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GhStatus {
    pub installed: bool,
    pub authenticated: bool,
    pub login: Option<String>,
    pub host: String,
    pub protocol: String, // "https" | "ssh"
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GhRepo {
    pub name: String,
    pub name_with_owner: String,
    pub owner: String,
    pub description: Option<String>,
    pub is_private: bool,
    pub is_fork: bool,
    pub is_archived: bool,
    pub pushed_at: Option<String>,
    pub url: String,
    pub ssh_url: String,
    pub language: Option<String>,
    pub default_branch: Option<String>,
}

#[tauri::command(async)]
pub fn gh_status() -> Result<GhStatus, String> {
    Err("not implemented".to_string())
}

#[tauri::command(async)]
pub fn gh_repo_list(owner: Option<String>, limit: Option<u32>) -> Result<Vec<GhRepo>, String> {
    let _ = (owner, limit);
    Err("not implemented".to_string())
}

#[allow(unused_imports)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;
}
