// The front end's player against the studio's real server: every example of
// every module, played through the wire as the page plays it. It needs the
// server's virtual environment (apps/flow-studio/README.md), and fails, not
// skips, without it.

import { afterAll, beforeAll, describe, expect, it } from "vitest";
import type { Wire } from "../src/api/wire";
import {
  type ExamplesDoc,
  type Outcome,
  runExample,
} from "../src/session/player";
import { startServer } from "./server";

let wire: Wire;
let stop: () => void;

beforeAll(async () => {
  ({ wire, stop } = await startServer());
}, 30_000);

afterAll(() => stop?.());

describe("every example, played through the wire as the page plays it", () => {
  it("passes, and none fails, waits or is not run", async () => {
    const tally: Record<Outcome, number> = {
      passed: 0,
      failed: 0,
      "not run": 0,
      waiting: 0,
    };
    const failures: string[] = [];
    for (const m of await wire.modules()) {
      if (!m.examples) continue;
      const bundle = await wire.module(m.file);
      const loaded = await wire.load(bundle.files);
      expect(loaded.loaded, m.file).toBe(true);
      const doc = bundle.examples?.doc as ExamplesDoc;
      for (const name of Object.keys(doc.examples)) {
        const { outcome, why } = await runExample(wire, doc, name);
        tally[outcome] += 1;
        // every example runs through the wire since the import did (ADR-0154)
        if (outcome !== "passed") failures.push(`${m.file} ${name}: ${why}`);
      }
    }
    expect(failures).toEqual([]);
    expect(tally.passed).toBeGreaterThan(0);
    console.log(`examples through the page's player: ${JSON.stringify(tally)}`);
  }, 120_000);
});
