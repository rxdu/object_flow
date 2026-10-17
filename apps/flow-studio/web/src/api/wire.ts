// The engine's wire (ADR-0150): one POST per library operation, the sandbox's
// controls, and the studio's own tooling. The shapes are the library's
// (docs/design/library-api.md §4 to §6); a backend that does not compute a
// field names it in `not_computed` rather than filling it in.

/** Why an input is not valid, from a closed set (ADR-0173 decision 9). */
export type InputReason =
  | "unknown_input"
  | "missing"
  | "wrong_type"
  | "malformed"
  | "wrong_currency"
  | "no_such_object"
  | "out_of_range"
  | "not_allowed";

export type Remedy =
  | "self_serviceable"
  | "delegable"
  | "temporal"
  | "dependent"
  | "unreachable_from_here";

/** A file's wire form: its content's hash, its size and its media type
 * (ADR-0173 decision 11). */
export interface FileValue {
  hash: string;
  size: number;
  media_type: string;
}

export type WireValue =
  null | boolean | number | string | FileValue | WireValue[];

interface Reported {
  not_computed?: string[];
}

export interface Flag {
  clause: string;
  remedy: Remedy | null;
}

export interface InvariantFailure {
  object_id: string;
  /** the object's type, which no read gives for an object the refused
   * request would have created (ADR-0151) */
  type: string;
  invariant: string;
  remedy: Remedy | null;
}

export type Verdict = Reported &
  (
    | {
        kind: "satisfied";
        object_id: string;
        version: number | null;
        events: number[];
        /** each object the request created, its own first where it is a creation */
        created: string[];
        replayed: boolean;
        flags: Flag[];
      }
    | { kind: "unsatisfied"; clause: string; remedy: Remedy }
    | {
        kind: "invariant_violated";
        failures: InvariantFailure[];
        remedy: Remedy | null;
      }
    | {
        kind: "call_refused";
        step: string;
        object_id: string | null;
        remedy: Remedy;
        verdict: { kind: string | null; clause: string | null; remedy: Remedy };
      }
    | { kind: "unavailable"; state: string | null; remedy: Remedy }
    | { kind: "unknown_transition"; transition: string; remedy: Remedy }
    // a request naming no type of the installed version (ADR-0210 decision 6)
    | { kind: "unknown_type"; type: string; remedy: Remedy }
    | {
        kind: "invalid_input";
        inputs: string[];
        /** why each input is at fault (ADR-0173 decision 9) */
        reasons?: Record<string, InputReason>;
        remedy: Remedy;
      }
    | { kind: "not_requestable"; parents: string[]; remedy: Remedy }
    | { kind: "not_proposable"; remedy: Remedy }
    | { kind: "over_limit"; remedy: Remedy }
    | { kind: "not_found"; remedy: Remedy }
    | { kind: "stale"; remedy: Remedy }
  );

export interface FlowObject extends Reported {
  id: string;
  type: string;
  state: string;
  version?: number;
  declaration_version?: number;
  created_at?: string;
  created_by_kind?: string;
  recorded_from?: string;
  open?: boolean;
  attributes: Record<string, WireValue>;
}

export interface FlowEvent {
  position: number;
  object_id: string;
  transition: string;
  from_state: string | null;
  to_state: string;
  changes_state: boolean;
  actor_id: string;
  actor_kind: string;
  declaration_version: number;
  recorded_at: string;
  occurred_at: string;
  imported: boolean;
}

/** History from before the store, attached to an object at import, which
 * `history` returns before its events (library-api.md §6, ADR-0163). */
export interface LegacyEntry {
  legacy: true;
  object_id: string;
  ordinal: number;
  kind: string;
  occurred_at: string;
  payload: Record<string, unknown>;
}

/** What `history` returns: an object's legacy entries, then its events. */
export type HistoryItem = FlowEvent | LegacyEntry;

export function isLegacy(e: HistoryItem): e is LegacyEntry {
  return "legacy" in e;
}

export interface RequestBody {
  transition: string;
  actor: { id: string; principal?: string };
  object_id?: string;
  type?: string;
  inputs?: Record<string, WireValue>;
  occurred_at?: string;
  propose?: boolean;
}

export type Declaration = Record<string, unknown> & {
  attributes?: Record<string, AttributeSpec>;
  transitions?: Record<string, TransitionSpec>;
  conditions?: Record<string, { expression: string; remedy?: Remedy }>;
  states?: Record<string, { category?: string; final?: boolean }>;
  /** only the import writes its objects (flow-format.md §4.15) */
  mirror?: boolean;
  metrics?: Record<string, unknown>;
  /** the members of each enumeration the type's attributes are of */
  enumerations?: Record<string, string[]>;
};

export type Availability = "available" | "available_with_input" | "unavailable";

/** What a transition offers its actor now (library-api.md §5, ADR-0152). */
export interface TransitionOffer {
  transition: string;
  /** the observation kind a recording creates; otherwise null */
  creates: string | null;
  availability: Availability;
  verdict: Verdict | null;
  /** each input by name, with the type a caller supplies */
  inputs: Record<string, string>;
  unevaluated: string[];
  flags: Flag[];
  /** it writes who takes it into what an invariant reads, so it may be
   * refused for who takes it, which no offer can tell (ADR-0173) */
  actor_dependent?: boolean;
}

export interface MetricPage extends Reported {
  rows: { dimensions: Record<string, WireValue>; value: WireValue }[];
  cursor: string | null;
  flags: unknown[];
  /** why the read cannot be answered, where it cannot: `unknown_metric`,
   * `unknown_dimension` among them (`library-api.md` §5) */
  refused: string | null;
}

export interface AttributeSpec {
  type?: string;
  reference?: string;
}

export interface TransitionSpec {
  kind?: string;
  /** a request for it may be filed as a proposal (DESIGN.md §9) */
  proposable?: boolean;
  from?: string[] | string;
  to?: string;
  guards?: Record<string, string>;
  inputs?: Record<string, AttributeSpec>;
  required_inputs?: string[];
  optional_inputs?: string[];
}

export interface ModuleEntry {
  file: string;
  /** the design's example modules are read-only; the workspace's are the
   * author's to edit */
  source: Source;
  editable: boolean;
  module: string;
  imports: string[];
  types: string[];
  examples: string | null;
}

export type Source = "design" | "workspace";

export interface ModuleFile {
  name: string;
  text: string;
}

/** A finding of the flow checker against a file's line (flow-format.md
 * §11.5). */
export interface Finding {
  file: string;
  line: number;
  severity: "fatal" | "notice";
  code: string;
  message: string;
}

export interface ModuleBundle {
  files: ModuleFile[];
  examples: (ModuleFile & { doc: unknown }) | null;
}

export type LoadResult =
  | {
      loaded: true;
      modules: string[];
      notices: number;
      now: string;
      /** every type the modules declare, which a caller may create */
      types: string[];
      /** every evaluator function, as `<evaluator>.<function>` */
      evaluators: string[];
    }
  | {
      loaded: false;
      findings: { file: string; line: number; code: string; message: string }[];
    };

/** One object of an import batch, as the mapping produced it (ADR-0154). */
/** One finding of a change's dry run: what it is, how many objects it
 * reaches, and the first of them (publish-and-import.md §2). */
export interface PublishFinding {
  severity: "fatal" | "decision" | "notice";
  code: string;
  message: string;
  affected: number;
  sample: string[];
}

/** A change's impact report, as its dry run computes it: the version it
 * would install, whether it may be approved, its findings, every object
 * each category reaches, by id, and what the version declares that its
 * reviewer reads (publish-and-import.md §2). */
export interface PublishReport {
  version: number;
  accepted: boolean;
  now: string;
  findings: PublishFinding[];
  affected: Record<string, string[]>;
  proposals_invalidated: number;
  /** each invariant's form, `Type.invariant` to its form (ADR-0055) */
  invariant_forms?: Record<string, string>;
  /** each invariant's transitions that could break it, as `Type.transition`
   * (DESIGN.md §5.6) */
  could_violate?: Record<string, string[]>;
  /** each metric dimension that names a person,
   * `Type.metric.dimension[.path]` */
  person_dimensions?: string[];
  not_computed?: string[];
}

export interface ImportedObject {
  type: string;
  legacy_key: string;
  state: string;
  /** values in the wire's notation, a reference by legacy key */
  attributes: Record<string, WireValue>;
}

/** A sequence an import raises past the last number a legacy system minted
 * from it, in a scope where it is scoped, `scope` empty otherwise
 * (`library-api.md` §5 `SequenceValue`). */
export interface RaisedSequence {
  sequence: string;
  last: number;
  scope?: string;
}

/** What an import wrote, the classes it found, and why it was refused,
 * where it was (`library-api.md` §5, ADR-0154). */
export interface ImportReport {
  written: number;
  findings: Record<string, unknown>[];
  refused_types: string[];
  refusal: { type: string; legacy_key: string; reason: string } | null;
}

/** A request the backend cannot answer: the wire's 4xx or 5xx. */
export class WireFault extends Error {
  readonly status: number;

  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

/** A construct the backend cannot run, which it never guesses (a 501). */
export class NotRun extends WireFault {}

/** what answers the wire: the reference runner, or the Rust core's service
 * the studio's server forwards to (docs/design/flow-studio.md §2) */
export interface Backend {
  backend: "runner" | "core";
  service?: string;
}

export interface Wire {
  request(body: RequestBody): Promise<Verdict>;
  get(id: string): Promise<FlowObject | null>;
  /** null where no object has the id (ADR-0173 decision 12) */
  history(
    id: string,
  ): Promise<{ events: HistoryItem[]; not_computed: string[] } | null>;
  /** the event at each position given, in the order given, and null where
   * the log holds none (ADR-0151) */
  events(
    positions: number[],
  ): Promise<{ events: (FlowEvent | null)[]; not_computed: string[] }>;
  declaration(type: string): Promise<Declaration>;
  /** a batch of a port, with an empty disposition file (ADR-0154) */
  importBatch(
    actor: string,
    objects: ImportedObject[],
    sequences?: RaisedSequence[],
  ): Promise<ImportReport>;
  /** the object an external identifier names, a legacy key among them */
  lookup(
    actor: string,
    source: string,
    value: string,
  ): Promise<FlowObject | null>;
  /** null where no object has the id (ADR-0173 decision 12) */
  availability(actor: string, id: string): Promise<TransitionOffer[] | null>;
  /** the library's `publish`: a change's dry run, or its approval of the
   * change's version its approver read (library-api.md §6, ADR-0166) */
  publish(
    actor: string,
    change: string,
    how: { dry_run: true } | { expected_version: number },
  ): Promise<{ verdict: Verdict | null; report: PublishReport | null }>;
  /** A reader's read of a metric: a row per group of the dimensions
   *  `keep` names, every declared one where it names none. */
  metric(actor: string, name: string, keep?: string[]): Promise<MetricPage>;
  load(files: ModuleFile[]): Promise<LoadResult>;
  reset(): Promise<{ now: string }>;
  clock(
    change: { advance: string } | { set: string },
  ): Promise<{ now: string }>;
  evaluators(answers: Record<string, unknown>): Promise<unknown>;
  backend(): Promise<Backend>;
  modules(): Promise<ModuleEntry[]>;
  module(file: string, source?: Source): Promise<ModuleBundle>;
  diagram(
    file: string,
    type: string,
    source?: Source,
  ): Promise<{ mermaid: string }>;
  /** the studio's editing tools (docs/design/flow-studio.md §3.1, §5) */
  /** the checker's findings over files, and, where none is fatal, the
   * types they declare in the order a load lists them, with the file
   * declaring each */
  check(files: ModuleFile[]): Promise<{
    findings: Finding[];
    fatal: number;
    types: string[];
    homes: Record<string, string>;
  }>;
  draw(text: string, type: string): Promise<{ mermaid: string }>;
  save(file: string, text: string): Promise<{ saved: string }>;
  copy(file: string): Promise<{ copied: string[] }>;
  addExample(
    module: string,
    name: string,
    example: Record<string, unknown>,
  ): Promise<{ saved: boolean; findings?: Finding[]; text: string }>;
}

/** The wire over HTTP; `base` is empty for the page's own origin. */
export function httpWire(base = ""): Wire {
  async function call<T>(
    method: "GET" | "POST",
    path: string,
    body?: unknown,
  ): Promise<T> {
    const resp = await fetch(base + path, {
      method,
      headers: body === undefined ? {} : { "Content-Type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const data: unknown = await resp.json();
    if (!resp.ok) {
      const fault =
        typeof data === "object" && data !== null && "fault" in data
          ? String((data as { fault: unknown }).fault)
          : resp.statusText;
      throw resp.status === 501
        ? new NotRun(501, fault)
        : new WireFault(resp.status, fault);
    }
    return data as T;
  }
  const post = <T>(path: string, body: unknown) => call<T>("POST", path, body);
  const q = encodeURIComponent;
  return {
    request: (body) => post("/v1/request", body),
    get: (id) => post("/v1/get", { id }),
    // every page, its cursor followed to the last (ADR-0192 decision 5)
    history: async (id) => {
      type Page = {
        events: HistoryItem[];
        cursor: string | null;
        not_computed: string[];
      } | null;
      const events: HistoryItem[] = [];
      let cursor: string | null = null;
      let notComputed: string[];
      do {
        const page: Page = await post<Page>(
          "/v1/history",
          cursor === null ? { id } : { id, cursor },
        );
        if (page === null) return null;
        events.push(...page.events);
        notComputed = page.not_computed;
        cursor = page.cursor;
      } while (cursor !== null);
      return { events, not_computed: notComputed };
    },
    events: (positions) => post("/v1/events", { positions }),
    declaration: (type) => post("/v1/declaration", { type }),
    importBatch: (actor, objects, sequences = []) =>
      post("/v1/import_batch", {
        actor: { id: actor },
        batch: sequences.length > 0 ? { objects, sequences } : { objects },
        disposition: [],
        dry_run: false,
      }),
    lookup: (actor, source, value) =>
      post("/v1/lookup", { actor: { id: actor }, source, value }),
    load: (files) => post("/sandbox/load", { files }),
    reset: () => post("/sandbox/reset", {}),
    clock: (change) => post("/sandbox/clock", change),
    evaluators: (answers) => post("/sandbox/evaluators", { answers }),
    backend: () => call("GET", "/studio/backend"),
    modules: () => call("GET", "/studio/modules"),
    module: (file, source = "design") =>
      call("GET", `/studio/module?file=${q(file)}&source=${source}`),
    availability: (actor, id) =>
      post("/v1/availability", { actor: { id: actor }, id }),
    publish: (actor, change, how) =>
      post("/v1/publish", { actor: { id: actor }, change, ...how }),
    metric: (actor, name, keep) =>
      post("/v1/metric", { actor: { id: actor }, name, keep }),
    diagram: (file, type, source = "design") =>
      call(
        "GET",
        `/studio/diagram?file=${q(file)}&type=${q(type)}&source=${source}`,
      ),
    check: (files) => post("/studio/check", { files }),
    draw: (text, type) => post("/studio/draw", { text, type }),
    save: (file, text) => post("/studio/save", { file, text }),
    copy: (file) => post("/studio/copy", { file }),
    addExample: (module, name, example) =>
      post("/studio/examples", { module, name, example }),
  };
}
