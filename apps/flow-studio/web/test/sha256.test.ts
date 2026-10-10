// The synchronous SHA-256 the player names a file by, against Node's.

import { createHash } from "node:crypto";
import { describe, expect, it } from "vitest";
import { sha256Hex } from "../src/session/sha256";

describe("sha256Hex", () => {
  it.each([
    "",
    "abc",
    "taxi.pdf",
    "a".repeat(55),
    "a".repeat(56),
    "a".repeat(64),
    "é漢字",
    "x".repeat(1000),
  ])("is Node's digest of %j", (text) => {
    expect(sha256Hex(text)).toBe(
      createHash("sha256").update(text, "utf8").digest("hex"),
    );
  });
});
