// Stream F2 — Ref-to-ref compare.
// See PLAN.md section 3 for the design and REVIEW-PERF.md must-fix 5 for the
// commit cap this module implements.
use super::*;

/// Same shape as `commit_graph`'s format string (lib.rs) so `parse_commit_graph`
/// can be reused as-is.
const COMMIT_LOG_FORMAT: &str = "%H%x1f%P%x1f%D%x1f%an%x1f%ar%x1f%s%x1f%b%x1e";

/// REVIEW-PERF must-fix 5: an unbounded `git log base..head` is ~430 B/commit
/// of IPC JSON. Cap it like `commit_graph` does and surface the truncation.
const COMMIT_LOG_CAP: usize = 1000;

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
    let root = active_repo(&state)?;
    ref_compare(&root, base.as_deref(), &head, three_dot)
}

#[tauri::command(async)]
pub fn get_ref_file_diff(
    state: State<'_, AppState>,
    base: Option<String>,
    head: String,
    path: String,
    three_dot: bool,
) -> Result<FileDiff, String> {
    let root = active_repo(&state)?;
    ref_file_diff(&root, base.as_deref(), &head, &path, three_dot)
}

pub(crate) fn ref_compare(
    root: &Path,
    base: Option<&str>,
    head: &str,
    three_dot: bool,
) -> Result<RefCompare, String> {
    let base = resolve_base(root, base)?;
    validate_ref_arg(&base)?;
    validate_ref_arg(head)?;

    // Ahead/behind is inherently a "how far have the two sides diverged since
    // their common ancestor" question, so it always uses the three-dot
    // (symmetric-difference) range regardless of the `three_dot` toggle, which
    // only controls how the file diff is computed.
    let counts = git(
        root,
        &[
            "rev-list",
            "--left-right",
            "--count",
            &format!("{base}...{head}"),
        ],
    )?;
    let mut fields = counts.split_whitespace();
    let behind: u32 = fields.next().unwrap_or("0").parse().unwrap_or(0);
    let ahead: u32 = fields.next().unwrap_or("0").parse().unwrap_or(0);

    let merge_base = git_optional(root, &["merge-base", &base, head])?;

    let files_out = if three_dot {
        git(
            root,
            &[
                "diff",
                "--name-status",
                "--find-renames",
                &format!("{base}...{head}"),
            ],
        )?
    } else {
        git(root, &["diff", "--name-status", "--find-renames", &base, head])?
    };
    let files = parse_name_status(&files_out);

    let limit = COMMIT_LOG_CAP.to_string();
    let format_arg = format!("--format={COMMIT_LOG_FORMAT}");
    let range = format!("{base}..{head}");
    let log_out = git(root, &["log", &format_arg, "-n", &limit, &range])?;
    let commits = parse_commit_graph(&log_out);
    let truncated = commits_truncated(ahead, commits.len());

    Ok(RefCompare {
        base,
        head: head.to_string(),
        merge_base,
        three_dot,
        ahead,
        behind,
        files,
        commits,
        commits_truncated: truncated,
    })
}

pub(crate) fn ref_file_diff(
    root: &Path,
    base: Option<&str>,
    head: &str,
    path: &str,
    three_dot: bool,
) -> Result<FileDiff, String> {
    let base = resolve_base(root, base)?;
    validate_ref_arg(&base)?;
    validate_ref_arg(head)?;
    validate_repo_relative_path(path)?;

    let diff = if three_dot {
        git_optional(root, &["diff", &format!("{base}...{head}"), "--", path])?.unwrap_or_default()
    } else {
        git_optional(root, &["diff", &base, head, "--", path])?.unwrap_or_default()
    };

    Ok(FileDiff {
        path: path.to_string(),
        staged: false,
        binary: diff.contains("Binary files") || diff.contains("GIT binary patch"),
        diff,
    })
}

fn resolve_base(root: &Path, base: Option<&str>) -> Result<String, String> {
    match base {
        Some(value) => Ok(value.to_string()),
        None => default_base_branch(root),
    }
}

/// Pure so it can be unit-tested without spinning up 1000+ commits in a
/// throwaway repo just to exercise the cap.
fn commits_truncated(ahead: u32, commits_shown: usize) -> bool {
    (commits_shown as u32) < ahead
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{commit_all, run, write_file, TempRepo};

    #[test]
    fn commits_truncated_flags_only_when_the_cap_bites() {
        assert!(!commits_truncated(3, 3));
        assert!(!commits_truncated(0, 0));
        assert!(commits_truncated(1200, 1000));
        assert!(!commits_truncated(1000, 1000));
    }

    #[test]
    fn option_like_refs_are_rejected() {
        let repo = TempRepo::new();
        let root = repo.path();
        write_file(root, "a.txt", "1\n");
        commit_all(root, "initial");

        assert!(ref_compare(root, Some("-x"), "HEAD", true).is_err());
        assert!(ref_compare(root, Some("main"), "-x", true).is_err());
        assert!(ref_file_diff(root, Some("-x"), "HEAD", "a.txt", true).is_err());
    }

    #[test]
    fn file_diff_path_traversal_is_rejected() {
        let repo = TempRepo::new();
        let root = repo.path();
        write_file(root, "a.txt", "1\n");
        commit_all(root, "initial");

        let result = ref_file_diff(root, Some("main"), "HEAD", "../../etc/passwd", true);
        assert!(result.is_err());
    }

    #[test]
    fn default_base_resolution_falls_back_to_main() {
        let repo = TempRepo::new();
        let root = repo.path();
        write_file(root, "a.txt", "1\n");
        commit_all(root, "initial");
        run(root, &["checkout", "-b", "feature"]);
        write_file(root, "b.txt", "2\n");
        commit_all(root, "feat: add b");

        let compare = ref_compare(root, None, "feature", true).expect("compare should resolve");
        assert_eq!(compare.base, "main");
        assert_eq!(compare.ahead, 1);
        assert_eq!(compare.behind, 0);
    }

    #[test]
    fn ahead_and_behind_counts_reflect_each_sides_unique_commits() {
        let repo = TempRepo::new();
        let root = repo.path();
        write_file(root, "shared.txt", "v1\n");
        commit_all(root, "initial");
        run(root, &["checkout", "-b", "feature"]);
        write_file(root, "feature-file.txt", "new\n");
        commit_all(root, "feat: add feature file");
        run(root, &["checkout", "main"]);
        write_file(root, "shared.txt", "v2\n");
        commit_all(root, "chore: bump shared on main");
        write_file(root, "shared.txt", "v3\n");
        commit_all(root, "chore: bump shared again");

        let compare = ref_compare(root, Some("main"), "feature", true).expect("compare");
        assert_eq!(compare.ahead, 1);
        assert_eq!(compare.behind, 2);
    }

    #[test]
    fn three_dot_and_two_dot_diffs_differ_once_base_has_diverged() {
        let repo = TempRepo::new();
        let root = repo.path();
        write_file(root, "shared.txt", "v1\n");
        commit_all(root, "initial");
        run(root, &["checkout", "-b", "feature"]);
        write_file(root, "feature-file.txt", "new\n");
        commit_all(root, "feat: add feature file");
        run(root, &["checkout", "main"]);
        write_file(root, "shared.txt", "v2\n");
        commit_all(root, "chore: bump shared on main");

        let three_dot = ref_compare(root, Some("main"), "feature", true).expect("three-dot compare");
        let two_dot = ref_compare(root, Some("main"), "feature", false).expect("two-dot compare");

        // Three-dot (since merge base) only shows what feature itself did.
        assert_eq!(three_dot.files.len(), 1);
        assert_eq!(three_dot.files[0].path, "feature-file.txt");

        // Two-dot (direct) also shows main's own change reverting away.
        assert_eq!(two_dot.files.len(), 2);
        assert!(two_dot.files.iter().any(|f| f.path == "shared.txt"));
        assert!(two_dot.files.iter().any(|f| f.path == "feature-file.txt"));
    }

    #[test]
    fn renames_are_reported_with_an_r_status() {
        let repo = TempRepo::new();
        let root = repo.path();
        let body = (1..=20).map(|n| format!("line {n}\n")).collect::<String>();
        write_file(root, "module.txt", &body);
        commit_all(root, "initial");
        run(root, &["checkout", "-b", "feature"]);
        run(root, &["mv", "module.txt", "renamed.txt"]);
        commit_all(root, "chore: rename module");

        let compare = ref_compare(root, Some("main"), "feature", true).expect("compare");
        assert_eq!(compare.files.len(), 1);
        assert_eq!(compare.files[0].status, "R");
        assert_eq!(compare.files[0].path, "renamed.txt");
    }

    #[test]
    fn per_file_diff_contains_the_added_line() {
        let repo = TempRepo::new();
        let root = repo.path();
        write_file(root, "shared.txt", "v1\n");
        commit_all(root, "initial");
        run(root, &["checkout", "-b", "feature"]);
        write_file(root, "feature-file.txt", "new\n");
        commit_all(root, "feat: add feature file");

        let diff = ref_file_diff(root, Some("main"), "feature", "feature-file.txt", true)
            .expect("file diff should succeed");
        assert!(!diff.binary);
        assert!(diff.diff.contains("+new"));
    }
}

