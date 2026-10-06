// Free play through the page's own player, against the real server: a
// person creates objects, reads what one offers, takes it, and reads a
// metric move (docs/design/flow-studio.md §6, slice 2; ADR-0152).

import { afterAll, beforeAll, describe, expect, it } from "vitest";
import type { TransitionOffer, Wire } from "../src/api/wire";
import { OPERATOR, Player } from "../src/session/player";
import { BACKENDS, startServer } from "./server";

describe.each(BACKENDS)("against the %s", (backend) => {
  let wire: Wire;
  let stop: () => void;

  beforeAll(async () => {
    ({ wire, stop } = await startServer(backend));
  }, 300_000);

  afterAll(() => stop?.());

  const offered = (offers: TransitionOffer[], name: string) =>
    offers.find((o) => o.transition === name && o.creates === null);

  describe("free play on the service module", () => {
    it("creates, offers, takes and measures, as any caller would", async () => {
      const bundle = await wire.module("service.yaml");
      const loaded = await wire.load(bundle.files);
      expect(loaded.loaded).toBe(true);
      const p = new Player(wire, (await wire.reset()).now);
      const op = { id: OPERATOR };

      const ana = await p.request(
        {
          transition: "add",
          actor: op,
          type: "User",
          inputs: { login: "ana", name: "Ana" },
        },
        "User.add",
      );
      expect(ana.kind).toBe("satisfied");
      if (ana.kind !== "satisfied") return;
      const job = await p.request(
        {
          transition: "open",
          actor: { id: ana.object_id },
          type: "ServiceJob",
          inputs: { engineer: ana.object_id },
        },
        "ServiceJob.open",
      );
      expect(job.kind).toBe("satisfied");
      if (job.kind !== "satisfied") return;
      expect(p.touched).toEqual([ana.object_id, job.object_id]);

      const before = await wire.availability(ana.object_id, job.object_id);
      expect(before.some((o) => o.transition === "open")).toBe(false);
      const start = offered(before, "start");
      expect(start?.availability).not.toBe("unavailable");
      const refused = before.filter((o) => o.availability === "unavailable");
      expect(refused.length).toBeGreaterThan(0);
      expect(refused.every((o) => o.verdict !== null)).toBe(true);

      const wip = async () =>
        (await wire.metric(ana.object_id, "ServiceJob.work_in_progress"))
          .rows[0]?.value;
      expect(await wip()).toBe(1);

      const took = await p.request(
        {
          transition: "start",
          actor: { id: ana.object_id },
          object_id: job.object_id,
        },
        "start",
      );
      expect(took.kind).toBe("satisfied");
      expect((await wire.get(job.object_id))?.state).toBe("WORKING");
      const after = await wire.availability(ana.object_id, job.object_id);
      expect(offered(after, "start")?.availability).toBe("unavailable");
      expect(p.log.map((e) => e.summary)).toEqual([
        "User.add",
        "ServiceJob.open",
        "start",
      ]);

      const t0 = p.now;
      await p.advance("2 h");
      expect(p.now - t0).toBe(7200);
    }, 60_000);
  });
});
