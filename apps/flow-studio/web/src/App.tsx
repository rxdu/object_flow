import { useCallback, useEffect, useMemo, useState } from "react";
import {
  type Declaration,
  type LoadResult,
  type ModuleBundle,
  type ModuleEntry,
  type Source,
  type Verdict,
  httpWire,
} from "./api/wire";
import { Diagram } from "./components/Diagram";
import { Editor } from "./components/Editor";
import { FreePlay } from "./components/FreePlay";
import { Metrics } from "./components/Metrics";
import { Objects } from "./components/Objects";
import { Offers } from "./components/Offers";
import { RuleSet } from "./components/RuleSet";
import { Stepper } from "./components/Stepper";
import { explain } from "./session/explain";
import { type ExamplesDoc, OPERATOR, Player } from "./session/player";

interface Chosen {
  file: string;
  source: Source;
}

const same = (a: Chosen | null, b: Chosen | null) =>
  a?.file === b?.file && a?.source === b?.source;

/** Slices 1 to 3 (docs/design/flow-studio.md §6): open a module, the
 * design's or the workspace's; see each type's lifecycle, rules and metrics;
 * edit it as text with the checker's findings against its lines; step
 * through one of its examples against the sandbox; play freely, any
 * transition the live offer allows, on any object, as any actor, with the
 * clock and the evaluators' answers in hand; and save what was found as an
 * example the checker holds. */
export function App() {
  const wire = useMemo(() => httpWire(), []);
  const [modules, setModules] = useState<ModuleEntry[]>([]);
  const [chosen, setChosen] = useState<Chosen | null>(null);
  const [bundle, setBundle] = useState<ModuleBundle | null>(null);
  const [load, setLoad] = useState<LoadResult | null>(null);
  const [doc, setDoc] = useState<ExamplesDoc | null>(null);
  const [picked, setPicked] = useState<string | null>(null);
  const [example, setExample] = useState<string | null>(null);
  const [diagram, setDiagram] = useState("");
  const [decl, setDecl] = useState<Declaration | null>(null);
  const [player, setPlayer] = useState<Player | null>(null);
  const [tick, setTick] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [actor, setActor] = useState(OPERATOR);
  const [last, setLast] = useState<Verdict | null>(null);
  const [tab, setTab] = useState<"view" | "edit">("view");

  const changed = useCallback((p: Player) => {
    setPlayer(p);
    setTick((t) => t + 1);
  }, []);

  const refresh = useCallback(
    () =>
      wire
        .modules()
        .then((ms) => {
          setModules(ms);
          setChosen((c) => {
            if (c) return c;
            const first = ms.find((m) => m.examples);
            return first ? { file: first.file, source: first.source } : null;
          });
        })
        .catch((e: unknown) =>
          setError(`The studio's server does not answer: ${String(e)}`),
        ),
    [wire],
  );

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const entry = modules.find(
    (m) => m.file === chosen?.file && m.source === chosen?.source,
  );
  // the type shown: the one picked, if the module declares it, else its first
  const type =
    picked && entry?.types.includes(picked)
      ? picked
      : (entry?.types[0] ?? null);

  useEffect(() => {
    if (!chosen) return;
    let live = true;
    void (async () => {
      const b = await wire.module(chosen.file, chosen.source);
      const result = await wire.load(b.files);
      if (!live) return;
      setBundle(b);
      setLoad(result);
      const examples = (b.examples?.doc ?? null) as ExamplesDoc | null;
      setDoc(examples);
      setExample(
        examples ? (Object.keys(examples.examples ?? {})[0] ?? null) : null,
      );
      if (!examples && result.loaded) {
        // with no example to step, free play starts from an empty sandbox
        const { now } = await wire.reset();
        if (live) changed(new Player(wire, now));
      }
    })().catch((e: unknown) => setError(String(e)));
    return () => {
      live = false;
    };
  }, [wire, chosen, changed]);

  useEffect(() => {
    if (!chosen || !type || !load?.loaded) return;
    let live = true;
    void Promise.all([
      wire.diagram(chosen.file, type, chosen.source),
      wire.declaration(type),
    ])
      .then(([d, t]) => {
        if (live) {
          setDiagram(d.mermaid);
          setDecl(t);
        }
      })
      .catch((e: unknown) => setError(String(e)));
    return () => {
      live = false;
    };
  }, [wire, chosen, type, load]);

  // a module chosen anew starts with nothing loaded until the sandbox says so
  const choose = (next: Chosen) => {
    if (same(next, chosen)) return;
    setBundle(null);
    setLoad(null);
    setDoc(null);
    setDiagram("");
    setDecl(null);
    setSelected(null);
    setActor(OPERATOR);
    setLast(null);
    setPlayer(null);
    setChosen(next);
  };

  const done = (v: Verdict | null) => {
    setLast(v);
    setTick((t) => t + 1);
  };

  // an edited module loaded into the sandbox starts free play afresh
  const loadedEdit = (r: LoadResult) => {
    setLoad(r);
    setSelected(null);
    setLast(null);
    if (r.loaded) {
      void wire.reset().then(({ now }) => changed(new Player(wire, now)));
    }
  };

  const module = bundle?.files[bundle.files.length - 1];
  const editable =
    chosen?.source === "workspace" && entry?.editable ? chosen.file : null;

  const list = (source: Source, title: string) => {
    const ms = modules.filter((m) => m.source === source);
    return (
      <section>
        <h2 className="label">{title}</h2>
        {ms.length === 0 && (
          <p className="text-xs text-slate-400">
            Nothing yet: copy a module here to edit it.
          </p>
        )}
        <ul>
          {ms.map((m) => (
            <li key={`${m.source}:${m.file}`}>
              <button
                className={`w-full rounded px-2 py-1 text-left text-sm ${same(chosen, m) ? "bg-slate-200" : "hover:bg-slate-100"}`}
                onClick={() => choose({ file: m.file, source: m.source })}
              >
                {m.file}
                {!m.examples && (
                  <span className="text-xs text-slate-400"> no examples</span>
                )}
              </button>
            </li>
          ))}
        </ul>
      </section>
    );
  };

  return (
    <div className="min-h-full bg-stone-50 text-slate-900">
      <header className="flex items-baseline gap-3 border-b border-slate-200 bg-white px-4 py-2">
        <h1 className="text-lg font-semibold">Flow Studio</h1>
        <span className="text-sm text-slate-500">
          ObjectFlow's flows, edited, run and measured against the reference
          runner
        </span>
      </header>
      {error && (
        <p className="m-4 rounded bg-red-50 px-3 py-2 text-sm text-red-800">
          {error}
        </p>
      )}
      <main className="grid gap-4 p-4 lg:grid-cols-[14rem_minmax(0,1fr)_minmax(0,28rem)]">
        <nav className="space-y-4">
          {list("workspace", "Your workspace")}
          {list("design", "The design's modules")}
          {doc && (
            <section>
              <h2 className="label">Examples</h2>
              <ul className="max-h-96 overflow-y-auto">
                {Object.keys(doc.examples ?? {}).map((name) => (
                  <li key={name}>
                    <button
                      className={`w-full rounded px-2 py-0.5 text-left font-mono text-xs break-all ${name === example ? "bg-slate-200" : "hover:bg-slate-100"}`}
                      onClick={() => setExample(name)}
                    >
                      {name}
                    </button>
                  </li>
                ))}
              </ul>
            </section>
          )}
        </nav>

        <section className="min-w-0 space-y-4">
          <div className="flex gap-1 border-b border-slate-200">
            {(["view", "edit"] as const).map((t) => (
              <button
                key={t}
                className={`-mb-px border-b-2 px-3 py-1 text-sm ${tab === t ? "border-slate-800 font-medium" : "border-transparent text-slate-500"}`}
                onClick={() => setTab(t)}
              >
                {t === "view" ? "View" : "Edit"}
              </button>
            ))}
          </div>
          {load && !load.loaded && (
            <div className="rounded bg-red-50 p-3 text-sm text-red-900">
              <p className="font-medium">The module does not load:</p>
              {load.findings.map((f, i) => (
                <p key={i} className="font-mono text-xs">
                  {f.file}:{f.line} {f.code} {f.message}
                </p>
              ))}
            </div>
          )}
          {entry && (
            <div className="flex flex-wrap gap-1">
              {entry.types.map((t) => (
                <button
                  key={t}
                  className={`rounded-full px-3 py-0.5 text-sm ${t === type ? "bg-slate-800 text-white" : "bg-white ring-1 ring-slate-300"}`}
                  onClick={() => setPicked(t)}
                >
                  {t}
                </button>
              ))}
            </div>
          )}
          {chosen && bundle && module && (
            <div hidden={tab !== "edit"}>
              <Editor
                key={`${chosen.source}:${chosen.file}:${module.text.length}`}
                wire={wire}
                file={chosen.file}
                source={chosen.source}
                editable={editable !== null}
                text={module.text}
                deps={bundle.files.slice(0, -1)}
                type={type}
                onLoaded={loadedEdit}
                onSaved={() => {
                  void refresh();
                  void wire.module(chosen.file, chosen.source).then(setBundle);
                }}
                onCopied={() =>
                  void refresh().then(() =>
                    choose({ file: chosen.file, source: "workspace" }),
                  )
                }
              />
            </div>
          )}
          {tab === "view" && (
            <>
              {diagram && (
                <div className="rounded border border-slate-200 bg-white p-3">
                  <Diagram source={diagram} />
                </div>
              )}
              {decl && (
                <div className="rounded border border-slate-200 bg-white p-3">
                  <h2 className="label">Rules of {type}</h2>
                  <RuleSet decl={decl} />
                </div>
              )}
              {decl && type && load?.loaded && (
                <div className="rounded border border-slate-200 bg-white p-3">
                  <h2 className="label">
                    Metrics of {type}, over every object
                  </h2>
                  <Metrics
                    wire={wire}
                    type={type}
                    decl={decl}
                    actor={actor}
                    tick={tick}
                  />
                </div>
              )}
            </>
          )}
        </section>

        <aside className="min-w-0 space-y-4">
          {doc && example && load?.loaded && (
            <div className="rounded border border-slate-200 bg-white p-3">
              <h2 className="label">Simulation</h2>
              <Stepper
                key={`${chosen?.source}:${chosen?.file}/${example}`}
                wire={wire}
                doc={doc}
                example={example}
                onChange={changed}
              />
            </div>
          )}
          {player && load?.loaded && (
            <div className="rounded border border-slate-200 bg-white p-3">
              <h2 className="label">Free play</h2>
              <p className="mb-3 text-xs text-slate-500">
                From wherever the sandbox stands: after an example's steps, or
                from empty. Choose an object below to see what it offers.
              </p>
              <FreePlay
                wire={wire}
                player={player}
                types={load.types}
                evaluators={load.evaluators}
                actor={actor}
                setActor={setActor}
                tick={tick}
                onDone={done}
                editable={editable}
              />
              {last && (
                <p
                  className={`mt-3 rounded px-2 py-1 text-sm ${last.kind === "satisfied" ? "bg-emerald-50" : "bg-amber-50"}`}
                >
                  {explain(last)}
                  {"clause" in last && last.clause ? ` · ${last.clause}` : ""}
                  {"remedy" in last && last.remedy ? ` · ${last.remedy}` : ""}
                </p>
              )}
            </div>
          )}
          {player && selected && (
            <div className="rounded border border-slate-200 bg-white p-3">
              <h2 className="label">What {selected} offers</h2>
              <Offers
                wire={wire}
                player={player}
                objectId={selected}
                actor={actor}
                tick={tick}
                onDone={done}
              />
            </div>
          )}
          <div className="rounded border border-slate-200 bg-white p-3">
            <h2 className="label">Objects</h2>
            <Objects
              wire={wire}
              player={player}
              tick={tick}
              selected={selected}
              onSelect={setSelected}
            />
          </div>
        </aside>
      </main>
    </div>
  );
}
