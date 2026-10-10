// What a verdict tells its reader, in a sentence: the line the simulator
// shows under every step's verdict, above the verdict's own fields.
//
// The studio exists to help its author understand what the engine decided
// and why (docs/design/flow-studio.md §1), and a refusal's remedy class is
// the engine's answer to "what can I do about it?" (DESIGN.md §5.5):
//
//   self_serviceable       supply or correct an input of this request
//   delegable              another actor must act
//   temporal               only time will satisfy it
//   dependent              another object must change first
//   unreachable_from_here  nothing this caller sends, waits for or asks of
//                          another actor or object will satisfy it from here
//
// The fields beside each `kind` are in ../api/wire.ts. Never restate a
// guard's logic here (renderers.md §1): name the clause, and say what the
// remedy means for the person reading. The words are neutral between a
// request refused and an offer blocked, since the live offer explains a
// transition no one has requested yet with the same sentence.

import type { InputReason, Remedy, Verdict } from "../api/wire";

/** What each remedy class asks of the person reading, in their words: the
 * one part of the sentence that is the studio's voice rather than the
 * engine's facts. */
export const REMEDY: Record<Remedy, string> = {
  self_serviceable: "supply or correct an input and try again",
  delegable: "someone else must act first",
  temporal: "wait, and try again later",
  dependent: "another object must change first",
  unreachable_from_here: "nothing done from here will change that",
};

/** What each reason says of an input, after "is" or "are". */
export const REASON: Record<InputReason, string> = {
  unknown_input: "not one this transition takes",
  missing: "left out",
  wrong_type: "of another type than it takes",
  malformed: "not written as its type is",
  wrong_currency: "in another currency",
  no_such_object: "naming no object of its type",
  out_of_range: "more than its type holds",
  not_allowed: "not one this transition allows",
};

/** Why the inputs are at fault, grouped where they share a reason. */
function faults(
  inputs: string[],
  reasons: Record<string, InputReason>,
): string {
  const by = new Map<InputReason, string[]>();
  for (const i of inputs) {
    const r = reasons[i];
    if (r) by.set(r, [...(by.get(r) ?? []), i]);
  }
  const parts = [...by].map(([r, names], n) =>
    n === 0
      ? `${names.length === 1 ? `The input ${names[0]} is` : `The inputs ${listed(names)} are`} ${REASON[r]}`
      : `${listed(names)} ${names.length === 1 ? "is" : "are"} ${REASON[r]}`,
  );
  return parts.join(", and ");
}

/** A list as a sentence reads it: "a", "a and b", "a, b and c". */
function listed(items: string[], last = "and"): string {
  if (items.length <= 1) return items.join("");
  return `${items.slice(0, -1).join(", ")} ${last} ${items[items.length - 1]}`;
}

/** The sentence for a verdict: what the engine decided, by name, and what
 * its remedy class asks of the reader. */
export function explain(v: Verdict): string {
  const next = (r: Remedy | null): string => (r ? `: ${REMEDY[r]}.` : ".");
  switch (v.kind) {
    case "satisfied": {
      if (v.replayed) return "Already applied; this request replays it.";
      const flags = v.flags.map((f) => f.clause);
      return flags.length > 0
        ? `Applied, and flagged by ${listed(flags)}.`
        : "Applied.";
    }
    case "unsatisfied":
      return `${v.clause} does not hold${next(v.remedy)}`;
    case "invariant_violated": {
      const broken = v.failures.map((f) => `${f.type}.${f.invariant}`);
      return `It would break ${listed([...new Set(broken)])}${next(v.remedy)}`;
    }
    case "call_refused": {
      const by = v.verdict.clause
        ? ` by ${v.verdict.clause}`
        : v.verdict.kind
          ? ` as ${v.verdict.kind}`
          : "";
      return `Its step ${v.step} was refused${by}${next(v.remedy)}`;
    }
    // its remedy is always unreachable_from_here (DESIGN.md §5.5), which is
    // true of this request in this state and misleads as a sentence: the
    // object can usually be moved to a state the transition is taken from
    case "unavailable":
      return v.state
        ? `Not available in ${v.state}; it is taken from another state.`
        : "Not available now; it is taken from another state.";
    case "unknown_transition":
      return `${v.transition} is not a transition this request can take${next(v.remedy)}`;
    // a backend that does not work out which input or parent names none,
    // and says so in `not_computed` (wire.ts); the sentence then names none
    case "invalid_input":
      if (v.reasons && v.inputs.every((i) => v.reasons?.[i]))
        return `${faults(v.inputs, v.reasons)}${next(v.remedy)}`;
      return `${
        v.inputs.length === 0
          ? "An input is"
          : v.inputs.length === 1
            ? `The input ${v.inputs[0]} is`
            : `The inputs ${listed(v.inputs)} are`
      } not valid${next(v.remedy)}`;
    case "not_requestable":
      return v.parents.length === 0
        ? `Only another transition may take it${next(v.remedy)}`
        : `Only ${listed(v.parents, "or")} may take it${next(v.remedy)}`;
    case "not_proposable":
      return "It cannot be filed as a proposal; it may be requested directly.";
    case "over_limit":
      return `It would pass a declared limit${next(v.remedy)}`;
    case "not_found":
      return `No object has that id${next(v.remedy)}`;
    case "stale":
      return `The object changed since it was read${next(v.remedy)}`;
  }
}
