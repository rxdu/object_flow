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
  const [outcome, setOutcome] = useState<{
    holds: boolean;
    text: string;
  } | null>(null);
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
            ? { holds: false, text: `Does not do what it claims: ${problem}` }
            : { holds: true, text: "Does what the example claims." },
        );
      } else if (ps.kind === "step" && d.status === "refused") {
        setOutcome({
          holds: false,
          text: "A setup step is refused, so the example cannot be what it claims.",
        });
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
      <p className="text-sm leading-relaxed text-slate-700">
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
      <ol className="space-y-1.5">
        {steps.map((ps, i) => {
          const d = done[i];
          const look = d ? STATUS[d.status] : NEXT;
          return (
            <li
              key={i}
              className={`rounded-md border-l-4 px-3 py-1.5 text-sm ${look.row}`}
            >
              <div className="flex items-start gap-2">
                <span
                  className={`mt-0.5 inline-flex w-22 shrink-0 items-center gap-1 rounded-full px-2 py-0.5 text-[11px] font-semibold tracking-wide uppercase ${look.badge}`}
                >
                  <span aria-hidden="true">{look.icon}</span>
                  {d?.status ?? "next"}
                </span>
                <span className="min-w-0">
                  {ps.label}
                  {ps.kind !== "evaluators" && (
                    <span className="ml-2 font-mono text-xs break-words">
                      {summary(ps.step)}
                    </span>
                  )}
                </span>
              </div>
              {d?.verdict && (
                <div className="mt-0.5 ml-24 text-xs">{explain(d.verdict)}</div>
              )}
              {d?.note && <div className="mt-0.5 ml-24 text-xs">{d.note}</div>}
            </li>
          );
        })}
      </ol>
      {outcome && (
        <p
          className={`flex items-start gap-2 rounded-md px-3 py-2 text-sm font-medium ${outcome.holds ? "bg-emerald-100 text-emerald-900" : "bg-red-100 text-red-900"}`}
        >
          <span aria-hidden="true">{outcome.holds ? "✓" : "✕"}</span>
          {outcome.text}
        </p>
      )}
    </section>
  );
}

/** How a step's row and badge look, by what became of it. */
interface Look {
  row: string;
  badge: string;
  icon: string;
}

const NEXT: Look = {
  row: "border-slate-200 bg-white text-slate-500",
  badge: "bg-slate-100 text-slate-500",
  icon: "·",
};

const STATUS: Record<Done["status"], Look> = {
  applied: {
    row: "border-emerald-500 bg-emerald-50 text-emerald-950",
    badge: "bg-emerald-100 text-emerald-800",
    icon: "✓",
  },
  answered: {
    row: "border-slate-400 bg-slate-50 text-slate-700",
    badge: "bg-slate-200 text-slate-700",
    icon: "✓",
  },
  imported: {
    row: "border-emerald-500 bg-emerald-50 text-emerald-950",
    badge: "bg-emerald-100 text-emerald-800",
    icon: "↓",
  },
  refused: {
    row: "border-amber-500 bg-amber-50 text-amber-950",
    badge: "bg-amber-100 text-amber-800",
    icon: "✕",
  },
  waiting: {
    row: "border-sky-500 bg-sky-50 text-sky-950",
    badge: "bg-sky-100 text-sky-800",
    icon: "…",
  },
  "not run": {
    row: "border-violet-500 bg-violet-50 text-violet-950",
    badge: "bg-violet-100 text-violet-800",
    icon: "−",
  },
  fault: {
    row: "border-red-500 bg-red-50 text-red-950",
    badge: "bg-red-100 text-red-800",
    icon: "!",
  },
};
