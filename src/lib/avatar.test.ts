import { describe, expect, it } from "vitest";
import { avatarUrl, coAuthorsOf, md5 } from "./avatar";

describe("coAuthorsOf", () => {
  it("reads Co-authored-by trailers, any case, once per address", () => {
    const body = [
      "Tighten the retry loop.",
      "",
      "Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>",
      "co-authored-by: Priya Natarajan <priya@example.com>",
      "Co-authored-by: Claude <NOREPLY@anthropic.com>",
    ].join("\n");
    expect(coAuthorsOf(body)).toEqual([
      { name: "Claude Opus 5", email: "noreply@anthropic.com" },
      { name: "Priya Natarajan", email: "priya@example.com" },
    ]);
  });

  it("ignores prose that only mentions co-authors", () => {
    expect(coAuthorsOf("Thanks to the co-authored-by trailer idea <x@y.z> in review")).toEqual([]);
  });
});

describe("md5", () => {
  it("matches the RFC 1321 test suite", () => {
    expect(md5("")).toBe("d41d8cd98f00b204e9800998ecf8427e");
    expect(md5("abc")).toBe("900150983cd24fb0d6963f7d28e17f72");
    expect(md5("The quick brown fox jumps over the lazy dog")).toBe("9e107d9d372bb6826bd81d3542a419d6");
    // 80 bytes: the length field no longer fits in the first block.
    expect(md5("12345678901234567890123456789012345678901234567890123456789012345678901234567890")).toBe(
      "57edf4a22be3c955ac49da2e2107b67a",
    );
  });
});

describe("avatarUrl", () => {
  it("uses the GitHub account id from an id+login noreply address", () => {
    expect(avatarUrl("12345+octocat@users.noreply.github.com")).toBe(
      "https://avatars.githubusercontent.com/u/12345?s=64&v=4",
    );
  });

  it("uses the login from an older noreply address", () => {
    expect(avatarUrl("octocat@users.noreply.github.com")).toBe("https://avatars.githubusercontent.com/octocat?s=64");
  });

  it("hashes any other address for Gravatar, normalised first", () => {
    expect(avatarUrl("  MyEmailAddress@example.com ")).toBe(
      `https://www.gravatar.com/avatar/${md5("myemailaddress@example.com")}?s=64&d=404`,
    );
  });

  it("gives up on something that is not an address", () => {
    expect(avatarUrl("")).toBeNull();
    expect(avatarUrl("not-an-email")).toBeNull();
  });
});
