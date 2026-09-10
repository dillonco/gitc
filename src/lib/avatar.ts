// Author avatars for graph nodes. GitHub's noreply addresses map straight to
// the account's avatar (the numeric id needs no lookup); anything else goes
// to Gravatar, which only ever sees an MD5 of the address. `d=404` makes a
// missing Gravatar fail, so the node falls back to initials.
export function avatarUrl(email: string, size = 64): string | null {
  const address = email.trim().toLowerCase();
  if (!address.includes("@")) return null;
  const noreply = address.match(/^(?:(\d+)\+)?([^@]+)@users\.noreply\.github\.com$/);
  if (noreply) {
    return noreply[1]
      ? `https://avatars.githubusercontent.com/u/${noreply[1]}?s=${size}&v=4`
      : `https://avatars.githubusercontent.com/${encodeURIComponent(noreply[2])}?s=${size}`;
  }
  return `https://www.gravatar.com/avatar/${md5(address)}?s=${size}&d=404`;
}

export type Person = { name: string; email: string };

// "Co-authored-by: Name <email>" trailers, as git and GitHub write them.
export function coAuthorsOf(body: string): Person[] {
  const people = new Map<string, Person>();
  for (const match of body.matchAll(/^co-authored-by:[ \t]*(.*?)[ \t]*<([^>\s]+)>[ \t]*$/gim)) {
    const key = match[2].toLowerCase();
    if (!people.has(key)) people.set(key, { name: match[1] || match[2], email: match[2] });
  }
  return [...people.values()];
}

const shifts = [
  7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22,
  5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20,
  4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23,
  6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];
const constants = Array.from({ length: 64 }, (_, i) => Math.floor(Math.abs(Math.sin(i + 1)) * 2 ** 32) | 0);

// RFC 1321. Web Crypto has no MD5, and Gravatar keys on it.
export function md5(input: string): string {
  const bytes = new TextEncoder().encode(input);
  const blocks = ((bytes.length + 8) >>> 6) + 1;
  const words = new Int32Array(blocks * 16);
  bytes.forEach((byte, i) => (words[i >> 2] |= byte << ((i % 4) * 8)));
  words[bytes.length >> 2] |= 0x80 << ((bytes.length % 4) * 8);
  words[blocks * 16 - 2] = bytes.length * 8;
  words[blocks * 16 - 1] = Math.floor((bytes.length * 8) / 2 ** 32);

  const state = [0x67452301, 0xefcdab89 | 0, 0x98badcfe | 0, 0x10325476];
  for (let block = 0; block < words.length; block += 16) {
    let [a, b, c, d] = state;
    for (let i = 0; i < 64; i++) {
      let f: number;
      let g: number;
      if (i < 16) {
        f = (b & c) | (~b & d);
        g = i;
      } else if (i < 32) {
        f = (d & b) | (~d & c);
        g = (5 * i + 1) % 16;
      } else if (i < 48) {
        f = b ^ c ^ d;
        g = (3 * i + 5) % 16;
      } else {
        f = c ^ (b | ~d);
        g = (7 * i) % 16;
      }
      const sum = (f + a + constants[i] + words[block + g]) | 0;
      a = d;
      d = c;
      c = b;
      b = (b + ((sum << shifts[i]) | (sum >>> (32 - shifts[i])))) | 0;
    }
    state[0] = (state[0] + a) | 0;
    state[1] = (state[1] + b) | 0;
    state[2] = (state[2] + c) | 0;
    state[3] = (state[3] + d) | 0;
  }

  return state
    .map((word) =>
      [0, 8, 16, 24].map((shift) => ((word >>> shift) & 0xff).toString(16).padStart(2, "0")).join(""),
    )
    .join("");
}
