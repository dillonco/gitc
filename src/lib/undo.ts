import type { GitAction, GitResult, RepositoryState } from "./types";

// Undo/redo for the actions that have a clean inverse. An
// entry is recorded right after its action succeeds and only applies while
// the repository is still where that action left it: if HEAD or the
// checked-out branch has moved since (a terminal, the rebase panel, another
// tool), it refuses rather than resetting over whatever happened in between.

export type RepoSnapshot = Pick<RepositoryState, "head" | "currentBranch" | "files">;

export type UndoEntry = {
  label: string;
  undo: GitAction[];
  redo: GitAction[];
  // A step is a hard reset, which would take uncommitted work with it.
  needsCleanTree: boolean;
};

export type UndoItem = {
  entry: UndoEntry;
  head: string;
  branch: string | null;
};

// Undone with a hard reset to where HEAD was, so only recorded from a clean tree.
const movesHead: Record<string, string> = {
  merge: "merge",
  pull: "pull",
  pullMerge: "pull",
  pullRebase: "pull",
  cherryPick: "cherry-pick",
  revert: "revert",
};

export const undoableKinds = new Set([
  "commit",
  "commitAmend",
  "checkoutBranch",
  "checkoutRemote",
  "checkoutCommit",
  "createBranch",
  "deleteBranch",
  "deleteBranchForce",
  "createTag",
  "deleteTag",
  "reset",
  ...Object.keys(movesHead),
]);

const act = (kind: string, fields: Omit<GitAction, "kind"> = {}): GitAction => ({ kind, ...fields });

function checkoutOf(snapshot: RepoSnapshot): GitAction {
  return snapshot.currentBranch
    ? act("checkoutBranch", { branch: snapshot.currentBranch })
    : act("checkoutCommit", { target: snapshot.head });
}

// `git branch -d` and `git tag -d` both report the ref's old value as "(was abc1234)".
function previousTarget(result: GitResult) {
  return result.stdout.match(/\(was ([0-9a-f]+)\)/)?.[1] ?? null;
}

export function undoEntryFor(
  action: GitAction,
  before: RepoSnapshot,
  after: RepoSnapshot,
  result: GitResult,
): UndoEntry | null {
  if (!result.ok) return null;
  const branch = action.branch ?? "";
  const sameBranch = (before.currentBranch ?? null) === (after.currentBranch ?? null);

  switch (action.kind) {
    case "commit":
    case "commitAmend":
      if (before.head === "unborn" || before.head === after.head || !sameBranch) return null;
      return {
        label: action.kind === "commit" ? "commit" : "amend",
        undo: [act("reset", { target: before.head, mode: "soft" })],
        redo: [act("reset", { target: after.head, mode: "soft" })],
        needsCleanTree: false,
      };

    case "checkoutBranch":
    case "checkoutRemote":
    case "checkoutCommit":
      if (before.head === after.head && sameBranch) return null;
      return {
        label: `checkout ${after.currentBranch || after.head}`,
        undo: [checkoutOf(before)],
        redo: [checkoutOf(after)],
        needsCleanTree: false,
      };

    case "createBranch":
      // gitc creates branches with `checkout -b`, so undo steps back off it first.
      if (!branch || after.currentBranch !== branch) return null;
      return {
        label: `create ${branch}`,
        undo: [checkoutOf(before), act("deleteBranchForce", { branch })],
        redo: [act("createBranch", { branch, target: after.head })],
        needsCleanTree: false,
      };

    case "deleteBranch":
    case "deleteBranchForce": {
      const target = previousTarget(result);
      if (!branch || !target) return null;
      return {
        label: `delete ${branch}`,
        undo: [act("branchAt", { branch, target })],
        redo: [act("deleteBranchForce", { branch })],
        needsCleanTree: false,
      };
    }

    case "createTag": {
      const target = action.target || before.head;
      if (!branch || target === "unborn") return null;
      return {
        label: `tag ${branch}`,
        undo: [act("deleteTag", { branch })],
        redo: [act("createTag", { branch, target })],
        needsCleanTree: false,
      };
    }

    case "deleteTag": {
      // For an annotated tag this is the tag object, so recreating the ref restores it whole.
      const target = previousTarget(result);
      if (!branch || !target) return null;
      return {
        label: `delete tag ${branch}`,
        undo: [act("createTag", { branch, target })],
        redo: [act("deleteTag", { branch })],
        needsCleanTree: false,
      };
    }

    case "reset": {
      const mode = action.mode ?? "mixed";
      if (before.head === after.head || !sameBranch) return null;
      if (mode === "hard" && before.files.length > 0) return null;
      return {
        label: `reset to ${after.head}`,
        undo: [act("reset", { target: before.head, mode })],
        redo: [act("reset", { target: after.head, mode })],
        needsCleanTree: mode === "hard",
      };
    }

    default: {
      const label = movesHead[action.kind];
      if (!label || before.files.length > 0 || before.head === "unborn" || before.head === after.head || !sameBranch) {
        return null;
      }
      return {
        label,
        undo: [act("reset", { target: before.head, mode: "hard" })],
        redo: [act("reset", { target: after.head, mode: "hard" })],
        needsCleanTree: true,
      };
    }
  }
}

export function undoItem(entry: UndoEntry, state: RepoSnapshot): UndoItem {
  return { entry, head: state.head, branch: state.currentBranch ?? null };
}

export function blockedReason(item: UndoItem, state: RepoSnapshot): "moved" | "dirty" | null {
  if (state.head !== item.head || (state.currentBranch ?? null) !== item.branch) return "moved";
  if (item.entry.needsCleanTree && state.files.length > 0) return "dirty";
  return null;
}
