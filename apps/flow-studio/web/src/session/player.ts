// An example played through the wire a step at a time, as a caller would
// (docs/design/flow-studio.md §5, §6). Every step is a library operation or a
// sandbox control, and every expectation is checked against what `request`,
// `get` and `history` report. An example needing what the interface does not
// yet offer is `waiting`, with the reason (ADR-0150, Consequences).

import {
  type AttributeSpec,
  type Declaration,
  type FlowEvent,
  type ModuleFile,
  NotRun,
  type PublishReport,
  type RequestBody,
  type Verdict,
  type Source,
  type Wire,
  type WireValue,
  isLegacy,
} from "../api/wire";
import {
  durationSeconds,
  instantSeconds,
  resolveInstant,
  sameDecimal,
} from "./notation";

export const OPERATOR = "operator";

export interface StepRequest {
  type?: string;
  object?: string;
  transition?: string;
  as?: string;
  creates?: string[];
  actor?: string;
  after?: string;
  occurred_at?: string;
  inputs?: Record<string, unknown>;
  proposal?: string;
  /** an import's: the state it arrives in, and its attribute values */
  state?: string;
  values?: Record<string, unknown>;
}

/** A `publish` step: the files of another version, beside the examples
 * file, its actor and when (flow-format.md §11.3, ADR-0166). */
export interface PublishStep {
  source: string[];
  actor?: string;
  after?: string;
}

export type ExampleStep = Partial<
  Record<
    "request" | "propose" | "approve" | "reject" | "withdraw" | "import",
    StepRequest
  >
> & { publish?: PublishStep };

/** Where a `publish` step finds the files it names: each beside the
 * examples file, by name, null where there is none; and the examples file's
 * own name, its change's source reference. */
export interface Beside {
  read(name: string): Promise<string | null>;
  examples: string;
}

/** The files beside a module's examples file, each read from its module's
 * bundle in the studio's catalogue. */
export function besideModule(
  wire: Wire,
  examples: string,
  source?: Source,
): Beside {
  // in the examples file's own source, and else in the design's, as a
  // module's imports are found
  const sources = [...new Set([source ?? "design", "design"])] as Source[];
  return {
    examples,
    read: async (name) => {
      for (const s of sources) {
        try {
          const bundle = await wire.module(name, s);
          const text = bundle.files.find((f) => f.name === name)?.text;
          if (text !== undefined) return text;
        } catch {
          // not in this source; the next is tried
        }
      }
      return null;
    },
  };
}

/** The name a file declares, which a change's source keys it by: its
 * module's, or `<module>.examples` for an examples file (ADR-0155). */
export function declaredName(text: string): string | null {
  const module = /^module:\s*([A-Za-z_][A-Za-z0-9_]*)\s*$/m.exec(text)?.[1];
  if (module) return module;
  const examples = /^examples_for:\s*([A-Za-z_][A-Za-z0-9_]*)\s*$/m.exec(
    text,
  )?.[1];
  return examples ? `${examples}.examples` : null;
}

export interface Expectation {
  verdict: string;
  clause?: string;
  remedy?: string;
  state?: string;
  values?: Record<string, unknown>;
  occurred_at?: string;
  cascaded?: string[];
  warned?: string[];
  proposal?: string;
}

interface Part {
  description: string;
  given?: string;
  evaluators?: Record<string, unknown>;
  steps?: ExampleStep[];
}

export interface Example extends Part {
  request?: StepRequest;
  approve?: StepRequest;
  expect: Expectation;
}

export interface ExamplesDoc {
  examples_for: string;
  setups?: Record<string, Part>;
  examples: Record<string, Example>;
}

/** One thing the player does: answer evaluators, take a setup's step, or
 * make the request under test. */
export type PlanStep =
  | { kind: "evaluators"; label: string; answers: Record<string, unknown> }
  | { kind: "step"; label: string; step: ExampleStep }
  | { kind: "tested"; label: string; step: ExampleStep; expect: Expectation };

export type Outcome = "passed" | "failed" | "not run" | "waiting";

export class Waiting extends Error {}

/** One thing done to the sandbox since it was reset, kept so a session can
 * be written as an example (docs/design/flow-studio.md §5). */
export type TrailEntry =
  | { kind: "evaluators"; answers: Record<string, unknown> }
  | { kind: "clock"; by: string }
  /** a step's verdict, or null for an import or a publish, which no
   * verdict answers; and why it cannot be written as an example's step,
   * where it cannot */
  | {
      kind: "step";
      step: ExampleStep;
      verdict: Verdict | null;
      unwritable?: string;
    }
  | {
      kind: "free";
      body: RequestBody;
      verdict: Verdict;
      /** the objects it created other than its own, in the order created */
      created: string[];
    };

class Failed extends Error {}

const VERBS = ["request", "propose", "approve", "reject", "withdraw"] as const;
const BUILT_IN: Record<string, string> = {
  id: "text",
  open: "bool",
  created_at: "timestamp",
  created_by_kind: "text",
  recorded_from: "timestamp",
  declaration_version: "int",
};

function chain(setups: Record<string, Part>, name: string): Part[] {
  const s = setups[name];
  if (!s) throw new Error(`no setup ${name}`);
  return [...(s.given ? chain(setups, s.given) : []), s];
}

/** The steps an example takes, its setups' first. */
export function plan(doc: ExamplesDoc, name: string): PlanStep[] {
  const ex = doc.examples[name];
  if (!ex) throw new Error(`no example ${name}`);
  const setups = doc.setups ?? {};
  const parts: [string, Part][] = [
    ...(ex.given ? chain(setups, ex.given) : []).map((p) => {
      const label =
        Object.entries(setups).find(([, s]) => s === p)?.[0] ?? "setup";
      return [`setup ${label}`, p] as [string, Part];
    }),
    [`example ${name}`, ex],
  ];
  const out: PlanStep[] = [];
  for (const [label, part] of parts) {
    if (part.evaluators) {
      out.push({
        kind: "evaluators",
        label: `${label}: evaluators`,
        answers: part.evaluators,
      });
    }
    (part.steps ?? []).forEach((step, i) => {
      out.push({ kind: "step", label: `${label}, step ${i + 1}`, step });
    });
  }
  const tested: ExampleStep = ex.approve
    ? { approve: ex.approve }
    : { request: ex.request ?? {} };
  out.push({
    kind: "tested",
    label: `example ${name}, the request under test`,
    step: tested,
    expect: ex.expect,
  });
  return out;
}

/** What a step asks for, as a person reads it: `ServiceJob.open as job`. */
export function summary(step: ExampleStep): string {
  if (step.import) return `import ${step.import.type ?? ""}`;
  if (step.publish) return `publish ${step.publish.source.join(", ")}`;
  const [verb, r] = verbOf(step);
  if (verb === "approve" || verb === "reject" || verb === "withdraw") {
    return `${verb} ${r.proposal ?? ""}`;
  }
  const target = r.type ?? r.object ?? "";
  const as = r.as ? ` as ${r.as}` : r.proposal ? ` as ${r.proposal}` : "";
  const by = r.actor ? ` by ${r.actor}` : "";
  const after = r.after ? `after ${r.after}, ` : "";
  return `${after}${verb === "propose" ? "propose " : ""}${target}.${r.transition ?? ""}${as}${by}`;
}

function verbOf(step: ExampleStep): [(typeof VERBS)[number], StepRequest] {
  for (const v of VERBS) {
    const r = step[v];
    if (r) return [v, r];
  }
  throw new Error("a step names no request");
}

/** One example's run: the clock it knows and the aliases it has bound. */
export class Player {
  readonly wire: Wire;
  now: number;
  readonly aliases = new Map<string, string>([[OPERATOR, OPERATOR]]);
  /** every object a request of the session wrote, in the order first
   * written, read from the events each appended (ADR-0151) */
  readonly touched: string[] = [];
  private readonly types = new Map<string, string>();
  private readonly decls = new Map<string, Declaration>();

  /** where a `publish` step reads its files, none where nothing gives
   * them, which leaves the step not run */
  readonly beside: Beside | undefined;

  constructor(wire: Wire, now: string, beside?: Beside) {
    this.wire = wire;
    this.now = instantSeconds(now);
    this.beside = beside;
  }

  /** A publish step: a change drafted with the files it names, each keyed
   * by the name it declares and its source reference the examples file's,
   * then submitted and approved as the step's actor; a refusal at any of
   * the three fails the example, and the steps after it read the version
   * installed (flow-format.md §11.3, ADR-0166 decision 3). */
  async published(pb: PublishStep): Promise<void> {
    if (pb.after)
      this.now = instantSeconds(
        (await this.wire.clock({ advance: pb.after })).now,
      );
    const source: Record<string, string> = {};
    for (const f of pb.source) {
      const text = this.beside ? await this.beside.read(f) : null;
      if (text === null)
        throw new NotRun(
          501,
          `the publish step's file ${f}, which no directory beside the examples file gives`,
        );
      source[declaredName(text) ?? f] = text;
    }
    const actor = { id: this.alias(pb.actor ?? OPERATOR) };
    const drafted = await this.wire.request({
      type: "DeclarationChange",
      transition: "draft",
      actor,
      // a change's source is a map of its files, which the wire carries and
      // `WireValue`, a request's value, does not describe
      inputs: {
        source: source as unknown as WireValue,
        source_ref: `example:${this.beside?.examples ?? ""}`,
      },
    });
    if (drafted.kind !== "satisfied")
      throw new Failed(`the publish's change is not drafted: ${drafted.kind}`);
    const change = drafted.object_id;
    const submitted = await this.wire.request({
      transition: "submit",
      actor,
      object_id: change,
    });
    if (submitted.kind !== "satisfied")
      throw new Failed(
        `the publish is refused at submission as ${submitted.kind}`,
      );
    const version = (await this.wire.get(change))?.version ?? 0;
    const { verdict } = await this.wire.publish(actor.id, change, {
      expected_version: version,
    });
    if (verdict?.kind !== "satisfied")
      throw new Failed(
        `the publish is refused at approval as ${verdict?.kind ?? "nothing"}`,
      );
    this.decls.clear(); // each type as the version installed has it
  }

  alias(a: string): string {
    const id = this.aliases.get(a);
    if (id === undefined) throw new Failed(`no object is named ${a}`);
    return id;
  }

  async decl(type: string): Promise<Declaration> {
    let d = this.decls.get(type);
    if (!d) {
      d = await this.wire.declaration(type);
      this.decls.set(type, d);
    }
    return d;
  }

  async typeOf(id: string): Promise<string> {
    let t = this.types.get(id);
    if (t === undefined) {
      const obj = await this.wire.get(id);
      if (!obj) throw new Failed(`no object ${id}`);
      t = obj.type;
      this.types.set(id, t);
    }
    return t;
  }

  /** An example's written value as the wire carries it. */
  wireValue(v: unknown, spec: AttributeSpec | undefined): WireValue {
    if (v === null || v === undefined) return null;
    if (spec?.reference) {
      return spec.reference.endsWith("[]")
        ? (v as string[]).map((a) => this.alias(a))
        : this.alias(String(v));
    }
    const typ = spec?.type ?? "";
    if (typ.endsWith("[]")) {
      return (v as unknown[]).map((x) =>
        this.wireValue(x, { type: typ.slice(0, -2) }),
      );
    }
    if (typ === "timestamp") return resolveInstant(String(v), this.now);
    // a decimal is a string on the wire, never a binary float (ADR-0150)
    if (typ.startsWith("decimal") && typeof v === "number") return String(v);
    if (typeof v === "string" && /^[A-Z]/.test(typ) && v.includes(".")) {
      return v.slice(v.indexOf(".") + 1); // an enumeration member, by its name
    }
    return v as WireValue;
  }

  /** Answer the evaluators, or take one step; a verdict for a step taken. */
  async perform(ps: PlanStep): Promise<Verdict | null> {
    if (ps.kind === "evaluators") {
      await this.answer(ps.answers);
      return null;
    }
    const v = await this.play(ps.step);
    this.trail.push({ kind: "step", step: ps.step, verdict: v });
    return v;
  }

  /** Set what evaluators answer, as an example's `evaluators` does. */
  async answer(answers: Record<string, unknown>): Promise<void> {
    await this.wire.evaluators(answers);
    this.trail.push({ kind: "evaluators", answers });
  }

  /** The type a family's legacy keys are qualified by: its farthest base,
   * or itself (ADR-0154, C23). */
  async root(type: string): Promise<string> {
    const seen = new Set<string>();
    let at = type;
    for (;;) {
      const base = (await this.decl(at)).extends;
      if (typeof base !== "string" || seen.has(at)) return at;
      seen.add(at);
      at = base;
    }
  }

  /** An import step: a one-object batch, its legacy key the step's alias,
   * with an empty disposition file, so it admits nothing; the object is then
   * found by its legacy key (ADR-0154). */
  async imported(im: StepRequest): Promise<void> {
    if (im.after)
      this.now = instantSeconds(
        (await this.wire.clock({ advance: im.after })).now,
      );
    const type = im.type ?? "";
    const key = im.as ?? "";
    const specs = (await this.decl(type)).attributes ?? {};
    const attributes: Record<string, WireValue> = {};
    for (const [k, v] of Object.entries(im.values ?? {})) {
      // a reference names another imported object by its legacy key, which
      // is its alias
      attributes[k] =
        specs[k] && "reference" in specs[k]
          ? (v as WireValue)
          : this.wireValue(v, specs[k]);
    }
    const report = await this.wire.importBatch(OPERATOR, [
      { type, legacy_key: key, state: im.state ?? "", attributes },
    ]);
    if (report.refusal)
      throw new Failed(
        `the import of ${key} is refused: ${report.refusal.reason}`,
      );
    const found = await this.wire.lookup(
      OPERATOR,
      "legacy",
      `${await this.root(type)}:${key}`,
    );
    if (!found)
      throw new Failed(`the import of ${key} wrote nothing lookup finds`);
    this.aliases.set(key, found.id);
    this.types.set(found.id, found.type);
  }

  async play(step: ExampleStep): Promise<Verdict | null> {
    if (step.import) {
      await this.imported(step.import);
      return null;
    }
    if (step.publish) {
      await this.published(step.publish);
      return null;
    }
    const [verb, r] = verbOf(step);
    if (r.after)
      this.now = instantSeconds(
        (await this.wire.clock({ advance: r.after })).now,
      );
    const actor = { id: this.alias(r.actor ?? OPERATOR) };
    if (verb === "approve" || verb === "reject" || verb === "withdraw") {
      const decided = await this.wire.request({
        transition: verb,
        actor,
        object_id: this.alias(r.proposal ?? ""),
      });
      if (decided.kind === "satisfied") await this.note(decided);
      return decided;
    }
    const body: RequestBody = { transition: r.transition ?? "", actor };
    let type: string;
    if (r.type) {
      type = body.type = r.type;
    } else {
      body.object_id = this.alias(r.object ?? "");
      type = await this.typeOf(body.object_id);
    }
    const d = await this.decl(type);
    const x = d.transitions?.[body.transition];
    const specs: Record<string, AttributeSpec> = {
      ...(d.attributes ?? {}),
      ...(x?.inputs ?? {}),
    };
    if (r.inputs) {
      body.inputs = Object.fromEntries(
        Object.entries(r.inputs).map(([k, v]) => [
          k,
          this.wireValue(v, specs[k]),
        ]),
      );
    }
    if (r.occurred_at)
      body.occurred_at = resolveInstant(r.occurred_at, this.now);
    if (verb === "propose") body.propose = true;
    const v = await this.wire.request(body);
    if (v.kind === "satisfied") {
      if (verb === "propose" && r.proposal)
        this.aliases.set(r.proposal, v.object_id);
      else if (r.as) {
        this.aliases.set(r.as, v.object_id);
        this.types.set(v.object_id, type);
      }
      await this.note(v);
      if (r.creates && r.creates.length > 0)
        await this.nameCreated(r.creates, v);
    }
    return v;
  }

  /** What the session did beyond its example: each request a person made in
   * free play, and its verdict, in order. */
  readonly log: { summary: string; verdict: Verdict | null }[] = [];
  /** everything done to the sandbox since it was reset, in order */
  readonly trail: TrailEntry[] = [];

  /** A request a person makes in free play, as any caller makes it: its
   * verdict is logged, and what it wrote is listed. */
  async request(body: RequestBody, summary: string): Promise<Verdict> {
    const v = await this.wire.request(body);
    let created: string[] = [];
    if (v.kind === "satisfied") {
      const events = await this.appended(v);
      for (const e of events) {
        if (!this.touched.includes(e.object_id)) this.touched.push(e.object_id);
      }
      created = events
        .slice(1)
        .filter((e) => e.from_state === null)
        .map((e) => e.object_id);
    }
    this.log.push({ summary, verdict: v });
    this.trail.push({ kind: "free", body, verdict: v, created });
    return v;
  }

  /** An object imported in free play, as an example's `import` step writes
   * it (flow-format.md §11.3): its legacy key the alias later steps name it
   * by, logged, and drafted as that step. A refused import throws, naming
   * why, and leaves nothing behind. */
  async importStep(im: StepRequest): Promise<void> {
    await this.imported(im);
    this.trail.push({ kind: "step", step: { import: im }, verdict: null });
    this.log.push({
      summary: `import ${im.type ?? ""} as ${im.as ?? ""}`,
      verdict: null,
    });
  }

  /** A version the session would publish (ADR-0166): a change drafted with
   * the files given, each keyed by the name it declares and its source
   * reference the studio's, then submitted, which runs its dry run. The
   * report is read again, so a submission its dry run refused still shows
   * what refused it. Nothing is logged until the change is approved. */
  async draftVersion(
    files: ModuleFile[],
    label: string,
  ): Promise<{
    change: string;
    submitted: Verdict;
    report: PublishReport | null;
  }> {
    const source: Record<string, string> = {};
    for (const f of files) source[declaredName(f.text) ?? f.name] = f.text;
    const actor = { id: OPERATOR };
    const drafted = await this.wire.request({
      type: "DeclarationChange",
      transition: "draft",
      actor,
      // a change's source is a map of its files, which the wire carries and
      // `WireValue`, a request's value, does not describe
      inputs: {
        source: source as unknown as WireValue,
        source_ref: `studio:${label}`,
      },
    });
    if (drafted.kind !== "satisfied")
      throw new Failed(`the change is not drafted: ${drafted.kind}`);
    const change = drafted.object_id;
    const submitted = await this.wire.request({
      transition: "submit",
      actor,
      object_id: change,
    });
    const { report } = await this.wire.publish(OPERATOR, change, {
      dry_run: true,
    });
    return { change, submitted, report };
  }

  /** A submitted change approved, its version installed over the session's
   * objects: logged, and kept as a `publish` step of the files given, or,
   * for a version no file beside the examples file holds, with why it
   * cannot be written (flow-format.md §11.3, ADR-0166). */
  async approveVersion(
    change: string,
    label: string,
    step: PublishStep | null,
    unwritable?: string,
  ): Promise<Verdict | null> {
    const version = (await this.wire.get(change))?.version ?? 0;
    const { verdict } = await this.wire.publish(OPERATOR, change, {
      expected_version: version,
    });
    if (verdict?.kind === "satisfied") {
      this.decls.clear(); // each type as the version installed has it
      this.trail.push({
        kind: "step",
        step: { publish: step ?? { source: [] } },
        verdict: null,
        unwritable: step ? undefined : unwritable,
      });
      this.log.push({ summary: `publish ${label}`, verdict });
    }
    return verdict;
  }

  /** A drafted or submitted change withdrawn, which installs nothing and is
   * no part of the session. */
  async withdrawVersion(change: string): Promise<Verdict> {
    return this.wire.request({
      transition: "withdraw",
      actor: { id: OPERATOR },
      object_id: change,
    });
  }

  /** Every object the session knows, by the name the example gives it where
   * it gives one, with its type: what an input naming an object chooses
   * from. */
  async known(): Promise<{ id: string; name: string; type: string }[]> {
    const names = new Map([...this.aliases].map(([a, id]) => [id, a]));
    const ids = [
      ...[...this.aliases.values()].filter(
        (id) => id !== OPERATOR && !id.startsWith("proposal-"),
      ),
      ...this.touched,
    ];
    return Promise.all(
      [...new Set(ids)].map(async (id) => ({
        id,
        name: names.get(id) ?? id,
        type: await this.typeOf(id),
      })),
    );
  }

  /** Advance the sandbox's clock, as an example's `after` does. */
  async advance(by: string): Promise<void> {
    this.now = instantSeconds((await this.wire.clock({ advance: by })).now);
    this.trail.push({ kind: "clock", by });
  }

  /** Every object the request wrote, for the objects the page lists. */
  private async note(v: Verdict & { kind: "satisfied" }): Promise<void> {
    for (const e of await this.appended(v)) {
      if (!this.touched.includes(e.object_id)) this.touched.push(e.object_id);
    }
  }

  /** The events a satisfied request appended, read by position
   * (ADR-0151). */
  async appended(v: Verdict & { kind: "satisfied" }): Promise<FlowEvent[]> {
    const { events } = await this.wire.events(v.events);
    return events.filter((e): e is FlowEvent => e !== null);
  }

  /** Alias the objects the request created, other than its own, in the order
   * it created them: its creation events after the first (flow-format.md
   * §11.3). */
  private async nameCreated(
    names: string[],
    v: Verdict & { kind: "satisfied" },
  ): Promise<void> {
    const made = (await this.appended(v))
      .slice(1)
      .filter((e) => e.from_state === null)
      .map((e) => e.object_id);
    if (names.length > made.length) {
      throw new Failed(
        `a step names ${names.length} objects its request creates, and it creates ${made.length}`,
      );
    }
    const types = new Set(
      await Promise.all(made.slice(0, names.length).map((m) => this.typeOf(m))),
    );
    if (types.size > 1) {
      throw new Failed(
        "a step names objects its request creates, and they are of more than one type",
      );
    }
    names.forEach((alias, i) => {
      const id = made[i];
      if (id !== undefined) this.aliases.set(alias, id);
    });
  }

  /** The transitions a request caused, in the order each began: its events
   * after the first, a redaction left out, and a proposal's own events, an
   * approval's and an invalidation's, which are not transitions the request
   * took (flow-format.md §11.4). */
  async caused(v: Verdict & { kind: "satisfied" }): Promise<string[]> {
    const typed = await Promise.all(
      (await this.appended(v)).map(async (e) => ({
        e,
        type: await this.typeOf(e.object_id),
      })),
    );
    return typed
      .filter(({ type }) => type !== "Proposal")
      .slice(1)
      .filter(({ e }) => e.transition !== "redact")
      .map(({ e, type }) => `${type}.${e.transition}`);
  }

  private async requestedType(tested: ExampleStep): Promise<string | null> {
    const [verb, r] = verbOf(tested);
    if (verb === "approve") {
      const p = await this.wire.get(this.alias(r.proposal ?? ""));
      const target = p?.attributes["target"];
      return typeof target === "string" ? this.typeOf(target) : null;
    }
    return r.type ?? this.typeOf(this.alias(r.object ?? ""));
  }

  /** What differs between an expectation and what the wire reports, or null.
   * Throws Waiting or NotRun where it cannot tell. */
  async compare(
    want: Expectation,
    v: Verdict,
    tested: ExampleStep,
  ): Promise<string | null> {
    const got = v.kind === "satisfied" ? "applied" : v.kind;
    if (v.kind !== "satisfied") {
      if (want.verdict === "applied")
        return `expected it to apply, and it is refused as ${got}`;
      if (got !== want.verdict)
        return `expected ${want.verdict}, and it is ${got}`;
      let clause: string | null = null;
      let remedy: string | null;
      if (v.kind === "unsatisfied") {
        clause = v.clause;
        remedy = v.remedy;
      } else if (v.kind === "invariant_violated") {
        const first = v.failures[0];
        if (!first) return "an invariant violation names no failure";
        const owner = first.type;
        clause =
          owner === (await this.requestedType(tested))
            ? first.invariant
            : `${owner}.${first.invariant}`;
        remedy = v.remedy;
        if (want.remedy !== undefined && v.not_computed?.includes("remedy")) {
          throw new NotRun(
            501,
            "the remedy of an invariant that fails on an object other than the one requested",
          );
        }
      } else if (v.kind === "call_refused") {
        clause = v.verdict.clause;
        remedy = v.remedy;
      } else {
        remedy = v.remedy;
      }
      if (want.clause !== undefined && clause !== want.clause) {
        return `expected the clause ${want.clause}, and it is ${clause}`;
      }
      if (want.remedy !== undefined && remedy !== want.remedy) {
        return `expected the remedy ${want.remedy}, and it is ${remedy}`;
      }
      return null;
    }
    if (want.verdict !== "applied")
      return `expected ${want.verdict}, and the request applies`;
    const obj = await this.wire.get(v.object_id);
    if (!obj) return `the object ${v.object_id} cannot be read`;
    if (want.state !== undefined && obj.state !== want.state) {
      return `expected the state ${want.state}, and it is ${obj.state}`;
    }
    const d = await this.decl(obj.type);
    for (const [k, expected] of Object.entries(want.values ?? {})) {
      let spec = d.attributes?.[k];
      let have: WireValue | undefined;
      if (k in obj.attributes) have = obj.attributes[k];
      else if (k in BUILT_IN) {
        have = (obj as unknown as Record<string, WireValue>)[k];
        spec = { type: BUILT_IN[k] };
      } else return `${obj.type} has no member ${k} the wire reports`;
      const same = spec
        ? this.same(expected, have ?? null, spec)
        : looseEqual(expected, have ?? null);
      if (!same)
        return `expected ${k} to hold ${JSON.stringify(expected)}, and it holds ${JSON.stringify(have)}`;
    }
    if (want.occurred_at !== undefined) {
      const { events } = await this.wire.history(v.object_id);
      const mine = events.find(
        (e) => !isLegacy(e) && e.position === v.events[0],
      );
      if (
        !mine ||
        mine.occurred_at !== resolveInstant(want.occurred_at, this.now)
      ) {
        return "the occurred time differs";
      }
    }
    if (want.cascaded !== undefined) {
      const caused = await this.caused(v);
      if (JSON.stringify(caused) !== JSON.stringify(want.cascaded)) {
        return `expected the request to cause ${want.cascaded.join(", ") || "nothing"}, and it causes ${caused.join(", ") || "nothing"}`;
      }
    }
    if (want.warned !== undefined) {
      const warned = v.flags.map((f) => f.clause);
      if (JSON.stringify(warned) !== JSON.stringify(want.warned)) {
        return `expected warnings ${want.warned.join(", ")}, and they are ${warned.join(", ")}`;
      }
    }
    return null;
  }

  private same(
    expected: unknown,
    got: WireValue,
    spec: AttributeSpec,
  ): boolean {
    if (expected === null || expected === undefined || got === null) {
      return (expected ?? null) === got;
    }
    const typ = spec.type ?? "";
    if (spec.reference || typ.endsWith("[]")) {
      return (
        JSON.stringify(this.wireValue(expected, spec)) === JSON.stringify(got)
      );
    }
    if (typ === "timestamp") {
      return (
        instantSeconds(resolveInstant(String(expected), this.now)) ===
        instantSeconds(String(got))
      );
    }
    if (typ === "duration")
      return durationSeconds(String(expected)) === durationSeconds(String(got));
    if (typ.startsWith("decimal") || typ.startsWith("money")) {
      return sameDecimal(String(expected), String(got));
    }
    if (typ === "date") return String(expected) === got;
    return this.wireValue(expected, spec) === got;
  }
}

/** A derived attribute's value, which no declaration types: a number
 * compared exactly. */
function looseEqual(expected: unknown, got: WireValue): boolean {
  if (typeof expected === "boolean" || typeof got === "boolean")
    return expected === got;
  if (
    typeof expected === "number" &&
    (typeof got === "number" || typeof got === "string")
  ) {
    try {
      return sameDecimal(expected, got);
    } catch {
      return false;
    }
  }
  return JSON.stringify(expected) === JSON.stringify(got);
}

/** A whole example, played and compared: its outcome and why. */
export async function runExample(
  wire: Wire,
  doc: ExamplesDoc,
  name: string,
  beside?: Beside,
): Promise<{ outcome: Outcome; why: string }> {
  const { now } = await wire.reset();
  const p = new Player(wire, now, beside);
  try {
    for (const ps of plan(doc, name)) {
      const v = await p.perform(ps);
      if (ps.kind === "step" && v && v.kind !== "satisfied") {
        return {
          outcome: "failed",
          why: `${ps.label} is refused as ${v.kind}`,
        };
      }
      if (ps.kind === "tested" && v) {
        let problem = await p.compare(ps.expect, v, ps.step);
        if (
          problem === null &&
          ps.expect.proposal !== undefined &&
          ps.step.approve
        ) {
          const prop = await wire.get(p.alias(ps.step.approve.proposal ?? ""));
          // the format writes the state in lower case (§11.4, D521)
          if (prop?.state.toLowerCase() !== ps.expect.proposal) {
            problem = `expected the proposal ${ps.expect.proposal}, and it is ${prop?.state}`;
          }
        }
        return problem
          ? { outcome: "failed", why: problem }
          : { outcome: "passed", why: "" };
      }
    }
    return { outcome: "failed", why: "the example has no request under test" };
  } catch (e) {
    if (e instanceof Waiting) return { outcome: "waiting", why: e.message };
    if (e instanceof NotRun) return { outcome: "not run", why: e.message };
    if (e instanceof Failed) return { outcome: "failed", why: e.message };
    throw e;
  }
}
