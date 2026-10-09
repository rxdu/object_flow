import { useCallback, useEffect, useRef, useState } from "react";
import { NotRun, type Verdict, type Wire, WireFault } from "../api/wire";
import { explain } from "../session/explain";
import {
  type Beside,
  type ExamplesDoc,
  type PlanStep,
  Player,
  Waiting,
  plan,
  summary,
} from "../session/player";

interface Done {
  status:
    | "applied"
    | "refused"
    | "answered"
    | "imported"
    | "waiting"
    | "not run"
    | "fault";
  verdict?: Verdict;
  note?: string;
}

/** An example played a step at a time against the sandbox. */
export function Stepper({
  wire,
  doc,
  example,
  beside,
  now,
  onChange,
}: {
  wire: Wire;
  doc: ExamplesDoc;
  /** where a `publish` step reads its files (ADR-0166) */
  beside?: Beside;
  example: string;
  /** the sandbox's clock, the page's one reading of it, which free play's
   * Advance moves as an example's `after` does */
  now: string;
  onChange: (player: Player) => void;
}) {
  const steps: PlanStep[] = plan(doc, example);
  const player = useRef<Player | null>(null);
  // the next step's index: a ref, since a run takes several steps within one
  // render, where `done` would still be the render's
  const next = useRef(0);
  const [done, setDone] = useState<Done[]>([]);
  const [outcome, setOutcome] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const begin = useCallback(
    (start: string) => {
      player.current = new Player(wire, start, beside);
      next.current = 0;
      setDone([]);
      setOutcome(null);
      onChange(player.current);
    },
    [wire, beside, onChange],
  );

  // a fresh sandbox for the example, once it is shown; the parent keys this
  // component by example, so another example mounts it anew
  useEffect(() => {
    let live = true;
    void wire.reset().then((r) => {
      if (live) begin(r.now);
    });
    return () => {
      live = false;
    };
  }, [wire, begin]);

  async function reset() {
    begin((await wire.reset()).now);
  }

  const finished =
    done.length >= steps.length ||
    done.some((d) => d.status !== "applied" && d.status !== "answered");

  async function stepOnce(): Promise<boolean> {
    const p = player.current;
    const ps = steps[next.current];
    if (!p || !ps) return false;
    next.current += 1;
    let d: Done;
    try {
      const v = await p.perform(ps);
      d =
        v === null
          ? { status: ps.kind === "evaluators" ? "answered" : "imported" }
          : {
              status: v.kind === "satisfied" ? "applied" : "refused",
              verdict: v,
            };
      if (ps.kind === "tested" && v) {
        const problem = await p.compare(ps.expect, v, ps.step);
        setOutcome(
          problem
            ? `Does not do what it claims: ${problem}`
            : "Does what the example claims.",
        );
      } else if (ps.kind === "step" && d.status === "refused") {
        setOutcome(
          "A setup step is refused, so the example cannot be what it claims.",
        );
      }
    } catch (e) {
      if (e instanceof Waiting) d = { status: "waiting", note: e.message };
      else if (e instanceof NotRun) d = { status: "not run", note: e.message };
      else if (e instanceof WireFault)
        d = { status: "fault", note: `${e.status}: ${e.message}` };
      else throw e;
    }
    setDone((prev) => [...prev, d]);
    onChange(p);
    return d.status === "applied" || d.status === "answered";
  }

  async function run(all: boolean) {
    setBusy(true);
    try {
      let go = await stepOnce();
      while (all && go && next.current < steps.length) {
        go = await stepOnce();
      }
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="space-y-3">
      <p className="text-sm text-slate-700">
        {doc.examples[example]?.description}
      </p>
      <div className="flex flex-wrap items-center gap-2">
        <button
          className="btn"
          disabled={busy || finished}
          onClick={() => void run(false)}
        >
          Step
        </button>
        <button
          className="btn"
          disabled={busy || finished}
          onClick={() => void run(true)}
        >
          Run to the end
        </button>
        <button
          className="btn-quiet"
          disabled={busy}
          onClick={() => void reset()}
        >
          Reset
        </button>
        <span className="ml-auto font-mono text-xs text-slate-500 tabular-nums">
          now {now}
        </span>
      </div>
      <ol className="space-y-1">
        {steps.map((ps, i) => {
          const d = done[i];
          return (
            <li
              key={i}
              className={`rounded px-2 py-1 text-sm ${d ? STATUS[d.status] : "text-slate-400"}`}
            >
              <div className="flex gap-2">
                <span className="w-16 shrink-0 text-xs uppercase tracking-wide">
                  {d?.status ?? "next"}
                </span>
                <span>
                  {ps.label}
                  {ps.kind !== "evaluators" && (
                    <span className="ml-2 font-mono text-xs">
                      {summary(ps.step)}
                    </span>
                  )}
                </span>
              </div>
              {d?.verdict && (
                <div className="ml-18 text-xs">{explain(d.verdict)}</div>
              )}
              {d?.note && <div className="ml-18 text-xs">{d.note}</div>}
            </li>
          );
        })}
      </ol>
      {outcome && (
        <p className="rounded bg-slate-100 px-2 py-1 text-sm">{outcome}</p>
      )}
    </section>
  );
}

const STATUS: Record<Done["status"], string> = {
  applied: "bg-emerald-50 text-emerald-900",
  answered: "bg-slate-50 text-slate-700",
  imported: "bg-emerald-50 text-emerald-900",
  refused: "bg-amber-50 text-amber-900",
  waiting: "bg-sky-50 text-sky-900",
  "not run": "bg-violet-50 text-violet-900",
  fault: "bg-red-50 text-red-900",
};
