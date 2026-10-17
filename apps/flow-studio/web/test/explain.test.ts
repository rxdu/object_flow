import { describe, expect, it } from "vitest";
import type { Verdict } from "../src/api/wire";
import { explain, REMEDY } from "../src/session/explain";

describe("the sentence under a verdict", () => {
  it("names what was applied, a replay and the flags it raised", () => {
    const applied: Extract<Verdict, { kind: "satisfied" }> = {
      kind: "satisfied",
      object_id: "ServiceJob:1",
      version: 1,
      events: [1],
      created: ["ServiceJob:1"],
      replayed: false,
      flags: [],
    };
    expect(explain(applied)).toBe("Applied.");
    expect(explain({ ...applied, replayed: true })).toBe(
      "Already applied; this request replays it.",
    );
    expect(
      explain({
        ...applied,
        flags: [
          { clause: "late", remedy: null },
          { clause: "costly", remedy: null },
        ],
      }),
    ).toBe("Applied, and flagged by late and costly.");
  });

  it("names a refusal's clause and what its remedy asks, never the rule", () => {
    expect(
      explain({
        kind: "unsatisfied",
        clause: "photo_attached",
        remedy: "self_serviceable",
      }),
    ).toBe(
      "photo_attached does not hold: supply or correct an input and try again.",
    );
    expect(
      explain({
        kind: "invariant_violated",
        failures: [
          {
            object_id: "ServiceJob:1",
            type: "ServiceJob",
            invariant: "engineer_active",
            remedy: "dependent",
          },
          {
            object_id: "ServiceJob:2",
            type: "ServiceJob",
            invariant: "engineer_active",
            remedy: "dependent",
          },
        ],
        remedy: "dependent",
      }),
    ).toBe(
      "It would break ServiceJob.engineer_active: another object must change first.",
    );
    expect(
      explain({
        kind: "call_refused",
        step: "effect.0",
        object_id: "Robot:3",
        remedy: "delegable",
        verdict: {
          kind: "unsatisfied",
          clause: "approved",
          remedy: "delegable",
        },
      }),
    ).toBe(
      "Its step effect.0 was refused by approved: someone else must act first.",
    );
  });

  it("explains every other refusal by what it names", () => {
    const cases: [Verdict, string][] = [
      [
        { kind: "unavailable", state: "DONE", remedy: "unreachable_from_here" },
        "Not available in DONE; it is taken from another state.",
      ],
      [
        {
          kind: "unknown_transition",
          transition: "reopen",
          remedy: "self_serviceable",
        },
        "reopen is not a transition this request can take: supply or correct an input and try again.",
      ],
      [
        { kind: "unknown_type", type: "Invoice", remedy: "self_serviceable" },
        "Invoice is not a type of the installed flow: supply or correct an input and try again.",
      ],
      [
        {
          kind: "invalid_input",
          inputs: ["engineer", "site"],
          remedy: "self_serviceable",
        },
        "The inputs engineer and site are not valid: supply or correct an input and try again.",
      ],
      [
        {
          kind: "invalid_input",
          inputs: [],
          remedy: "self_serviceable",
          not_computed: ["inputs"],
        },
        "An input is not valid: supply or correct an input and try again.",
      ],
      [
        {
          kind: "not_requestable",
          parents: [],
          remedy: "unreachable_from_here",
        },
        "Only another transition may take it: nothing done from here will change that.",
      ],
      [
        {
          kind: "not_requestable",
          parents: ["Order.add_line", "Order.copy"],
          remedy: "unreachable_from_here",
        },
        "Only Order.add_line or Order.copy may take it: nothing done from here will change that.",
      ],
      [
        {
          kind: "invalid_input",
          inputs: ["amount"],
          reasons: { amount: "wrong_currency" },
          remedy: "self_serviceable",
        },
        "The input amount is in another currency: supply or correct an input and try again.",
      ],
      [
        {
          kind: "invalid_input",
          inputs: ["amount", "receipt"],
          reasons: { amount: "missing", receipt: "missing" },
          remedy: "self_serviceable",
        },
        "The inputs amount and receipt are left out: supply or correct an input and try again.",
      ],
      [
        {
          kind: "invalid_input",
          inputs: ["amount", "colour"],
          reasons: { amount: "malformed", colour: "unknown_input" },
          remedy: "self_serviceable",
        },
        "The input amount is not written as its type is, and colour is not one this transition takes: supply or correct an input and try again.",
      ],
      [
        { kind: "not_proposable", remedy: "self_serviceable" },
        "It cannot be filed as a proposal; it may be requested directly.",
      ],
      [
        { kind: "stale", remedy: "temporal" },
        "The object changed since it was read: wait, and try again later.",
      ],
    ];
    for (const [v, sentence] of cases) expect(explain(v)).toBe(sentence);
  });

  it("has a sentence for every remedy class", () => {
    expect(Object.keys(REMEDY).sort()).toEqual([
      "delegable",
      "dependent",
      "self_serviceable",
      "temporal",
      "unreachable_from_here",
    ]);
  });
});
