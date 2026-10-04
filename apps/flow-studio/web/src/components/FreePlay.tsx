import { useEffect, useState } from "react";
import type { Declaration, TransitionSpec, Verdict, Wire } from "../api/wire";
import { explain } from "../session/explain";
import { OPERATOR, type Player } from "../session/player";
import { InputForm, type Known } from "./InputForm";
import { SaveExample } from "./SaveExample";

const ANSWERS = ["satisfied", "unsatisfied", "unanswered"] as const;

/** The inputs an initial transition takes, typed as the runner types an
 * offer's (ADR-0152). */
function creationSchema(d: Declaration, x: TransitionSpec) {
  const typed = (spec?: { type?: string; reference?: string }) =>
    spec?.reference ?? spec?.type ?? "";
  const out: Record<string, string> = {};
  for (const k of [...(x.required_inputs ?? []), ...(x.optional_inputs ?? [])])
    out[k] = typed(d.attributes?.[k]);
  for (const [k, spec] of Object.entries(x.inputs ?? {})) out[k] = typed(spec);
  return out;
}

/** The triggers of free play (docs/design/flow-studio.md §4): who acts, the
 * clock, the evaluators' answers, and a new object of any type the sandbox
 * holds; then the session's own requests, in order. */
export function FreePlay({
  wire,
  player,
  types,
  evaluators,
  actor,
  setActor,
  tick,
  onDone,
  editable,
}: {
  wire: Wire;
  player: Player;
  types: string[];
  evaluators: string[];
  actor: string;
  setActor: (a: string) => void;
  tick: number;
  onDone: (v: Verdict | null) => void;
  /** the workspace module free play may save an example of, if any */
  editable: string | null;
}) {
  const [known, setKnown] = useState<Known[]>([]);
  const [actors, setActors] = useState<Known[]>([]);
  const [by, setBy] = useState("1 h");
  const [type, setType] = useState("");
  const [decl, setDecl] = useState<Declaration | null>(null);
  const [initial, setInitial] = useState("");
  const [answers, setAnswers] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    void player.known().then(async (k) => {
      // an actor is an object of a type one of whose attributes marks an
      // actor identity (DESIGN.md §6 step 1)
      const marks = await Promise.all(
        k.map(async (o) =>
          Object.values((await player.decl(o.type)).attributes ?? {}).some(
            (spec) => "actor_kind" in (spec as object),
          ),
        ),
      );
      if (!live) return;
      setKnown(k);
      setActors(k.filter((_, i) => marks[i]));
    });
    return () => {
      live = false;
    };
  }, [player, tick]);

  useEffect(() => {
    let live = true;
    if (type) {
      void player.decl(type).then((d) => {
        if (!live) return;
        setDecl(d);
        const first = Object.entries(d.transitions ?? {}).find(
          ([, x]) => x.kind === "initial",
        );
        setInitial(first?.[0] ?? "");
      });
    }
    return () => {
      live = false;
    };
  }, [player, type]);

  async function guarded(action: () => Promise<Verdict | null>) {
    try {
      setError(null);
      onDone(await action());
    } catch (e) {
      setError(String(e));
    }
  }

  const initials = Object.entries(decl?.transitions ?? {}).filter(
    ([, x]) => x.kind === "initial",
  );
  const x = decl?.transitions?.[initial];

  return (
    <div className="space-y-4 text-sm">
      {error && <p className="text-red-700">{error}</p>}
      <div className="grid grid-cols-[5rem_1fr] items-center gap-2">
        <span className="text-xs text-slate-500">acting as</span>
        <select
          className="field"
          value={actor}
          onChange={(e) => setActor(e.target.value)}
        >
          <option value={OPERATOR}>operator</option>
          {actors.map((a) => (
            <option key={a.id} value={a.id}>
              {a.name} ({a.type})
            </option>
          ))}
        </select>
        <span className="text-xs text-slate-500">clock</span>
        <form
          className="flex gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            void guarded(async () => {
              await player.advance(by);
              return null;
            });
          }}
        >
          <input
            className="field w-24"
            value={by}
            onChange={(e) => setBy(e.target.value)}
          />
          <button className="btn-quiet">Advance</button>
        </form>
      </div>

      {evaluators.length > 0 && (
        <div>
          <h3 className="label">Evaluators answer</h3>
          {evaluators.map((fn) => (
            <div key={fn} className="flex items-center gap-2">
              <span className="font-mono text-xs">{fn}</span>
              <select
                className="field ml-auto w-36"
                value={answers[fn] ?? ""}
                onChange={(e) => {
                  const a = e.target.value;
                  setAnswers((prev) => ({ ...prev, [fn]: a }));
                  void guarded(async () => {
                    await player.answer({ [fn]: a });
                    return null;
                  });
                }}
              >
                <option value="" disabled>
                  (not answered)
                </option>
                {ANSWERS.map((a) => (
                  <option key={a}>{a}</option>
                ))}
              </select>
            </div>
          ))}
        </div>
      )}

      <div>
        <h3 className="label">Create</h3>
        <div className="flex gap-2">
          <select
            className="field"
            value={type}
            onChange={(e) => setType(e.target.value)}
          >
            <option value="">(a type)</option>
            {types.map((t) => (
              <option key={t}>{t}</option>
            ))}
          </select>
          {initials.length > 1 && (
            <select
              className="field"
              value={initial}
              onChange={(e) => setInitial(e.target.value)}
            >
              {initials.map(([n]) => (
                <option key={n}>{n}</option>
              ))}
            </select>
          )}
        </div>
        {decl && x && (
          <InputForm
            key={`${type}/${initial}`}
            schema={creationSchema(decl, x)}
            enumerations={decl.enumerations ?? {}}
            known={known}
            onSubmit={(inputs) =>
              void guarded(() =>
                player.request(
                  {
                    transition: initial,
                    actor: { id: actor },
                    type,
                    inputs,
                  },
                  `${type}.${initial}`,
                ),
              )
            }
            onCancel={() => setType("")}
          />
        )}
      </div>

      {player.log.length > 0 && (
        <div>
          <h3 className="label">This session</h3>
          <ol className="space-y-1">
            {player.log.map((entry, i) => (
              <li key={i} className="text-xs">
                <span className="font-mono">{entry.summary}</span>{" "}
                <span
                  className={
                    entry.verdict.kind === "satisfied"
                      ? "text-emerald-800"
                      : "text-amber-800"
                  }
                >
                  {explain(entry.verdict)}
                </span>
              </li>
            ))}
          </ol>
        </div>
      )}

      {player.log.length > 0 &&
        (editable ? (
          <SaveExample wire={wire} player={player} module={editable} />
        ) : (
          <p className="text-xs text-slate-500">
            To save this session as an example, copy the module to the
            workspace, where the studio may write.
          </p>
        ))}
    </div>
  );
}
