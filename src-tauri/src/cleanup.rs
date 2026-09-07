// Stream F1 — Branch & worktree cleanup.
//
// Algorithm follows REVIEW-PERF.md must-fix 2, not the slower per-branch loop
// in PLAN.md section 2: one `for-each-ref` pass supplies ahead/behind-vs-base
// and merge status for every branch via the `%(ahead-behind:<base>)` atom
// (git >= 2.41), with a documented per-branch fallback for older git. Only
// unmerged branches that are ahead of base get the (3-spawn) squash-merge
// probe, run across a small thread pool.
use super::*;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchAudit {
    pub name: String,
    pub current: bool,
    pub is_base: bool,
    pub head: String,
    pub short_head: String,
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

const DEFAULT_STALE_DAYS: u32 = 30;
const MIN_STALE_DAYS: u32 = 1;
const MAX_STALE_DAYS: u32 = 3650;
/// Diminishing returns past this measured on a 150-branch synthetic repo
/// (REVIEW-PERF.md §1.6: 4 threads 769ms, 8 threads 545ms).
const MAX_PROBE_WORKERS: usize = 8;

#[tauri::command(async)]
pub fn get_branch_cleanup(
    state: State<'_, AppState>,
    base: Option<String>,
    stale_days: Option<u32>,
) -> Result<BranchCleanupReport, String> {
    let root = active_repo(&state)?;
    branch_cleanup(&root, base.as_deref(), resolved_stale_days(stale_days))
}

fn resolved_stale_days(stale_days: Option<u32>) -> u32 {
    stale_days
        .unwrap_or(DEFAULT_STALE_DAYS)
        .clamp(MIN_STALE_DAYS, MAX_STALE_DAYS)
}

pub(crate) fn branch_cleanup(
    root: &Path,
    base: Option<&str>,
    stale_days: u32,
) -> Result<BranchCleanupReport, String> {
    let stale_days = stale_days.clamp(MIN_STALE_DAYS, MAX_STALE_DAYS);
    let base = match base.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => value.to_string(),
        None => default_base_branch(root)?,
    };
    validate_ref_arg(&base)?;

    let rows = for_each_ref_rows(root, &base)?;

    let worktrees = parse_worktrees(&git(root, &["worktree", "list", "--porcelain"])?, root);
    let mut worktree_by_branch: HashMap<&str, &str> = HashMap::new();
    for worktree in &worktrees {
        if worktree.main {
            continue;
        }
        if let Some(branch) = worktree.branch.as_deref() {
            worktree_by_branch.insert(branch, worktree.path.as_str());
        }
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);

    let trees: Vec<String> = rows.iter().map(|row| row.tree.clone()).collect();

    let mut audits: Vec<BranchAudit> = rows
        .into_iter()
        .map(|row| {
            let is_base = row.name == base;
            let merged = row.ahead_of_base == 0;
            let stale = !row.current && now - row.last_commit_unix > stale_days as i64 * 86_400;
            let worktree_path = worktree_by_branch
                .get(row.name.as_str())
                .map(|path| path.to_string());
            BranchAudit {
                name: row.name,
                current: row.current,
                is_base,
                head: row.head,
                short_head: row.short_head,
                upstream: row.upstream,
                upstream_gone: row.upstream_gone,
                ahead: row.ahead,
                behind: row.behind,
                ahead_of_base: row.ahead_of_base,
                behind_base: row.behind_base,
                merged,
                squash_merged: false,
                stale,
                last_commit_unix: row.last_commit_unix,
                last_commit_relative: row.last_commit_relative,
                worktree_path,
                classification: String::new(),
            }
        })
        .collect();

    for (index, is_squash) in squash_probe_results(root, &base, &audits, &trees) {
        audits[index].squash_merged = is_squash;
    }

    for audit in &mut audits {
        audit.classification = classify(audit).to_string();
    }

    audits.sort_by(|a, b| {
        classification_rank(&a.classification)
            .cmp(&classification_rank(&b.classification))
            .then_with(|| a.name.cmp(&b.name))
    });

    Ok(BranchCleanupReport {
        base,
        stale_days,
        branches: audits,
    })
}

/// Priority: current > base > merged > squashMerged > gone > stale > active.
pub(crate) fn classify(audit: &BranchAudit) -> &'static str {
    if audit.current {
        "current"
    } else if audit.is_base {
        "base"
    } else if audit.merged {
        "merged"
    } else if audit.squash_merged {
        "squashMerged"
    } else if audit.upstream_gone {
        "gone"
    } else if audit.stale {
        "stale"
    } else {
        "active"
    }
}

fn classification_rank(classification: &str) -> u8 {
    match classification {
        "current" => 0,
        "base" => 1,
        "merged" => 2,
        "squashMerged" => 3,
        "gone" => 4,
        "stale" => 5,
        _ => 6, // active
    }
}

/// One `for-each-ref` row before classification is computed.
struct RawBranchRow {
    name: String,
    current: bool,
    upstream: Option<String>,
    upstream_gone: bool,
    ahead: u32,
    behind: u32,
    last_commit_unix: i64,
    last_commit_relative: String,
    ahead_of_base: u32,
    behind_base: u32,
    tree: String,
    head: String,
    short_head: String,
}

/// One `for-each-ref refs/heads` pass carrying ahead/behind-vs-upstream,
/// ahead/behind-vs-base (via the `ahead-behind` atom, git >= 2.41), and the
/// branch's tree (for the squash probe) — replacing the per-branch
/// `rev-list`/`rev-parse` calls the initial plan spawned for every branch.
fn for_each_ref_rows(root: &Path, base: &str) -> Result<Vec<RawBranchRow>, String> {
    let format = format!(
        "%(HEAD)%09%(refname:short)%09%(upstream:short)%09%(upstream:track,nobracket)%09%(committerdate:unix)%09%(committerdate:relative)%09%(ahead-behind:{base})%09%(tree)%09%(objectname)%09%(objectname:short)"
    );
    let result = run_git(root, &["for-each-ref", "refs/heads", &format!("--format={format}")]);
    if result.ok {
        return Ok(parse_cleanup_rows(&result.stdout));
    }
    if result.stderr.contains("unknown field name") {
        // git < 2.41 doesn't know the `ahead-behind` atom; fall back to the
        // slower per-branch path (REVIEW-PERF.md §2.2).
        return for_each_ref_rows_legacy(root, base);
    }
    Err(if result.stderr.trim().is_empty() {
        format!("git for-each-ref failed with exit code {}", result.code)
    } else {
        result.stderr
    })
}

fn parse_cleanup_rows(out: &str) -> Vec<RawBranchRow> {
    out.lines()
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let head_marker = parts.next()?;
            let name = parts.next()?.to_string();
            // Detached HEAD emits a "(HEAD detached at abc1234)" pseudo-branch.
            if name.starts_with('(') {
                return None;
            }
            let upstream = parts
                .next()
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            let (upstream_gone, ahead, behind) = parse_track(parts.next().unwrap_or(""));
            let last_commit_unix = parts.next().unwrap_or("0").trim().parse().unwrap_or(0);
            let last_commit_relative = parts.next().unwrap_or("").to_string();
            let mut ahead_behind = parts.next().unwrap_or("0 0").split_whitespace();
            let ahead_of_base = ahead_behind.next().unwrap_or("0").parse().unwrap_or(0);
            let behind_base = ahead_behind.next().unwrap_or("0").parse().unwrap_or(0);
            let tree = parts.next().unwrap_or("").to_string();
            let head = parts.next().unwrap_or("").to_string();
            let short_head = parts.next().unwrap_or("").to_string();
            Some(RawBranchRow {
                current: head_marker == "*",
                name,
                upstream,
                upstream_gone,
                ahead,
                behind,
                last_commit_unix,
                last_commit_relative,
                ahead_of_base,
                behind_base,
                tree,
                head,
                short_head,
            })
        })
        .collect()
}

/// Pre-2.41 fallback: one `for-each-ref` for the cheap fields, then a
/// `rev-list --left-right --count` and a `rev-parse <branch>^{tree}` per
/// branch. This is the ~30-60ms-per-branch path REVIEW-PERF.md measured at
/// 2.4-4.4s on 150 branches; it only runs when the server's git predates the
/// `ahead-behind` atom.
fn for_each_ref_rows_legacy(root: &Path, base: &str) -> Result<Vec<RawBranchRow>, String> {
    let out = git(
        root,
        &[
            "for-each-ref",
            "refs/heads",
            "--format=%(HEAD)%09%(refname:short)%09%(upstream:short)%09%(upstream:track,nobracket)%09%(committerdate:unix)%09%(committerdate:relative)%09%(objectname)%09%(objectname:short)",
        ],
    )?;

    let mut rows = Vec::new();
    for line in out.lines() {
        let mut parts = line.split('\t');
        let head_marker = match parts.next() {
            Some(value) => value,
            None => continue,
        };
        let name = match parts.next() {
            Some(value) => value.to_string(),
            None => continue,
        };
        if name.starts_with('(') {
            continue;
        }
        let upstream = parts
            .next()
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let (upstream_gone, ahead, behind) = parse_track(parts.next().unwrap_or(""));
        let last_commit_unix = parts.next().unwrap_or("0").trim().parse().unwrap_or(0);
        let last_commit_relative = parts.next().unwrap_or("").to_string();
        let head = parts.next().unwrap_or("").to_string();
        let short_head = parts.next().unwrap_or("").to_string();

        let counts = git(
            root,
            &["rev-list", "--left-right", "--count", &format!("{base}...{name}")],
        )?;
        let mut counts_iter = counts.split_whitespace();
        let behind_base = counts_iter.next().unwrap_or("0").parse().unwrap_or(0);
        let ahead_of_base = counts_iter.next().unwrap_or("0").parse().unwrap_or(0);
        let tree = git(root, &["rev-parse", &format!("{name}^{{tree}}")])?;

        rows.push(RawBranchRow {
            current: head_marker == "*",
            name,
            upstream,
            upstream_gone,
            ahead,
            behind,
            last_commit_unix,
            last_commit_relative,
            ahead_of_base,
            behind_base,
            tree,
            head,
            short_head,
        });
    }
    Ok(rows)
}

/// Runs the squash-merge probe only for branches that need it (not merged,
/// ahead of base, not current/base), spread across a small thread pool.
/// Measured: 150 branches / 100 unmerged went from 4.4s serial to 545ms at
/// 8 threads (REVIEW-PERF.md §1.6).
fn squash_probe_results(
    root: &Path,
    base: &str,
    audits: &[BranchAudit],
    trees: &[String],
) -> Vec<(usize, bool)> {
    let candidates: Vec<usize> = audits
        .iter()
        .enumerate()
        .filter(|(_, audit)| !audit.merged && audit.ahead_of_base > 0 && !audit.current && !audit.is_base)
        .map(|(index, _)| index)
        .collect();
    if candidates.is_empty() {
        return Vec::new();
    }

    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, MAX_PROBE_WORKERS)
        .min(candidates.len());
    let mut buckets: Vec<Vec<usize>> = vec![Vec::new(); workers];
    for (position, index) in candidates.into_iter().enumerate() {
        buckets[position % workers].push(index);
    }

    std::thread::scope(|scope| {
        let handles: Vec<_> = buckets
            .into_iter()
            .map(|bucket| {
                scope.spawn(|| {
                    bucket
                        .into_iter()
                        .map(|index| {
                            let is_squash =
                                is_squash_merged(root, base, &audits[index].name, &trees[index])
                                    .unwrap_or(false);
                            (index, is_squash)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().expect("squash probe thread panicked"))
            .collect()
    })
}

/// Is `branch` fully accounted for on `base` by an equivalent squashed diff,
/// even though it isn't an ancestor of `base` (so `merged` is false)?
/// Builds a throwaway commit (`probe`) with `branch`'s tree on top of the
/// merge-base, then asks `git cherry` whether that combined diff already has
/// a patch-equivalent commit on `base`. Falls back to a full `git cherry`
/// comparison when `commit-tree` can't run (no committer identity configured
/// — common in fresh CI/container checkouts).
pub(crate) fn is_squash_merged(root: &Path, base: &str, branch: &str, tree: &str) -> Result<bool, String> {
    let merge_base = match git_optional(root, &["merge-base", base, branch])? {
        Some(value) => value,
        None => return Ok(false),
    };
    match git_optional(root, &["commit-tree", tree, "-p", &merge_base, "-m", "gitc-squash-probe"])? {
        Some(probe) => {
            let cherry = git(root, &["cherry", base, &probe])?;
            Ok(cherry.trim_start().starts_with('-'))
        }
        None => {
            let cherry = git(root, &["cherry", base, branch])?;
            let trimmed = cherry.trim();
            Ok(!trimmed.is_empty() && trimmed.lines().all(|line| line.starts_with('-')))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{commit_all, run, write_file, TempRepo, REPO_COUNTER};
    use std::sync::atomic::Ordering;

    fn base_audit() -> BranchAudit {
        BranchAudit {
            name: "x".to_string(),
            current: false,
            is_base: false,
            head: "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_string(),
            short_head: "deadbee".to_string(),
            upstream: None,
            upstream_gone: false,
            ahead: 0,
            behind: 0,
            ahead_of_base: 0,
            behind_base: 0,
            merged: false,
            squash_merged: false,
            stale: false,
            last_commit_unix: 0,
            last_commit_relative: String::new(),
            worktree_path: None,
            classification: String::new(),
        }
    }

    // ---- pure unit tests ----

    #[test]
    fn parse_track_handles_all_formats() {
        assert_eq!(parse_track(""), (false, 0, 0));
        assert_eq!(parse_track("gone"), (true, 0, 0));
        assert_eq!(parse_track("ahead 3"), (false, 3, 0));
        assert_eq!(parse_track("behind 2"), (false, 0, 2));
        assert_eq!(parse_track("ahead 3, behind 2"), (false, 3, 2));
    }

    #[test]
    fn classify_priority_order() {
        assert_eq!(classify(&{ let mut a = base_audit(); a.current = true; a }), "current");
        assert_eq!(
            classify(&{
                let mut a = base_audit();
                a.current = true;
                a.is_base = true;
                a
            }),
            "current"
        );
        assert_eq!(classify(&{ let mut a = base_audit(); a.is_base = true; a }), "base");
        assert_eq!(classify(&{ let mut a = base_audit(); a.merged = true; a }), "merged");
        assert_eq!(
            classify(&{
                let mut a = base_audit();
                a.merged = true;
                a.squash_merged = true;
                a
            }),
            "merged"
        );
        assert_eq!(
            classify(&{ let mut a = base_audit(); a.squash_merged = true; a }),
            "squashMerged"
        );
        assert_eq!(
            classify(&{
                let mut a = base_audit();
                a.squash_merged = true;
                a.upstream_gone = true;
                a
            }),
            "squashMerged"
        );
        assert_eq!(classify(&{ let mut a = base_audit(); a.upstream_gone = true; a }), "gone");
        assert_eq!(
            classify(&{
                let mut a = base_audit();
                a.upstream_gone = true;
                a.stale = true;
                a
            }),
            "gone"
        );
        assert_eq!(classify(&{ let mut a = base_audit(); a.stale = true; a }), "stale");
        assert_eq!(classify(&base_audit()), "active");
    }

    #[test]
    fn parses_for_each_ref_cleanup_rows() {
        let out = "*\tmain\t\t\t1700000000\t2 days ago\t0 0\ttree1\thead1\ths1\n \tfeature\torigin/feature\tahead 3, behind 1\t1690000000\t1 month ago\t2 5\ttree2\thead2\ths2\n*\t(HEAD detached at 7d24eb3)\t\t\t0\t\t0 0\t\t\t";
        let rows = parse_cleanup_rows(out);

        assert_eq!(rows.len(), 2);
        assert!(rows[0].current);
        assert_eq!(rows[0].name, "main");
        assert_eq!(rows[0].ahead_of_base, 0);
        assert_eq!(rows[0].behind_base, 0);
        assert_eq!(rows[0].tree, "tree1");
        assert_eq!(rows[0].head, "head1");
        assert_eq!(rows[0].short_head, "hs1");
        assert!(!rows[1].current);
        assert_eq!(rows[1].upstream.as_deref(), Some("origin/feature"));
        assert_eq!(rows[1].ahead, 3);
        assert_eq!(rows[1].behind, 1);
        assert_eq!(rows[1].ahead_of_base, 2);
        assert_eq!(rows[1].behind_base, 5);
    }

    #[test]
    fn resolved_stale_days_default_and_clamp() {
        assert_eq!(resolved_stale_days(None), 30);
        assert_eq!(resolved_stale_days(Some(0)), 1);
        assert_eq!(resolved_stale_days(Some(5_000)), 3650);
        assert_eq!(resolved_stale_days(Some(90)), 90);
    }

    // ---- integration tests against a real git CLI ----

    #[test]
    fn merged_branch_is_classified_merged() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "1\n");
        commit_all(repo.path(), "base commit");
        run(repo.path(), &["checkout", "-b", "feature/merged"]);
        write_file(repo.path(), "b.txt", "1\n");
        commit_all(repo.path(), "feature commit");
        run(repo.path(), &["checkout", "main"]);
        run(repo.path(), &["merge", "--no-ff", "feature/merged", "-m", "merge feature"]);

        let report = branch_cleanup(repo.path(), Some("main"), 30).expect("branch_cleanup");
        let audit = report
            .branches
            .iter()
            .find(|b| b.name == "feature/merged")
            .expect("feature/merged present");
        assert!(audit.merged);
        assert!(!audit.squash_merged);
        assert_eq!(audit.classification, "merged");
    }

    #[test]
    fn squash_merged_branch_is_detected_and_not_merged() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "1\n");
        commit_all(repo.path(), "base commit");
        run(repo.path(), &["checkout", "-b", "feature/squash"]);
        write_file(repo.path(), "b.txt", "1\n");
        commit_all(repo.path(), "feature work one");
        write_file(repo.path(), "c.txt", "1\n");
        commit_all(repo.path(), "feature work two");
        run(repo.path(), &["checkout", "main"]);
        run(repo.path(), &["merge", "--squash", "feature/squash"]);
        commit_all(repo.path(), "squash merge feature/squash");

        let report = branch_cleanup(repo.path(), Some("main"), 30).expect("branch_cleanup");
        let audit = report
            .branches
            .iter()
            .find(|b| b.name == "feature/squash")
            .expect("feature/squash present");
        assert!(!audit.merged);
        assert!(audit.squash_merged);
        assert_eq!(audit.classification, "squashMerged");
    }

    #[test]
    fn upstream_gone_branch_is_detected() {
        let origin = TempRepo::new();
        write_file(origin.path(), "a.txt", "1\n");
        commit_all(origin.path(), "init");

        let clone_path = std::env::temp_dir().join(format!(
            "gitc-test-clone-{}-{}",
            std::process::id(),
            REPO_COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        run(
            &std::env::temp_dir(),
            &["clone", origin.path().to_str().unwrap(), clone_path.to_str().unwrap()],
        );
        run(&clone_path, &["checkout", "-b", "feature/gone"]);
        write_file(&clone_path, "b.txt", "1\n");
        commit_all(&clone_path, "feature commit");
        run(&clone_path, &["push", "-u", "origin", "feature/gone"]);
        run(&clone_path, &["checkout", "main"]);
        run(origin.path(), &["branch", "-D", "feature/gone"]);
        run(&clone_path, &["fetch", "--prune"]);

        let report = branch_cleanup(&clone_path, Some("main"), 30).expect("branch_cleanup");
        let audit = report
            .branches
            .iter()
            .find(|b| b.name == "feature/gone")
            .expect("feature/gone present");
        assert!(audit.upstream_gone);
        assert!(!audit.merged);
        assert_eq!(audit.classification, "gone");

        fs::remove_dir_all(&clone_path).ok();
    }

    #[test]
    fn stale_branch_is_detected_via_committer_date() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "1\n");
        commit_all(repo.path(), "init");
        run(repo.path(), &["checkout", "-b", "experiment/old"]);
        write_file(repo.path(), "old.txt", "1\n");
        run(repo.path(), &["add", "-A"]);
        let committed = run_git_env(
            repo.path(),
            &["commit", "-m", "old work"],
            &[("GIT_COMMITTER_DATE", "2020-01-01T00:00:00")],
        );
        assert!(committed.ok, "commit failed: {}", committed.stderr);
        run(repo.path(), &["checkout", "main"]);
        write_file(repo.path(), "main2.txt", "1\n");
        commit_all(repo.path(), "advance main");

        let report = branch_cleanup(repo.path(), Some("main"), 30).expect("branch_cleanup");
        let audit = report
            .branches
            .iter()
            .find(|b| b.name == "experiment/old")
            .expect("experiment/old present");
        assert!(!audit.merged);
        assert!(audit.stale);
        assert_eq!(audit.classification, "stale");
    }

    #[test]
    fn branch_checked_out_in_linked_worktree_reports_path() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "1\n");
        commit_all(repo.path(), "init");
        run(repo.path(), &["branch", "wt-branch"]);

        let wt_path = std::env::temp_dir().join(format!(
            "gitc-test-wt-{}-{}",
            std::process::id(),
            REPO_COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        run(
            repo.path(),
            &["worktree", "add", wt_path.to_str().unwrap(), "wt-branch"],
        );

        let report = branch_cleanup(repo.path(), Some("main"), 30).expect("branch_cleanup");
        let audit = report
            .branches
            .iter()
            .find(|b| b.name == "wt-branch")
            .expect("wt-branch present");
        let reported = PathBuf::from(audit.worktree_path.clone().expect("worktree_path set"));
        assert_eq!(
            reported.canonicalize().unwrap_or(reported.clone()),
            wt_path.canonicalize().unwrap_or(wt_path.clone())
        );

        fs::remove_dir_all(&wt_path).ok();
    }

    #[test]
    fn stale_days_is_clamped_in_branch_cleanup() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "1\n");
        commit_all(repo.path(), "init");

        let low = branch_cleanup(repo.path(), Some("main"), 0).expect("branch_cleanup");
        assert_eq!(low.stale_days, 1);
        let high = branch_cleanup(repo.path(), Some("main"), 999_999).expect("branch_cleanup");
        assert_eq!(high.stale_days, 3650);
    }

    #[test]
    fn rejects_option_like_base() {
        let repo = TempRepo::new();
        write_file(repo.path(), "a.txt", "1\n");
        commit_all(repo.path(), "init");

        let err = branch_cleanup(repo.path(), Some("-x"), 30).expect_err("should reject");
        assert!(err.contains("invalid ref argument"));
    }
}
