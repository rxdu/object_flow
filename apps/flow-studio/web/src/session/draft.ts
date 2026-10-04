// A session written as an example (docs/design/flow-studio.md §5): what was
// done to the sandbox since it was reset, in the examples format's own
// notation (flow-format.md §11.3, §11.4), its last free request the one
// under test, and the expectation proposed from what that request did. The
// author confirms the claim and writes the description; the flow checker
// decides whether it holds before it is saved.

import type {
  AttributeSpec,
  RequestBody,
  Verdict,
  WireValue,
} from "../api/wire";
import { durationSeconds } from "./notation";
import { OPERATOR, type Player, type TrailEntry } from "./player";

const UNITS: [string, string, number][] = [
  ["week", "weeks", 604800],
  ["day", "days", 86400],
  ["h", "h", 3600],
  ["min", "min", 60],
  ["s", "s", 1],
];

/** Seconds as the model writes a duration, in its largest whole unit. */
export function durationText(seconds: number): string {
  for (const [one, many, size] of UNITS) {
    if (seconds % size === 0 && (seconds !== 0 || size === 1)) {
      const n = seconds / size;
      return `${n} ${Math.abs(n) === 1 ? one : many}`;
    }
  }
  return `${seconds} s`;
}

/** The verdict names an expectation may hold (flow-format.md §11.4). */
const EXPECTABLE = new Set([
  "unsatisfied",
  "unavailable",
  "invalid_input",
  "not_requestable",
  "over_limit",
  "invariant_violated",
  "call_refused",
]);

export interface Draft {
  example: Record<string, unknown> | null;
  /** why the session cannot be written as given, or what was left out */
  problems: string[];
  notes: string[];
}

/** The session the player holds, written as an example. */
export async function draftExample(
  p: Player,
  description: string,
): Promise<Draft> {
  const problems: string[] = [];
  const notes: string[] = [];
  const trail = p.trail;
  let last = -1;
  trail.forEach((e, i) => {
    if (e.kind === "free") last = i;
  });
  if (last === -1) {
    return {
      example: null,
      problems: [
        "Nothing was requested in free play, so no request is under test.",
      ],
      notes,
    };
  }

  // names, as the example's own aliases name objects, and new ones for the
  // objects free play created, by type
  const names = new Map<string, string>();
  for (const [alias, id] of p.aliases)
    if (alias !== OPERATOR) names.set(id, alias);
  const taken = new Set(names.values());
  const fresh = async (id: string): Promise<string> => {
    const known = names.get(id);
    if (known) return known;
    const base = (await p.typeOf(id)).replace(
      /[A-Z]/g,
      (c, i: number) => (i ? "_" : "") + c.toLowerCase(),
    );
    let n = 1;
    while (taken.has(`${base}${n}`)) n += 1;
    const name = `${base}${n}`;
    names.set(id, name);
    taken.add(name);
    return name;
  };
  const nameOf = (id: string): string => {
    const name = names.get(id);
    if (name === undefined) {
      problems.push(
        `${id} is named by no step, since a request created it without naming it.`,
      );
      return id;
    }
    return name;
  };

  const answers = new Map<string, unknown>();
  let pending = 0;
  const steps: Record<string, unknown>[] = [];

  async function written(
    e: Extract<TrailEntry, { kind: "free" }>,
  ): Promise<Record<string, unknown>> {
    const b: RequestBody = e.body;
    const r: Record<string, unknown> = {};
    const type = b.type ?? (b.object_id ? await p.typeOf(b.object_id) : "");
    if (b.type) r["type"] = b.type;
    else if (b.object_id) r["object"] = nameOf(b.object_id);
    r["transition"] = b.transition;
    if (b.actor.id !== OPERATOR) r["actor"] = nameOf(b.actor.id);
    if (pending) r["after"] = durationText(pending);
    pending = 0;
    if (b.occurred_at) r["occurred_at"] = b.occurred_at;
    if (b.inputs && Object.keys(b.inputs).length > 0) {
      const d = await p.decl(type);
      const x = d.transitions?.[b.transition];
      const specs: Record<string, AttributeSpec> = {
        ...(d.attributes ?? {}),
        ...(x?.inputs ?? {}),
      };
      const enums = d.enumerations ?? {};
      const inputs: Record<string, unknown> = {};
      for (const [k, v] of Object.entries(b.inputs)) {
        inputs[k] = example(v, specs[k], enums, nameOf);
      }
      r["inputs"] = inputs;
    }
    if (e.verdict.kind === "satisfied") {
      if (b.type) r["as"] = await fresh(e.verdict.object_id);
      if (e.created.length > 0) {
        // the objects its effect created, named in the order created, as
        // far as they are of one type (flow-format.md §11.3)
        const first = await p.typeOf(e.created[0] ?? "");
        const same: string[] = [];
        for (const id of e.created) {
          if ((await p.typeOf(id)) !== first) break;
          same.push(await fresh(id));
        }
        r["creates"] = same;
      }
    }
    return r;
  }

  for (let i = 0; i < last; i += 1) {
    const e = trail[i];
    if (!e) continue;
    if (e.kind === "evaluators") {
      for (const [k, v] of Object.entries(e.answers)) {
        if (answers.has(k) && answers.get(k) !== v) {
          problems.push(
            `${k} answered both ${String(answers.get(k))} and ${String(v)}, which one example cannot say.`,
          );
        }
        answers.set(k, v);
      }
    } else if (e.kind === "clock") {
      pending += durationSeconds(e.by);
    } else if (e.verdict !== null && e.verdict.kind !== "satisfied") {
      notes.push(
        "A refused request is left out, since an example's steps must apply.",
      );
    } else if (e.kind === "step") {
      const step = structuredClone(e.step) as Record<
        string,
        Record<string, unknown>
      >;
      const inner = Object.values(step)[0];
      if (pending && inner) {
        inner["after"] = durationText(
          pending +
            (inner["after"] ? durationSeconds(String(inner["after"])) : 0),
        );
        pending = 0;
      }
      steps.push(step);
    } else {
      steps.push({ request: await written(e) });
    }
  }

  const tested = trail[last] as Extract<TrailEntry, { kind: "free" }>;
  const request = await written(tested);
  const expect = await expectation(p, tested.body, tested.verdict, problems);
  const out: Record<string, unknown> = { description: description.trim() };
  if (answers.size > 0) out["evaluators"] = Object.fromEntries(answers);
  if (steps.length > 0) out["steps"] = steps;
  out["request"] = request;
  out["expect"] = expect;
  return { example: problems.length ? null : out, problems, notes };
}

/** A wire value as an example writes it: an object by its name, an
 * enumeration member qualified, the rest as it is. */
function example(
  v: WireValue,
  spec: AttributeSpec | undefined,
  enums: Record<string, string[]>,
  nameOf: (id: string) => string,
): unknown {
  if (v === null) return null;
  if (spec?.reference)
    return Array.isArray(v)
      ? v.map((x) => nameOf(String(x)))
      : nameOf(String(v));
  const typ = (spec?.type ?? "").replace(/\[\]$/, "");
  if (typ in enums) {
    const q = (x: WireValue) =>
      String(x).includes(".") ? x : `${typ}.${String(x)}`;
    return Array.isArray(v) ? v.map(q) : q(v);
  }
  return v;
}

/** What the request under test did, proposed as the claim an example makes
 * of it (flow-format.md §11.4). */
async function expectation(
  p: Player,
  body: RequestBody,
  v: Verdict,
  problems: string[],
): Promise<Record<string, unknown>> {
  if (v.kind === "satisfied") {
    const obj = await p.wire.get(v.object_id);
    return { verdict: "applied", state: obj?.state };
  }
  if (!EXPECTABLE.has(v.kind)) {
    problems.push(
      `The request under test is refused as ${v.kind}, which an example cannot expect.`,
    );
    return { verdict: v.kind };
  }
  if (v.kind === "unsatisfied")
    return { verdict: v.kind, clause: v.clause, remedy: v.remedy };
  if (v.kind === "invariant_violated") {
    const first = v.failures[0];
    const requested =
      body.type ?? (body.object_id ? await p.typeOf(body.object_id) : "");
    const out: Record<string, unknown> = { verdict: v.kind };
    if (first)
      out["clause"] =
        first.type === requested
          ? first.invariant
          : `${first.type}.${first.invariant}`;
    if (v.remedy) out["remedy"] = v.remedy;
    return out;
  }
  if (v.kind === "call_refused") {
    return { verdict: v.kind, clause: v.verdict.clause, remedy: v.remedy };
  }
  return { verdict: v.kind };
}
