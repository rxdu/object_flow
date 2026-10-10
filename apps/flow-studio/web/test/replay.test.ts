// The front end's player against the studio's real server: every example of
// every module, played through the wire as the page plays it. It needs the
// server's virtual environment (apps/flow-studio/README.md), and fails, not
// skips, without it.

import { afterAll, beforeAll, describe, expect, it } from "vitest";
import type { Wire } from "../src/api/wire";
import {
  besideModule,
  type ExamplesDoc,
  type Outcome,
  runExample,
} from "../src/session/player";
import { BACKENDS, startServer } from "./server";

describe.each(BACKENDS)("against the %s", (backend) => {
  let wire: Wire;
  let stop: () => void;

  beforeAll(async () => {
    ({ wire, stop } = await startServer(backend));
  }, 300_000);

  afterAll(() => stop?.());

  describe("a metric an example expects (ADR-0173 decision 6)", () => {
    const tested = "a_reassignment_is_measured_as_a_handoff";
    const wrong: [string, unknown][] = [
      ["ServiceJob.handoffs_per_job", [{ month: "2026-01", value: 2 }]],
      [
        "ServiceJob.work_in_progress",
        [
          { state: "WORKING", value: 1 },
          { state: "OPEN", value: 0 },
        ],
      ],
      ["ServiceJob.transition_counts", 4],
      ["ServiceJob.transition_count", 3],
      [
        "ServiceJob.time_in_state",
        [
          { state: "OPEN", month: "2026-01", value: "1 h" },
          { state: "WORKING", month: "2026-01", value: "3 h" },
        ],
      ],
    ];
    it.each(wrong)("fails where %s reads otherwise", async (metric, want) => {
      const bundle = await wire.module("service.yaml");
      expect((await wire.load(bundle.files)).loaded).toBe(true);
      const doc = structuredClone(bundle.examples?.doc) as ExamplesDoc;
      const ex = doc.examples[tested];
      if (!ex) throw new Error(`no example ${tested}`);
      const metrics = { ...(ex.expect.metrics ?? {}) };
      if (metric === "ServiceJob.transition_count")
        delete metrics["ServiceJob.transition_counts"];
      metrics[metric] = want;
      ex.expect.metrics = metrics;
      const beside = besideModule(wire, bundle.examples?.name ?? "");
      const right = await runExample(
        wire,
        bundle.examples?.doc as ExamplesDoc,
        tested,
        beside,
      );
      expect(right.outcome).toBe("passed");
      const { outcome } = await runExample(wire, doc, tested, beside);
      expect(outcome).toBe("failed");
    });
  });

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
          // a `publish` step's files beside the examples file (ADR-0166)
          const beside = besideModule(wire, bundle.examples?.name ?? "");
          const { outcome, why } = await runExample(wire, doc, name, beside);
          tally[outcome] += 1;
          // every example runs through the wire since the import did (ADR-0154)
          if (outcome !== "passed") failures.push(`${m.file} ${name}: ${why}`);
        }
      }
      expect(failures).toEqual([]);
      expect(tally.passed).toBeGreaterThan(0);
      console.log(
        `examples through the page's player: ${JSON.stringify(tally)}`,
      );
    }, 120_000);
  });
});
