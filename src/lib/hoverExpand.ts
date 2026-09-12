// Hover-to-expand for ellipsised text. Branch names and commit subjects are
// clipped to their column, so hovering one floats the full text over the row
// in place (same origin, same font) instead of making the reader wait for a
// native tooltip. The float is capped in width and wraps to a few lines at
// most, so a paragraph-length subject still ends in an ellipsis rather than
// covering the graph.

export const EXPAND_MAX_WIDTH = 440;
export const EXPAND_MAX_LINES = 3;
export const EXPAND_DELAY_MS = 150;
const VIEWPORT_GUTTER = 8;
// .hover-expand's inset: 1px border + padding. The float is offset by this
// so its text lands exactly where the clipped text starts.
const INSET_X = 7;
const INSET_Y = 3;

export interface HoverExpandParams {
  /** Text to float; defaults to the node's own text content. */
  text?: string | null;
  /** Show the float even when the text is not clipped. */
  always?: boolean;
}

export interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface Placement {
  left: number;
  top: number;
  minWidth: number;
  maxWidth: number;
}

export function isClipped(node: { scrollWidth: number; clientWidth: number }): boolean {
  // +1 absorbs sub-pixel rounding between the two measurements.
  return node.scrollWidth > node.clientWidth + 1;
}

/** Where the float goes so its text overlays the clipped text's origin,
 *  shrunk to stay inside the viewport and never wider than the cap. */
export function placeOverlay(rect: Box, viewportWidth: number, maxWidth = EXPAND_MAX_WIDTH): Placement {
  const left = Math.max(VIEWPORT_GUTTER, rect.left - INSET_X);
  const room = Math.max(0, viewportWidth - VIEWPORT_GUTTER - left);
  const cap = Math.min(maxWidth, room);
  return {
    left,
    top: rect.top - INSET_Y,
    // At least cover the element it replaces, so a short clip never shows
    // the original ellipsis peeking out past the float's edge.
    minWidth: Math.min(cap, rect.width + INSET_X * 2),
    maxWidth: cap,
  };
}

/** Nudge the float up when its bottom would run off the viewport. */
export function clampTop(top: number, height: number, viewportHeight: number): number {
  const overflow = top + height - (viewportHeight - VIEWPORT_GUTTER);
  return overflow > 0 ? Math.max(VIEWPORT_GUTTER, top - overflow) : top;
}

export function hoverExpand(node: HTMLElement, params: HoverExpandParams = {}) {
  let current = params;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let float: HTMLElement | null = null;

  const textFor = () => (current.text ?? node.textContent ?? "").trim();

  function show() {
    timer = null;
    if (float) return;
    if (!current.always && !isClipped(node)) return;
    const text = textFor();
    if (!text) return;

    const rect = node.getBoundingClientRect();
    const place = placeOverlay(rect, window.innerWidth);
    const style = getComputedStyle(node);

    float = document.createElement("div");
    float.className = "hover-expand";
    float.textContent = text;
    float.style.font = style.font;
    float.style.letterSpacing = style.letterSpacing;
    float.style.color = style.color;
    float.style.setProperty("-webkit-line-clamp", String(EXPAND_MAX_LINES));
    float.style.left = `${place.left}px`;
    float.style.top = `${place.top}px`;
    float.style.minWidth = `${place.minWidth}px`;
    float.style.maxWidth = `${place.maxWidth}px`;
    document.body.appendChild(float);

    const top = clampTop(place.top, float.offsetHeight, window.innerHeight);
    if (top !== place.top) float.style.top = `${top}px`;

    // Anything that moves the row out from under the float dismisses it.
    document.addEventListener("scroll", hide, { capture: true, passive: true });
    window.addEventListener("resize", hide);
    window.addEventListener("blur", hide);
  }

  function hide() {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
    if (!float) return;
    float.remove();
    float = null;
    document.removeEventListener("scroll", hide, { capture: true });
    window.removeEventListener("resize", hide);
    window.removeEventListener("blur", hide);
  }

  function enter() {
    if (timer || float) return;
    timer = setTimeout(show, EXPAND_DELAY_MS);
  }

  node.addEventListener("pointerenter", enter);
  node.addEventListener("pointerleave", hide);
  node.addEventListener("pointerdown", hide);

  return {
    update(next: HoverExpandParams = {}) {
      current = next;
      if (float) float.textContent = textFor();
    },
    destroy() {
      hide();
      node.removeEventListener("pointerenter", enter);
      node.removeEventListener("pointerleave", hide);
      node.removeEventListener("pointerdown", hide);
    },
  };
}
