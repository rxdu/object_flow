// The wire tests' server: once stopped, it leaves no workspace in the
// system's temporary directory, where every run left one per backend.

import { existsSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { startServer } from "./server";

describe("the test server", () => {
  it("leaves no workspace once stopped", async () => {
    const { wire, stop, workspace } = await startServer("runner");
    expect(existsSync(workspace)).toBe(true);
    await wire.backend();
    await stop();
    expect(existsSync(workspace)).toBe(false);
  }, 60_000);
});
