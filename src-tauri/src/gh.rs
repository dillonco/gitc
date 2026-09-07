// Stream F3 — GitHub clone browser via `gh`.
// See PLAN.md section 4 and REVIEW-PERF.md / REVIEW-UX.md for the full
// design. These commands take no `AppState` — no secrets, no OAuth; we shell
// out to the user's existing `gh` login and nothing more.
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

const GH_HOST: &str = "github.com";

#[tauri::command(async)]
pub fn gh_status() -> Result<GhStatus, String> {
    let Some(gh_path) = resolve_gh() else {
        return Ok(GhStatus {
            installed: false,
            authenticated: false,
            login: None,
            host: GH_HOST.to_string(),
            protocol: "https".to_string(),
            message: Some("GitHub CLI not found; install with `brew install gh`.".to_string()),
        });
    };

    let mut command = Command::new(&gh_path);
    command.args(["auth", "status", "--hostname", GH_HOST]);
    let result = run_command(&mut command, false);
    let combined = format!("{}\n{}", result.stdout, result.stderr);
    let (login, protocol) = parse_auth_status(&combined);

    if result.ok {
        Ok(GhStatus {
            installed: true,
            authenticated: true,
            login,
            host: GH_HOST.to_string(),
            protocol: protocol.unwrap_or_else(|| "https".to_string()),
            message: None,
        })
    } else {
        let detail = result.stderr.trim();
        let detail = if detail.is_empty() { result.stdout.trim() } else { detail };
        let message = if detail.is_empty() {
            "Run `gh auth login` in a terminal.".to_string()
        } else {
            format!("{detail}\nRun `gh auth login` in a terminal.")
        };
        Ok(GhStatus {
            installed: true,
            authenticated: false,
            login: None,
            host: GH_HOST.to_string(),
            protocol: "https".to_string(),
            message: Some(message),
        })
    }
}

#[tauri::command(async)]
pub fn gh_repo_list(owner: Option<String>, limit: Option<u32>) -> Result<Vec<GhRepo>, String> {
    let gh_path = resolve_gh()
        .ok_or_else(|| "GitHub CLI not found; install with `brew install gh`.".to_string())?;
    if let Some(owner) = owner.as_deref() {
        validate_owner(owner)?;
    }
    let limit = limit.unwrap_or(100).clamp(1, 500).to_string();

    let mut args: Vec<String> = vec!["repo".to_string(), "list".to_string()];
    if let Some(owner) = owner.as_deref() {
        args.push(owner.to_string());
    }
    args.push("--limit".to_string());
    args.push(limit);
    args.push("--json".to_string());
    args.push(
        "name,nameWithOwner,owner,description,isPrivate,isFork,isArchived,pushedAt,url,sshUrl,primaryLanguage,defaultBranchRef"
            .to_string(),
    );

    let mut command = Command::new(&gh_path);
    command.args(args.iter().map(String::as_str));
    let result = run_command(&mut command, false);
    if !result.ok {
        return Err(if result.stderr.trim().is_empty() {
            format!("gh repo list failed with exit code {}", result.code)
        } else {
            result.stderr.trim().to_string()
        });
    }
    parse_repo_list(&result.stdout)
}

/// Resolve an absolute path to the `gh` binary without ever spawning a
/// process: a Finder-launched Tauri app gets a minimal PATH (no
/// `/opt/homebrew/bin`), so we walk `$PATH` ourselves first, then fall back
/// to the well-known Homebrew / MacPorts / user-local install locations.
pub(crate) fn resolve_gh() -> Option<PathBuf> {
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join("gh");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    let mut fallbacks: Vec<PathBuf> = vec![
        PathBuf::from("/opt/homebrew/bin/gh"),
        PathBuf::from("/usr/local/bin/gh"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        fallbacks.push(PathBuf::from(home).join(".local/bin/gh"));
    }
    fallbacks.into_iter().find(|candidate| candidate.is_file())
}

/// Pure parse of `gh auth status`'s combined stdout+stderr text into
/// (login, protocol). Returns (None, None) when not authenticated.
pub(crate) fn parse_auth_status(text: &str) -> (Option<String>, Option<String>) {
    let mut login = None;
    let mut protocol = None;
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if login.is_none() {
            if let Some(idx) = line.find("account ") {
                let rest = &line[idx + "account ".len()..];
                login = rest.split_whitespace().next().map(|s| s.to_string());
            }
        }
        if protocol.is_none() {
            if let Some(idx) = line.find("Git operations protocol:") {
                let rest = line[idx + "Git operations protocol:".len()..].trim();
                if !rest.is_empty() {
                    protocol = Some(rest.to_string());
                }
            }
        }
    }
    (login, protocol)
}

/// Owner/org argument to `gh repo list`: non-empty, no leading `-` (so it
/// cannot be misread as a flag), and restricted to characters GitHub allows
/// in a login/org name.
pub(crate) fn validate_owner(owner: &str) -> Result<(), String> {
    if owner.is_empty() {
        return Err("owner must not be empty".to_string());
    }
    if owner.starts_with('-') {
        return Err("owner must not start with '-'".to_string());
    }
    if !owner
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
    {
        return Err("owner may only contain letters, digits, '.', '_', and '-'".to_string());
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct RawOwner {
    login: String,
}

#[derive(Debug, Deserialize)]
struct RawLanguage {
    name: String,
}

#[derive(Debug, Deserialize)]
struct RawBranchRef {
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRepo {
    name: String,
    name_with_owner: String,
    owner: RawOwner,
    #[serde(default)]
    description: Option<String>,
    is_private: bool,
    is_fork: bool,
    is_archived: bool,
    #[serde(default)]
    pushed_at: Option<String>,
    url: String,
    ssh_url: String,
    #[serde(default)]
    primary_language: Option<RawLanguage>,
    #[serde(default)]
    default_branch_ref: Option<RawBranchRef>,
}

/// Pure parse of `gh repo list --json ...`'s stdout.
pub(crate) fn parse_repo_list(json: &str) -> Result<Vec<GhRepo>, String> {
    let raw: Vec<RawRepo> = serde_json::from_str(json).map_err(|err| err.to_string())?;
    Ok(raw
        .into_iter()
        .map(|repo| GhRepo {
            name: repo.name,
            name_with_owner: repo.name_with_owner,
            owner: repo.owner.login,
            // gh prints "" rather than null for a repo with no description.
            description: repo.description.filter(|d| !d.trim().is_empty()),
            is_private: repo.is_private,
            is_fork: repo.is_fork,
            is_archived: repo.is_archived,
            pushed_at: repo.pushed_at,
            url: repo.url,
            ssh_url: repo.ssh_url,
            language: repo.primary_language.map(|lang| lang.name),
            default_branch: repo.default_branch_ref.map(|branch_ref| branch_ref.name),
        })
        .collect())
}

#[allow(unused_imports)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;

    #[test]
    fn parses_authenticated_status() {
        let text = "github.com\n  \u{2713} Logged in to github.com account dillonco (keyring)\n  - Active account: true\n  - Git operations protocol: https\n  - Token: gho_************************************\n  - Token scopes: 'admin:org', 'gist', 'repo', 'workflow'\n";
        let (login, protocol) = parse_auth_status(text);
        assert_eq!(login.as_deref(), Some("dillonco"));
        assert_eq!(protocol.as_deref(), Some("https"));
    }

    #[test]
    fn parses_ssh_protocol_status() {
        let text = "github.com\n  \u{2713} Logged in to github.com account octocat (oauth_token)\n  - Git operations protocol: ssh\n";
        let (login, protocol) = parse_auth_status(text);
        assert_eq!(login.as_deref(), Some("octocat"));
        assert_eq!(protocol.as_deref(), Some("ssh"));
    }

    #[test]
    fn parses_not_logged_in_status() {
        let text = "You are not logged into any GitHub hosts. To log in, run: gh auth login\n";
        let (login, protocol) = parse_auth_status(text);
        assert_eq!(login, None);
        assert_eq!(protocol, None);
    }

    #[test]
    fn parses_repo_list_json_sample() {
        // Verified sample: null primaryLanguage on the first repo, missing
        // defaultBranchRef key entirely on the first repo, empty description.
        let json = r#"[
            {
                "name": "gitc",
                "nameWithOwner": "dillonco/gitc",
                "owner": {"id": "MDQ6VXNlcjE=", "login": "dillonco"},
                "description": "",
                "isPrivate": false,
                "isFork": false,
                "isArchived": false,
                "pushedAt": "2026-09-05T19:24:10Z",
                "url": "https://github.com/dillonco/gitc",
                "sshUrl": "git@github.com:dillonco/gitc.git",
                "primaryLanguage": null
            },
            {
                "name": "forked",
                "nameWithOwner": "someone/forked",
                "owner": {"id": "MDQ6VXNlcjI=", "login": "someone"},
                "description": "A fork",
                "isPrivate": true,
                "isFork": true,
                "isArchived": true,
                "pushedAt": null,
                "url": "https://github.com/someone/forked",
                "sshUrl": "git@github.com:someone/forked.git",
                "primaryLanguage": {"name": "Rust"},
                "defaultBranchRef": {"name": "main"}
            }
        ]"#;
        let repos = parse_repo_list(json).expect("valid json parses");
        assert_eq!(repos.len(), 2);

        assert_eq!(repos[0].name, "gitc");
        assert_eq!(repos[0].owner, "dillonco");
        assert_eq!(repos[0].description, None, "empty string becomes None");
        assert_eq!(repos[0].language, None);
        assert_eq!(repos[0].default_branch, None, "missing key becomes None");
        assert!(!repos[0].is_private);

        assert_eq!(repos[1].owner, "someone");
        assert_eq!(repos[1].description.as_deref(), Some("A fork"));
        assert_eq!(repos[1].language.as_deref(), Some("Rust"));
        assert_eq!(repos[1].default_branch.as_deref(), Some("main"));
        assert!(repos[1].is_private && repos[1].is_fork && repos[1].is_archived);
        assert_eq!(repos[1].pushed_at, None);
    }

    #[test]
    fn parses_repo_list_rejects_garbage() {
        assert!(parse_repo_list("not json").is_err());
    }

    #[test]
    fn validate_owner_rejects_bad_input() {
        assert!(validate_owner("--foo").is_err());
        assert!(validate_owner("a b").is_err());
        assert!(validate_owner("").is_err());
    }

    #[test]
    fn validate_owner_accepts_good_input() {
        assert!(validate_owner("dillonco").is_ok());
        assert!(validate_owner("my-org.name_1").is_ok());
    }

    #[test]
    fn gh_status_never_panics_whether_or_not_gh_is_present() {
        // Offline-safe: when `gh` is missing this never spawns a process at
        // all (resolve_gh short-circuits to None). When it is present this
        // does one local `gh auth status` call, which is what the real UI
        // does on dialog mount — but the assertion only cares that it never
        // panics and always resolves to a status, not what that status is.
        assert!(gh_status().is_ok());
    }
}
