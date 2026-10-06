// A session saved as an example (docs/design/flow-studio.md §5): free play
// on a workspace copy of a module, drafted in the examples format, and saved
// only once the flow checker runs it and finds that it holds.

import { afterAll, beforeAll, describe, expect, it } from "vitest";
import type { Wire } from "../src/api/wire";
import { draftExample, durationText } from "../src/session/draft";
import { OPERATOR, Player } from "../src/session/player";
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
});
