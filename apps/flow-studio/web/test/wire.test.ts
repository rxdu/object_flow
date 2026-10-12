// The HTTP wire's reads that come in pages, followed to the last.

import { afterEach, describe, expect, it, vi } from "vitest";
import { httpWire } from "../src/api/wire";

describe("httpWire", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("reads a history page by page to the last (ADR-0192 decision 5)", async () => {
    const asked: unknown[] = [];
    const pages = [
      {
        events: [{ position: 1 }, { position: 2 }],
        cursor: "2",
        not_computed: ["x"],
      },
      { events: [{ position: 3 }], cursor: null, not_computed: ["x"] },
    ];
    vi.stubGlobal(
      "fetch",
      vi.fn((_path: string, init: { body: string }) => {
        asked.push(JSON.parse(init.body));
        const page = pages.shift();
        return Promise.resolve({ ok: true, json: () => Promise.resolve(page) });
      }),
    );
    const h = await httpWire().history("ticker-1");
    expect(h?.events.map((e) => (e as { position: number }).position)).toEqual([
      1, 2, 3,
    ]);
    expect(h?.not_computed).toEqual(["x"]);
    expect(asked).toEqual([
      { id: "ticker-1" },
      { id: "ticker-1", cursor: "2" },
    ]);
  });

  it("answers null for an object with no history", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() =>
        Promise.resolve({ ok: true, json: () => Promise.resolve(null) }),
      ),
    );
    expect(await httpWire().history("nothing-1")).toBeNull();
  });
});
