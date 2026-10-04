// The front end's player against the studio's real server: every example of
// every module, played through the wire as the page plays it. It needs the
// server's virtual environment (apps/flow-studio/README.md), and fails, not
// skips, without it.

import { type ChildProcess, spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { type Wire, httpWire } from "../src/api/wire";
import {
  type ExamplesDoc,
  type Outcome,
  runExample,
} from "../src/session/player";

const SERVER = fileURLToPath(new URL("../../server", import.meta.url));
const WAITING = ["import_batch"];

let server: ChildProcess;
let wire: Wire;

beforeAll(async () => {
  server = spawn(".venv/bin/python", ["-m", "flowstudio", "--port", "0"], {
    cwd: SERVER,
  });
  const base = await new Promise<string>((resolve, reject) => {
    let log = "";
    server.stderr?.on("data", (chunk: Buffer) => {
      log += chunk.toString();
      const m = /listening on (http:\/\/127\.0\.0\.1:\d+)/.exec(log);
      if (m?.[1]) resolve(m[1]);
    });
    server.on("exit", (code) =>
      reject(new Error(`the server exited (${code}): ${log}`)),
    );
    server.on("error", reject);
  });
  wire = httpWire(base);
}, 30_000);

afterAll(() => {
  server?.kill();
});

describe("every example, played through the wire as the page plays it", () => {
  it("passes or waits for a named reason, and none fails", async () => {
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
        if (outcome === "failed" || outcome === "not run")
          failures.push(`${m.file} ${name}: ${why}`);
        if (outcome === "waiting")
          expect(
            WAITING.some((w) => why.includes(w)),
            `${name}: ${why}`,
          ).toBe(true);
      }
    }
    expect(failures).toEqual([]);
    expect(tally.passed).toBeGreaterThan(0);
    console.log(`examples through the page's player: ${JSON.stringify(tally)}`);
  }, 120_000);
});
