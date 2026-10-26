// The front end's player against the studio's real server: every example of
// every module, played through the wire as the page plays it. It needs the
// server's virtual environment (apps/flow-studio/README.md), and fails, not
// skips, without it.

import { afterAll, beforeAll, describe, expect, it } from "vitest";
import type { Wire } from "../src/api/wire";
import {
  besideModule,
  type ExamplesDoc,
  goesOn,
  judged,
  type Outcome,
  Player,
  plan,
  runExample,
  take,
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

    it.each(wrong)(
      "is failed by the stepper where %s reads otherwise",
      async (metric, want) => {
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
        expect(await stepped(wire, doc, tested)).not.toBeNull();
        expect(
          await stepped(wire, bundle.examples?.doc as ExamplesDoc, tested),
        ).toBeNull();
      },
    );
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
          // a `publish` step's files beside the examples file (ADR-0166), and
          // the module's, which a publish of the change installs (ADR-0232)
          const beside = besideModule(
            wire,
            bundle.examples?.name ?? "",
            undefined,
            bundle.files,
          );
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

  describe("every example, stepped as the page's stepper steps it", () => {
    it("fails an approval whose proposal is not as its example claims", async () => {
      const bundle = await wire.module("delivery.yaml");
      expect((await wire.load(bundle.files)).loaded).toBe(true);
      const right = bundle.examples?.doc as ExamplesDoc;
      const doc = structuredClone(right);
      const tested = "an_agents_proposal_is_executed_when_approved";
      const ex = doc.examples[tested];
      if (!ex) throw new Error(`no example ${tested}`);
      ex.expect.proposal = "pending";
      expect(await stepped(wire, right, tested)).toBeNull();
      expect(await stepped(wire, doc, tested)).toBe(
        "expected the proposal pending, and it is EXECUTED",
      );
    });

    it("reaches its request under test", async () => {
      const stopped: string[] = [];
      let examples = 0;
      for (const m of await wire.modules()) {
        if (!m.examples) continue;
        const bundle = await wire.module(m.file);
        expect((await wire.load(bundle.files)).loaded, m.file).toBe(true);
        const doc = bundle.examples?.doc as ExamplesDoc;
        for (const name of Object.keys(doc.examples)) {
          const beside = besideModule(
            wire,
            bundle.examples?.name ?? "",
            undefined,
            bundle.files,
          );
          const steps = plan(doc, name);
          const p = new Player(wire, (await wire.reset()).now, beside);
          let reached = 0;
          for (const ps of steps) {
            const t = await take(p, ps);
            reached += 1;
            if (!goesOn(t)) {
              if (reached < steps.length)
                stopped.push(`${m.file} ${name}: ${ps.label} is ${t.status}`);
              break;
            }
          }
          // the stepper's cleanup, which holds the version under publish
          // again after an example starting at the version before (ADR-0232)
          if (doc.examples[name]?.before) await wire.load(bundle.files);
          examples += 1;
        }
      }
      expect(stopped).toEqual([]);
      expect(examples).toBeGreaterThan(0);
    }, 120_000);

    it("fails a step it cannot take, and stops there", async () => {
      const bundle = await wire.module("service.yaml");
      expect((await wire.load(bundle.files)).loaded).toBe(true);
      const doc = structuredClone(bundle.examples?.doc) as ExamplesDoc;
      const tested = "a_reassignment_is_measured_as_a_handoff";
      const ex = doc.examples[tested];
      if (!ex?.request) throw new Error(`no example ${tested}`);
      ex.request.object = "nobody";
      const p = new Player(wire, (await wire.reset()).now);
      const taken = [];
      for (const ps of plan(doc, tested)) {
        const t = await take(p, ps);
        taken.push(t);
        if (!goesOn(t)) break;
      }
      expect(taken.length).toBe(plan(doc, tested).length);
      expect(taken.at(-1)).toEqual({
        status: "failed",
        note: "no object is named nobody",
      });
    });
  });
});

/** An example stepped to its request under test as the page's stepper steps
 * it, and why it does not do what it claims, or null where it does. */
async function stepped(
  wire: Wire,
  doc: ExamplesDoc,
  name: string,
): Promise<string | null> {
  const p = new Player(wire, (await wire.reset()).now);
  for (const ps of plan(doc, name)) {
    const t = await take(p, ps);
    if (ps.kind === "tested") {
      if (!t.verdict)
        throw new Error(`${name}'s request under test is ${t.status}`);
      return judged(p, ps, t.verdict);
    }
    if (!goesOn(t))
      throw new Error(`${name} stops at ${ps.label}: ${t.status}`);
  }
  throw new Error(`${name} has no request under test`);
}
