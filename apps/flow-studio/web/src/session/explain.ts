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

import type { Remedy, Verdict } from "../api/wire";

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
    case "unavailable":
      return `Not available ${v.state ? `in ${v.state}` : "now"}${next(v.remedy)}`;
    case "unknown_transition":
      return `${v.transition} is not a transition this request can take${next(v.remedy)}`;
    case "invalid_input":
      return `${
        v.inputs.length === 1
          ? `The input ${v.inputs[0]} is`
          : `The inputs ${listed(v.inputs)} are`
      } not valid${next(v.remedy)}`;
    case "not_requestable":
      return `Only ${listed(v.parents, "or")} may take it${next(v.remedy)}`;
    case "over_limit":
      return `It would pass a declared limit${next(v.remedy)}`;
    case "not_found":
      return `No object has that id${next(v.remedy)}`;
    case "stale":
      return `The object changed since it was read${next(v.remedy)}`;
  }
}
