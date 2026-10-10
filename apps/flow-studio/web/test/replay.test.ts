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

  it("reads an identity that is also an alias as written (D568)", async () => {
    // scripts/check-flows.py's IDENTITY_FLOW and IDENTITY_EXAMPLES
    const flow =
      "module: approve\ncategories: [live, closed]\n\ntypes:\n  User:\n    description: A person who may act.\n    tracking: record\n    attributes:\n      login: { type: identity, actor_kind: human, unique: true }\n    states:\n      ACTIVE: { description: Working., category: live }\n      LEFT:   { description: Gone., category: closed, final: true }\n    transitions:\n      add:   { kind: initial, to: ACTIVE, required_inputs: [login] }\n      leave: { kind: external, from: ACTIVE, to: LEFT }\n\n  Request:\n    description: A request a person approves.\n    tracking: record\n    attributes:\n      title:       { type: string }\n      approved_by: { type: identity, optional: true }\n      approved_at: { type: timestamp, optional: true }\n    states:\n      OPEN:     { description: Waiting., category: live }\n      APPROVED: { description: Approved., category: closed, final: true, required_attributes: [approved_by] }\n    transitions:\n      open: { kind: initial, to: OPEN, required_inputs: [title] }\n      approve:\n        kind: external\n        from: OPEN\n        to: APPROVED\n        effect:\n          - assign: { location: approved_by, expr: actor.id }\n          - assign: { location: approved_at, expr: now }\n    metrics:\n      approvals_by_approver:\n        description: Approvals by who approved, by month.\n        source: objects\n        item: r\n        filter: r.approved_by is not null\n        dimensions:\n          approver: r.approved_by\n        time_dimension: r.approved_at\n        expression: count()\n";
    expect(
      (await wire.load([{ name: "approve.yaml", text: flow }])).loaded,
    ).toBe(true);
    const doc = {
      examples_for: "approve",
      setups: {
        ana_and_a_request: {
          description: "Ana, whose login is her alias, and an open request.",
          steps: [
            {
              request: {
                type: "User",
                transition: "add",
                as: "ana",
                inputs: { login: "ana" },
              },
            },
            {
              request: {
                type: "Request",
                transition: "open",
                as: "request",
                inputs: { title: "Laptop" },
              },
            },
          ],
        },
      },
      examples: {
        an_approval_is_counted_by_its_approver: {
          description:
            "Ana approving the request counts one approval under her login.",
          given: "ana_and_a_request",
          request: { object: "request", transition: "approve", actor: "ana" },
          expect: {
            verdict: "applied",
            state: "APPROVED",
            values: { approved_by: "ana" },
            metrics: {
              "Request.approvals_by_approver": [{ approver: "ana", value: 1 }],
            },
          },
        },
      },
    } as unknown as ExamplesDoc;
    const { outcome, why } = await runExample(
      wire,
      doc,
      "an_approval_is_counted_by_its_approver",
    );
    expect([outcome, why]).toEqual(["passed", ""]);
  });

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
