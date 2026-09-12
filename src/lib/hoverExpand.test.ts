import { describe, expect, it } from "vitest";
import { EXPAND_MAX_WIDTH, clampTop, isClipped, placeOverlay } from "./hoverExpand";

describe("isClipped", () => {
  it("is true only when content overflows by more than a rounding pixel", () => {
    expect(isClipped({ scrollWidth: 200, clientWidth: 120 })).toBe(true);
    expect(isClipped({ scrollWidth: 121, clientWidth: 120 })).toBe(false);
    expect(isClipped({ scrollWidth: 120, clientWidth: 120 })).toBe(false);
  });
});

describe("placeOverlay", () => {
  const rect = { left: 100, top: 300, width: 80, height: 18 };

  it("anchors the float on the clipped text's origin, offset by its own inset", () => {
    const place = placeOverlay(rect, 1400);
    expect(place.left).toBe(93);
    expect(place.top).toBe(297);
    expect(place.maxWidth).toBe(EXPAND_MAX_WIDTH);
    // covers the original element plus the inset on both sides
    expect(place.minWidth).toBe(94);
  });

  it("shrinks the cap so the float stays inside the viewport", () => {
    const place = placeOverlay({ ...rect, left: 1200 }, 1400);
    expect(place.left).toBe(1193);
    expect(place.maxWidth).toBe(1400 - 8 - 1193);
    expect(place.minWidth).toBeLessThanOrEqual(place.maxWidth);
  });

  it("never starts left of the viewport gutter", () => {
    expect(placeOverlay({ ...rect, left: 2 }, 1400).left).toBe(8);
  });
});

describe("clampTop", () => {
  it("leaves a float alone when it fits", () => {
    expect(clampTop(300, 60, 900)).toBe(300);
  });

  it("nudges a float up when its bottom would run off the viewport", () => {
    expect(clampTop(870, 60, 900)).toBe(900 - 8 - 60);
  });

  it("stops at the top gutter for a float taller than the viewport", () => {
    expect(clampTop(10, 2000, 900)).toBe(8);
  });
});
