// Shared modal infrastructure for the F1-F4 feature panels (CleanupPanel,
// CompareView, CloneDialog, RebasePanel). See REVIEW-UX.md section 2 for the
// full shell/keyboard/CSS contract this pairs with. Svelte 5 legacy-mode
// action (no runes) — usable as `use:trapFocus={{ initial: "#cleanup-base" }}`.

type TrapFocusOptions = { initial?: string };

export const FOCUSABLE =
  'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/**
 * Svelte action: traps Tab/Shift+Tab inside `node`, focuses the element
 * matching `initial` (a CSS selector) or the first focusable element, and
 * restores focus to whatever was focused before the node was attached once
 * the action is destroyed (i.e. when the panel closes).
 */
export function trapFocus(node: HTMLElement, opts: TrapFocusOptions = {}) {
  const previouslyFocused = document.activeElement as HTMLElement | null;
  let options = opts;

  function focusableElements(): HTMLElement[] {
    return Array.from(node.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
      (el) => !el.hasAttribute("disabled") && el.getClientRects().length > 0,
    );
  }

  function focusInitial() {
    const target =
      (options.initial && node.querySelector<HTMLElement>(options.initial)) ||
      focusableElements()[0] ||
      node;
    target.focus();
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key !== "Tab") return;
    const items = focusableElements();
    if (!items.length) {
      event.preventDefault();
      node.focus();
      return;
    }
    const first = items[0];
    const last = items[items.length - 1];
    const active = document.activeElement as HTMLElement | null;
    if (event.shiftKey) {
      if (active === first || !active || !node.contains(active)) {
        event.preventDefault();
        last.focus();
      }
    } else if (active === last || !active || !node.contains(active)) {
      event.preventDefault();
      first.focus();
    }
  }

  // Deferred so any {#if}/{#await}-rendered children (and the element the
  // `initial` selector targets) are in the DOM before we try to focus them.
  queueMicrotask(focusInitial);
  node.addEventListener("keydown", onKeydown);

  return {
    update(nextOpts: TrapFocusOptions = {}) {
      options = nextOpts;
    },
    destroy() {
      node.removeEventListener("keydown", onKeydown);
      previouslyFocused?.focus();
    },
  };
}
