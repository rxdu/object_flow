import { Fragment, useEffect, useState } from "react";
import type {
  Declaration,
  ModuleFile,
  TransitionSpec,
  Verdict,
  Wire,
} from "../api/wire";
import { explain } from "../session/explain";
import { writtenValue } from "../session/notation";
import { OPERATOR, type Player } from "../session/player";
import { InputForm, type Known } from "./InputForm";
import { SaveExample } from "./SaveExample";
import { type EditorVersion, Publish } from "./Publish";

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
  editor,
  onSelect,
  onInstalled,
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
  /** the editor's text, where the editor asked to publish it */
  editor: EditorVersion | null;
  onSelect: (id: string) => void;
  onInstalled: (files: ModuleFile[]) => void;
}) {
  const [known, setKnown] = useState<Known[]>([]);
  const [actors, setActors] = useState<Known[]>([]);
  const [by, setBy] = useState("1 h");
  const [type, setType] = useState("");
  const [decl, setDecl] = useState<Declaration | null>(null);
  const [initial, setInitial] = useState("");
  const [answers, setAnswers] = useState<Record<string, string>>({});
  const [proposing, setProposing] = useState(false);
  // an import: a mirror type's object as a legacy system held it (§4)
  const [mirrors, setMirrors] = useState<string[]>([]);
  const [importing, setImporting] = useState("");
  const [importDecl, setImportDecl] = useState<Declaration | null>(null);
  const [legacyKey, setLegacyKey] = useState("");
  const [arrives, setArrives] = useState("");
  const [values, setValues] = useState<Record<string, string>>({});
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

  useEffect(() => {
    let live = true;
    void Promise.all(
      types.map(async (t) => ((await player.decl(t)).mirror ? t : null)),
    ).then((found) => {
      if (live) setMirrors(found.filter((t): t is string => t !== null));
    });
    return () => {
      live = false;
    };
  }, [player, types]);

  useEffect(() => {
    let live = true;
    if (importing) {
      void player.decl(importing).then((d) => {
        if (!live) return;
        setImportDecl(d);
        setArrives(Object.keys(d.states ?? {})[0] ?? "");
        setValues({});
      });
    }
    return () => {
      live = false;
    };
  }, [player, importing]);

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
  // a creation marked proposable may be filed for approval instead
  const propose = proposing && x?.proposable === true;

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
            {/* a mirror takes no transition but an erasure (flow-format.md
                §4.15): its objects arrive under Import, below */}
            {types
              .filter((t) => !mirrors.includes(t))
              .map((t) => (
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
        {x?.proposable && (
          <label className="mt-1 flex items-center gap-1 text-xs">
            <input
              type="checkbox"
              checked={proposing}
              onChange={(e) => setProposing(e.target.checked)}
            />
            file as a proposal
          </label>
        )}
        {decl && x && (
          <InputForm
            key={`${type}/${initial}`}
            schema={creationSchema(decl, x)}
            enumerations={decl.enumerations ?? {}}
            known={known}
            action={propose ? "Propose" : "Request"}
            onSubmit={(inputs) =>
              void guarded(() =>
                player.request(
                  {
                    transition: initial,
                    actor: { id: actor },
                    type,
                    inputs,
                    ...(propose ? { propose: true } : {}),
                  },
                  `${propose ? "propose " : ""}${type}.${initial}`,
                ),
              )
            }
            onCancel={() => setType("")}
          />
        )}
      </div>

      {mirrors.length > 0 && (
        <div>
          <h3 className="label">Import</h3>
          <p className="mb-1 text-xs text-slate-500">
            An object of a mirror type as a legacy system held it: by its legacy
            key, which later steps name it by, in a state, with its values in
            the model&apos;s notation.
          </p>
          <div className="grid grid-cols-[6rem_1fr] items-center gap-2">
            <span className="text-xs text-slate-500">type</span>
            <select
              className="field"
              value={importing}
              onChange={(e) => setImporting(e.target.value)}
            >
              <option value="">(a mirror type)</option>
              {mirrors.map((t) => (
                <option key={t}>{t}</option>
              ))}
            </select>
            {importing && importDecl && (
              <>
                <span className="text-xs text-slate-500">legacy key</span>
                <input
                  className="field"
                  value={legacyKey}
                  onChange={(e) => setLegacyKey(e.target.value)}
                />
                <span className="text-xs text-slate-500">state</span>
                <select
                  className="field"
                  value={arrives}
                  onChange={(e) => setArrives(e.target.value)}
                >
                  {Object.keys(importDecl.states ?? {}).map((st) => (
                    <option key={st}>{st}</option>
                  ))}
                </select>
                {Object.entries(importDecl.attributes ?? {}).map(
                  ([a, spec]) => (
                    <Fragment key={a}>
                      <span className="font-mono text-xs">{a}</span>
                      <input
                        className="field"
                        placeholder={
                          spec.reference
                            ? `${spec.reference}, by legacy key`
                            : (spec.type ?? "")
                        }
                        value={values[a] ?? ""}
                        onChange={(e) =>
                          setValues((prev) => ({
                            ...prev,
                            [a]: e.target.value,
                          }))
                        }
                      />
                    </Fragment>
                  ),
                )}
              </>
            )}
          </div>
          {importing && importDecl && (
            <button
              className="btn mt-2"
              disabled={!legacyKey || !arrives}
              onClick={() =>
                void guarded(async () => {
                  const attrs = importDecl.attributes ?? {};
                  const enums = importDecl.enumerations ?? {};
                  await player.importStep({
                    type: importing,
                    as: legacyKey,
                    state: arrives,
                    values: Object.fromEntries(
                      Object.entries(values)
                        .filter(([, v]) => v !== "")
                        .map(([a, v]) => [a, writtenValue(v, attrs[a], enums)]),
                    ),
                  });
                  setLegacyKey("");
                  setValues({});
                  return null;
                })
              }
            >
              Import
            </button>
          )}
        </div>
      )}

      <Publish
        wire={wire}
        player={player}
        editor={editor}
        onDone={onDone}
        onSelect={onSelect}
        onInstalled={onInstalled}
      />

      {player.log.length > 0 && (
        <div>
          <h3 className="label">This session</h3>
          <ol className="space-y-1">
            {player.log.map((entry, i) => (
              <li key={i} className="text-xs">
                <span className="font-mono">{entry.summary}</span>{" "}
                <span
                  className={
                    entry.verdict === null || entry.verdict.kind === "satisfied"
                      ? "text-emerald-800"
                      : "text-amber-800"
                  }
                >
                  {entry.verdict === null ? "imported" : explain(entry.verdict)}
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
