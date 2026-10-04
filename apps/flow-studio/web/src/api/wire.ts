// The engine's wire (ADR-0150): one POST per library operation, the sandbox's
// controls, and the studio's own tooling. The shapes are the library's
// (docs/design/library-api.md §4 to §6); a backend that does not compute a
// field names it in `not_computed` rather than filling it in.

export type Remedy =
  | "self_serviceable"
  | "delegable"
  | "temporal"
  | "dependent"
  | "unreachable_from_here";

export type WireValue = null | boolean | number | string | WireValue[];

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
    | { kind: "invalid_input"; inputs: string[]; remedy: Remedy }
    | { kind: "not_requestable"; parents: string[]; remedy: Remedy }
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
}

export interface MetricPage extends Reported {
  rows: { dimensions: Record<string, WireValue>; value: WireValue }[];
  cursor: string | null;
  flags: unknown[];
}

export interface AttributeSpec {
  type?: string;
  reference?: string;
}

export interface TransitionSpec {
  kind?: string;
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

export interface Wire {
  request(body: RequestBody): Promise<Verdict>;
  get(id: string): Promise<FlowObject | null>;
  history(id: string): Promise<{ events: FlowEvent[]; not_computed: string[] }>;
  /** the event at each position given, in the order given, and null where
   * the log holds none (ADR-0151) */
  events(
    positions: number[],
  ): Promise<{ events: (FlowEvent | null)[]; not_computed: string[] }>;
  declaration(type: string): Promise<Declaration>;
  availability(actor: string, id: string): Promise<TransitionOffer[]>;
  metric(actor: string, name: string): Promise<MetricPage>;
  load(files: ModuleFile[]): Promise<LoadResult>;
  reset(): Promise<{ now: string }>;
  clock(
    change: { advance: string } | { set: string },
  ): Promise<{ now: string }>;
  evaluators(answers: Record<string, unknown>): Promise<unknown>;
  modules(): Promise<ModuleEntry[]>;
  module(file: string, source?: Source): Promise<ModuleBundle>;
  diagram(
    file: string,
    type: string,
    source?: Source,
  ): Promise<{ mermaid: string }>;
  /** the studio's editing tools (docs/design/flow-studio.md §3.1, §5) */
  check(files: ModuleFile[]): Promise<{ findings: Finding[]; fatal: number }>;
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
    history: (id) => post("/v1/history", { id }),
    events: (positions) => post("/v1/events", { positions }),
    declaration: (type) => post("/v1/declaration", { type }),
    load: (files) => post("/sandbox/load", { files }),
    reset: () => post("/sandbox/reset", {}),
    clock: (change) => post("/sandbox/clock", change),
    evaluators: (answers) => post("/sandbox/evaluators", { answers }),
    modules: () => call("GET", "/studio/modules"),
    module: (file, source = "design") =>
      call("GET", `/studio/module?file=${q(file)}&source=${source}`),
    availability: (actor, id) =>
      post("/v1/availability", { actor: { id: actor }, id }),
    metric: (actor, name) => post("/v1/metric", { actor: { id: actor }, name }),
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
