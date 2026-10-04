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
// remedy means for the person reading.

import type { Verdict } from "../api/wire";

export function explain(v: Verdict): string {
  // TODO(author): one sentence per verdict for the person stepping through
  // a flow. For example, an unsatisfied `photo_attached` with remedy
  // `self_serviceable` might read "Refused by photo_attached: supply or
  // correct an input and try again."
  return v.kind;
}
