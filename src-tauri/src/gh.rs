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
    /// `true` when the owner is an organization rather than a user.
    pub owner_is_org: bool,
    pub description: Option<String>,
    pub is_private: bool,
    pub is_fork: bool,
    pub is_archived: bool,
    pub pushed_at: Option<String>,
    pub url: String,
    pub ssh_url: String,
    pub language: Option<String>,
    pub default_branch: Option<String>,
    /// `owner/name` of the repository this one was forked from.
    pub parent: Option<String>,
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

/// Every repository the signed-in user can reach — their own, their
/// organizations', and ones they collaborate on — most recently pushed
/// first. One paginated REST call (`gh repo list` only covers one owner).
#[tauri::command(async)]
pub fn gh_repo_list() -> Result<Vec<GhRepo>, String> {
    let gh_path = resolve_gh()
        .ok_or_else(|| "GitHub CLI not found; install with `brew install gh`.".to_string())?;
    let mut command = Command::new(&gh_path);
    command.args([
        "api",
        "--paginate",
        "user/repos?per_page=100&sort=pushed&affiliation=owner,collaborator,organization_member",
        "--jq",
        ".[]",
    ]);
    let result = run_command(&mut command, false);
    if !result.ok {
        return Err(if result.stderr.trim().is_empty() {
            format!("gh api user/repos failed with exit code {}", result.code)
        } else {
            result.stderr.trim().to_string()
        });
    }
    parse_repo_list(&result.stdout)
}

/// `owner/name` of the repository a fork was made from. The list endpoint
/// leaves this out, so the dialog asks when a fork is selected.
#[tauri::command(async)]
pub fn gh_repo_parent(name_with_owner: String) -> Result<Option<String>, String> {
    let (owner, name) = name_with_owner
        .split_once('/')
        .ok_or_else(|| "expected owner/name".to_string())?;
    validate_owner(owner)?;
    validate_owner(name)?;
    let gh_path = resolve_gh().ok_or_else(|| "GitHub CLI not found".to_string())?;
    let mut command = Command::new(&gh_path);
    command.args([
        "api",
        &format!("repos/{owner}/{name}"),
        "--jq",
        ".parent.full_name // empty",
    ]);
    let result = run_command(&mut command, false);
    if !result.ok {
        return Err(result.stderr.trim().to_string());
    }
    Ok(Some(result.stdout.trim().to_string()).filter(|parent| !parent.is_empty()))
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
    #[serde(rename = "type", default)]
    kind: Option<String>,
}

/// One element of `GET /user/repos` (REST field names).
#[derive(Debug, Deserialize)]
struct RawRepo {
    name: String,
    full_name: String,
    owner: RawOwner,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    private: bool,
    #[serde(default)]
    fork: bool,
    #[serde(default)]
    archived: bool,
    #[serde(default)]
    pushed_at: Option<String>,
    html_url: String,
    ssh_url: String,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    default_branch: Option<String>,
}

/// Pure parse of `gh api --paginate user/repos --jq '.[]'`: one JSON object
/// per line.
pub(crate) fn parse_repo_list(ndjson: &str) -> Result<Vec<GhRepo>, String> {
    ndjson
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            let repo: RawRepo = serde_json::from_str(line).map_err(|err| err.to_string())?;
            Ok(GhRepo {
                name: repo.name,
                name_with_owner: repo.full_name,
                owner_is_org: repo.owner.kind.as_deref() == Some("Organization"),
                owner: repo.owner.login,
                description: repo.description.filter(|d| !d.trim().is_empty()),
                is_private: repo.private,
                is_fork: repo.fork,
                is_archived: repo.archived,
                pushed_at: repo.pushed_at,
                url: repo.html_url,
                ssh_url: repo.ssh_url,
                language: repo.language,
                default_branch: repo.default_branch,
                parent: None,
            })
        })
        .collect()
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
    fn parses_repo_list_ndjson_sample() {
        let ndjson = concat!(
            r#"{"name":"gitc","full_name":"dillonco/gitc","owner":{"login":"dillonco","id":1},"description":"","private":false,"fork":false,"archived":false,"pushed_at":"2026-09-05T19:24:10Z","html_url":"https://github.com/dillonco/gitc","ssh_url":"git@github.com:dillonco/gitc.git","language":null,"default_branch":"main"}"#,
            "\n",
            r#"{"name":"forked","full_name":"some-org/forked","owner":{"login":"some-org","type":"Organization"},"description":"A fork","private":true,"fork":true,"archived":true,"pushed_at":null,"html_url":"https://github.com/some-org/forked","ssh_url":"git@github.com:some-org/forked.git","language":"Rust"}"#,
            "\n"
        );
        let repos = parse_repo_list(ndjson).expect("valid ndjson parses");
        assert_eq!(repos.len(), 2);
        assert_eq!(repos[0].name_with_owner, "dillonco/gitc");
        assert_eq!(repos[0].description, None, "empty string becomes None");
        assert_eq!(repos[0].language, None);
        assert_eq!(repos[0].default_branch.as_deref(), Some("main"));
        assert_eq!(repos[1].owner, "some-org");
        assert!(repos[1].owner_is_org && !repos[0].owner_is_org);
        assert_eq!(repos[1].language.as_deref(), Some("Rust"));
        assert_eq!(repos[1].default_branch, None, "missing key becomes None");
        assert!(repos[1].is_private && repos[1].is_fork && repos[1].is_archived);
        assert!(parse_repo_list("").unwrap().is_empty());
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
