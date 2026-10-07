// A session saved as an example (docs/design/flow-studio.md §5): free play
// on a workspace copy of a module, drafted in the examples format, and saved
// only once the flow checker runs it and finds that it holds.

import { afterAll, beforeAll, describe, expect, it } from "vitest";
import type { Wire } from "../src/api/wire";
import { draftExample, durationText } from "../src/session/draft";
import {
  type ExamplesDoc,
  OPERATOR,
  Player,
  plan,
} from "../src/session/player";
import { BACKENDS, startServer } from "./server";

describe("durations as the model writes them", () => {
  it("takes the largest whole unit", () => {
    expect(durationText(5400)).toBe("90 min");
    expect(durationText(3600)).toBe("1 h");
    expect(durationText(172800)).toBe("2 days");
    expect(durationText(0)).toBe("0 s");
  });
});

describe.each(BACKENDS)("against the %s", (backend) => {
  let wire: Wire;
  let stop: () => void;

  beforeAll(async () => {
    ({ wire, stop } = await startServer(backend));
    await wire.copy("service.yaml");
    await wire.copy("delivery.yaml");
    await wire.copy("servicedesk.yaml");
  }, 300_000);

  afterAll(() => stop?.());

  async function fresh(): Promise<Player> {
    const bundle = await wire.module("service.yaml", "workspace");
    const loaded = await wire.load(bundle.files);
    expect(loaded.loaded).toBe(true);
    return new Player(wire, (await wire.reset()).now);
  }

  async function anaWithAJob(p: Player): Promise<string> {
    const ana = await p.request(
      {
        transition: "add",
        actor: { id: OPERATOR },
        type: "User",
        inputs: { login: "ana", name: "Ana" },
      },
      "User.add",
    );
    if (ana.kind !== "satisfied") throw new Error("Ana is not added");
    await p.advance("90 min");
    const job = await p.request(
      {
        transition: "open",
        actor: { id: ana.object_id },
        type: "ServiceJob",
        inputs: { engineer: ana.object_id },
      },
      "ServiceJob.open",
    );
    if (job.kind !== "satisfied") throw new Error("the job is not opened");
    return job.object_id;
  }

  describe("a session saved as an example", () => {
    it("names its objects, carries its clock, and holds", async () => {
      const p = await fresh();
      await anaWithAJob(p);
      const draft = await draftExample(p, "Ana opens a job for herself.");
      expect(draft.problems).toEqual([]);
      const ex = draft.example as Record<string, unknown>;
      expect(ex["steps"]).toEqual([
        {
          request: {
            type: "User",
            transition: "add",
            inputs: { login: "ana", name: "Ana" },
            as: "user1",
          },
        },
      ]);
      expect(ex["request"]).toEqual({
        type: "ServiceJob",
        transition: "open",
        actor: "user1",
        after: "90 min",
        inputs: { engineer: "user1" },
        as: "service_job1",
      });
      expect(ex["expect"]).toEqual({ verdict: "applied", state: "OPEN" });
      const saved = await wire.addExample(
        "service.yaml",
        "ana_opens_a_job_freely",
        ex,
      );
      expect(saved.findings ?? []).toEqual([]);
      expect(saved.saved).toBe(true);
    }, 60_000);

    it("proposes a refusal as the claim, and the checker holds it", async () => {
      const p = await fresh();
      const job = await anaWithAJob(p);
      const finish = await p.request(
        { transition: "finish", actor: { id: OPERATOR }, object_id: job },
        "finish",
      );
      expect(finish.kind).not.toBe("satisfied");
      const draft = await draftExample(
        p,
        "A job just opened cannot be finished.",
      );
      expect(draft.problems).toEqual([]);
      const ex = draft.example as Record<string, unknown>;
      expect((ex["expect"] as { verdict: string }).verdict).toBe(finish.kind);
      const saved = await wire.addExample(
        "service.yaml",
        "an_open_job_is_not_finished",
        ex,
      );
      expect(saved.findings ?? []).toEqual([]);
      expect(saved.saved).toBe(true);
    }, 60_000);

    it("says why a session with nothing requested cannot be one", async () => {
      const p = await fresh();
      await p.advance("1 h");
      const draft = await draftExample(p, "Nothing.");
      expect(draft.example).toBeNull();
      expect(draft.problems[0]).toMatch(/no request is under test/);
    }, 30_000);
  });

  describe("a session with a proposal", () => {
    // delivery.yaml's hand-over, as its example sets it up, then filed by
    // an agent in free play (DESIGN.md §9)
    async function handover(): Promise<{ p: Player; proposal: string }> {
      const bundle = await wire.module("delivery.yaml", "workspace");
      const loaded = await wire.load(bundle.files);
      expect(loaded.loaded).toBe(true);
      const p = new Player(wire, (await wire.reset()).now);
      const doc = bundle.examples?.doc as ExamplesDoc;
      for (const ps of plan(
        doc,
        "an_agents_proposal_is_executed_when_approved",
      ))
        if (ps.label.startsWith("setup")) await p.perform(ps);
      const scout = await p.request(
        {
          transition: "issue",
          actor: { id: OPERATOR },
          type: "Agent",
          inputs: { key_id: "scout", name: "Scout" },
        },
        "Agent.issue",
      );
      if (scout.kind !== "satisfied") throw new Error("Scout is not issued");
      const filed = await p.request(
        {
          transition: "complete",
          actor: { id: scout.object_id },
          object_id: p.alias("delivery"),
          propose: true,
        },
        "propose complete",
      );
      if (filed.kind !== "satisfied") throw new Error("nothing is filed");
      return { p, proposal: filed.object_id };
    }

    it("files and approves it, written as the format writes them, and holds", async () => {
      const { p, proposal } = await handover();
      // a pending proposal offers its decisions (D535)
      const offers = await wire.availability(OPERATOR, proposal);
      expect(
        offers
          .filter((o) => o.creates === null)
          .map((o) => [o.transition, o.availability]),
      ).toEqual([
        ["approve", "available"],
        ["reject", "available"],
        ["withdraw", "available"],
      ]);
      const approved = await p.request(
        { transition: "approve", actor: { id: OPERATOR }, object_id: proposal },
        "approve",
      );
      expect(approved.kind).toBe("satisfied");
      const draft = await draftExample(
        p,
        "Scout's hand-over, approved, delivers.",
      );
      expect(draft.problems).toEqual([]);
      const ex = draft.example as Record<string, unknown>;
      expect((ex["steps"] as unknown[]).slice(-2)).toEqual([
        {
          request: {
            type: "Agent",
            transition: "issue",
            inputs: { key_id: "scout", name: "Scout" },
            as: "agent1",
          },
        },
        {
          propose: {
            object: "delivery",
            transition: "complete",
            actor: "agent1",
            proposal: "proposal1",
          },
        },
      ]);
      expect(ex["approve"]).toEqual({ proposal: "proposal1" });
      expect(ex["request"]).toBeUndefined();
      expect(ex["expect"]).toEqual({
        verdict: "applied",
        state: "DELIVERED",
        proposal: "executed",
      });
      const saved = await wire.addExample(
        "delivery.yaml",
        "scouts_handover_approved_freely",
        ex,
      );
      expect(saved.findings ?? []).toEqual([]);
      expect(saved.saved).toBe(true);
    }, 120_000);

    it("says why a session ending in a filing cannot be one", async () => {
      const { p } = await handover();
      const draft = await draftExample(p, "Filed.");
      expect(draft.example).toBeNull();
      expect(draft.problems.join(" ")).toMatch(/files a proposal/);
    }, 120_000);
  });

  describe("a session with an import", () => {
    async function desk(): Promise<Player> {
      const bundle = await wire.module("servicedesk.yaml", "workspace");
      const loaded = await wire.load(bundle.files);
      expect(loaded.loaded).toBe(true);
      return new Player(wire, (await wire.reset()).now);
    }

    it("imports a legacy object, written as the format writes it, and holds", async () => {
      const p = await desk();
      const dana = {
        type: "Employee",
        as: "dana",
        state: "EMPLOYED",
        values: {
          employee_id: "E-1001",
          name: "Dana Lee",
          department: "Finance",
        },
      };
      await p.importStep(dana);
      expect(p.log).toEqual([
        { summary: "import Employee as dana", verdict: null },
      ]);
      const op = { id: OPERATOR };
      const laptop = await p.request(
        {
          transition: "receive",
          actor: op,
          type: "Laptop",
          inputs: { name: "ThinkPad 14" },
        },
        "Laptop.receive",
      );
      if (laptop.kind !== "satisfied") throw new Error("no laptop");
      await p.request(
        { transition: "deploy", actor: op, object_id: laptop.object_id },
        "deploy",
      );
      const assigned = await p.request(
        {
          transition: "assign",
          actor: op,
          object_id: laptop.object_id,
          inputs: { assigned_to: p.alias("dana") },
        },
        "assign",
      );
      expect(assigned.kind).toBe("satisfied");
      const draft = await draftExample(
        p,
        "A laptop goes to Dana, whom HR sent.",
      );
      expect(draft.problems).toEqual([]);
      const ex = draft.example as Record<string, unknown>;
      expect((ex["steps"] as unknown[])[0]).toEqual({ import: dana });
      expect(ex["request"]).toEqual({
        object: "laptop1",
        transition: "assign",
        inputs: { assigned_to: "dana" },
      });
      expect(ex["expect"]).toEqual({ verdict: "applied", state: "IN_USE" });
      const saved = await wire.addExample(
        "servicedesk.yaml",
        "a_laptop_goes_to_an_imported_employee_freely",
        ex,
      );
      expect(saved.findings ?? []).toEqual([]);
      expect(saved.saved).toBe(true);
    }, 120_000);

    it("refuses an import its type cannot hold, and records nothing", async () => {
      const p = await desk();
      await expect(
        p.importStep({
          type: "Employee",
          as: "nobody",
          state: "EMPLOYED",
          values: {},
        }),
      ).rejects.toThrow(/refused/);
      expect(p.log).toEqual([]);
      expect(p.trail).toEqual([]);
    }, 60_000);
  });
});
