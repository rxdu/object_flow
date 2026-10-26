import { useCallback, useEffect, useMemo, useState } from "react";
import {
  type Backend,
  type Declaration,
  type LoadResult,
  type ModuleBundle,
  type ModuleEntry,
  type Source,
  type Verdict,
  httpWire,
  type ModuleFile,
} from "./api/wire";
import { Diagram } from "./components/Diagram";
import { Editor } from "./components/Editor";
import type { EditorVersion } from "./components/Publish";
import { FreePlay } from "./components/FreePlay";
import { Metrics } from "./components/Metrics";
import { Objects } from "./components/Objects";
import { Offers } from "./components/Offers";
import { RuleSet } from "./components/RuleSet";
import { Stepper } from "./components/Stepper";
import { explain } from "./session/explain";
import { instantText } from "./session/notation";
import {
  besideModule,
  type ExamplesDoc,
  OPERATOR,
  Player,
} from "./session/player";

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
/** A version a publish installed: its files, the types they declare, and
 * the file declaring each. */
interface Installed {
  files: ModuleFile[];
  types: string[];
  homes: Record<string, string>;
}

export function App() {
  const wire = useMemo(() => httpWire(), []);
  const [modules, setModules] = useState<ModuleEntry[]>([]);
  const [chosen, setChosen] = useState<Chosen | null>(null);
  const [bundle, setBundle] = useState<ModuleBundle | null>(null);
  const [load, setLoad] = useState<LoadResult | null>(null);
  const [doc, setDoc] = useState<ExamplesDoc | null>(null);
  const [picked, setPicked] = useState<string | null>(null);
  // the type a module opens on: the one with the most transitions, its
  // flow's main type rather than a part or a catalogue it lists first
  const [main, setMain] = useState<string | null>(null);
  const [example, setExample] = useState<string | null>(null);
  const [diagram, setDiagram] = useState("");
  const [decl, setDecl] = useState<Declaration | null>(null);
  const [player, setPlayer] = useState<Player | null>(null);
  const [tick, setTick] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  // the editor's text it asked to publish into the running sandbox
  const [publishing, setPublishing] = useState<EditorVersion | null>(null);
  const [actor, setActor] = useState(OPERATOR);
  const [last, setLast] = useState<Verdict | null>(null);
  const [tab, setTab] = useState<"view" | "edit">("view");
  const [backend, setBackend] = useState<Backend | null>(null);

  // the version a publish in free play installed, which the page shows in
  // place of the module file's until the sandbox is reset or reloaded
  const [installed, setInstalled] = useState<Installed | null>(null);

  // a new player is a sandbox reset or reloaded, at the version loaded
  const changed = useCallback((p: Player) => {
    setPlayer(p);
    setTick((t) => t + 1);
    setInstalled(null);
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

  useEffect(() => {
    wire
      .backend()
      .then(setBackend)
      .catch(() => setBackend(null));
  }, [wire]);

  const entry = modules.find(
    (m) => m.file === chosen?.file && m.source === chosen?.source,
  );
  // the types shown: the installed version's, else the module's
  const types = installed?.types ?? entry?.types ?? [];
  // the type shown: the one picked, if the version declares it, else its
  // main one, else its first
  const type =
    picked && types.includes(picked)
      ? picked
      : main && types.includes(main)
        ? main
        : (types[0] ?? null);

  useEffect(() => {
    if (!chosen) return;
    let live = true;
    void (async () => {
      const b = await wire.module(chosen.file, chosen.source);
      // a module chosen since sends its own load; this one's, sent after,
      // would replace the sandbox the page then shows
      if (!live) return;
      const result = await wire.load(b.files);
      if (!live) return;
      setBundle(b);
      setLoad(result);
      if (result.loaded) {
        const counted = await Promise.all(
          result.types.map((t) =>
            wire
              .declaration(t)
              .then(
                (d) => [t, Object.keys(d.transitions ?? {}).length] as const,
              ),
          ),
        );
        if (!live) return;
        const most = counted.reduce<readonly [string, number] | null>(
          (best, c) => (best === null || c[1] > best[1] ? c : best),
          null,
        );
        setMain(most?.[0] ?? null);
      }
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
    // drawn from the file declaring the type in the version the sandbox
    // runs: the installed one's, else the module's
    const home = installed?.files.find((f) => f.name === installed.homes[type]);
    void Promise.all([
      home
        ? wire.draw(home.text, type)
        : wire.diagram(chosen.file, type, chosen.source),
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
  }, [wire, chosen, type, load, installed]);

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
    setMain(null);
    setPicked(null);
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
  // where an example's `publish` step reads its files: beside the module's
  // examples file, in the same catalogue (ADR-0166); one reader per module,
  // so the stepper does not start over on every render
  const examplesName = bundle?.examples?.name;
  const beside = useMemo(
    () =>
      examplesName
        ? besideModule(wire, examplesName, chosen?.source, bundle?.files)
        : undefined,
    [wire, examplesName, chosen?.source, bundle?.files],
  );
  const editable =
    chosen?.source === "workspace" && entry?.editable ? chosen.file : null;

  const list = (source: Source, title: string) => {
    const ms = modules.filter((m) => m.source === source);
    return (
      <section>
        <h2 className="label">{title}</h2>
        {ms.length === 0 && (
          <p className="px-2 text-xs leading-relaxed text-slate-500">
            Nothing yet. Open one of the design's modules, then <em>Edit</em>{" "}
            and <em>Copy to the workspace to edit</em>.
          </p>
        )}
        <ul className="space-y-0.5">
          {ms.map((m) => (
            <li key={`${m.source}:${m.file}`}>
              <button
                className={`w-full rounded-md border-l-2 px-2 py-1 text-left text-sm transition-colors ${same(chosen, m) ? "border-indigo-600 bg-indigo-50 font-medium text-indigo-900" : "border-transparent text-slate-700 hover:bg-slate-100"}`}
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
    <div className="flex h-full flex-col bg-slate-50 text-slate-900">
      <header className="flex shrink-0 items-center gap-3 border-b border-slate-200 bg-white px-5 py-3 shadow-sm">
        <Mark />
        <div className="flex items-baseline gap-3">
          <h1 className="text-lg font-semibold tracking-tight">Flow Studio</h1>
          <span className="hidden text-sm text-slate-500 sm:inline">
            ObjectFlow's flows, edited, run and measured
          </span>
        </div>
        <div className="ml-auto flex items-center gap-3">
          {backend && (
            <span
              className="inline-flex items-center gap-1.5 rounded-full bg-slate-100 px-2.5 py-1 text-xs text-slate-700"
              title={backend.service ?? "the Python reference runner"}
              data-testid="backend"
            >
              <span className="h-2 w-2 rounded-full bg-emerald-500" />
              {backend.backend === "core"
                ? "backend: the Rust core"
                : "backend: the reference runner"}
            </span>
          )}
          <a
            className="text-sm text-slate-500 hover:text-indigo-700"
            href="https://github.com/rxdu/object_flow/tree/main/apps/flow-studio#readme"
            target="_blank"
            rel="noreferrer"
          >
            Guide
          </a>
        </div>
      </header>
      {error && (
        <p className="mx-5 mt-4 rounded-md bg-red-50 px-3 py-2 text-sm text-red-800">
          {error}
        </p>
      )}
      <main className="grid min-h-0 flex-1 gap-5 p-5 lg:grid-cols-[16rem_minmax(0,1fr)_minmax(0,28rem)]">
        <nav className="space-y-5 lg:min-h-0 lg:overflow-y-auto lg:pr-1">
          {list("workspace", "Your workspace")}
          {list("design", "The design's modules")}
          {doc && (
            <section>
              <h2 className="label">
                Examples{" "}
                <span className="font-normal text-slate-400">
                  {Object.keys(doc.examples ?? {}).length}
                </span>
              </h2>
              <ul className="space-y-0.5">
                {Object.keys(doc.examples ?? {}).map((name) => (
                  <li key={name}>
                    <button
                      className={`w-full rounded-md border-l-2 px-2 py-1 text-left text-sm leading-snug transition-colors ${name === example ? "border-indigo-600 bg-indigo-50 text-indigo-900" : "border-transparent text-slate-600 hover:bg-slate-100"}`}
                      title={name}
                      onClick={() => setExample(name)}
                    >
                      {readable(name)}
                    </button>
                  </li>
                ))}
              </ul>
            </section>
          )}
        </nav>

        <section className="min-w-0 space-y-4 lg:min-h-0 lg:overflow-y-auto lg:pr-1">
          <div className="flex gap-1 border-b border-slate-200">
            {(["view", "edit"] as const).map((t) => (
              <button
                key={t}
                className={`-mb-px border-b-2 px-3 py-1.5 text-sm transition-colors ${tab === t ? "border-indigo-600 font-medium text-indigo-700" : "border-transparent text-slate-500 hover:text-slate-800"}`}
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
          {installed && (
            <p className="text-xs text-slate-500">
              Showing the version published in free play, which the sandbox runs
              until it is reset.
            </p>
          )}
          {entry && (
            <div className="flex flex-wrap gap-1">
              {types.map((t) => (
                <button
                  key={t}
                  className={`rounded-full px-3 py-1 text-sm transition-colors ${t === type ? "bg-indigo-600 text-white shadow-sm" : "bg-white text-slate-700 ring-1 ring-slate-300 hover:bg-slate-50"}`}
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
                onPublish={
                  player && load?.loaded
                    ? (files) => {
                        // the publish panel in free play takes it up
                        setPublishing({
                          files,
                          label: `${chosen.file} (edited)`,
                        });
                        setTab("view");
                      }
                    : null
                }
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
                <div className="card">
                  <h2 className="label">Lifecycle of {type}</h2>
                  <Diagram source={diagram} />
                </div>
              )}
              {decl && (
                <div className="card">
                  <h2 className="label">Rules of {type}</h2>
                  <RuleSet decl={decl} />
                </div>
              )}
              {decl && type && load?.loaded && (
                <div className="card">
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

        <aside className="min-w-0 space-y-4 lg:min-h-0 lg:overflow-y-auto lg:pr-1">
          {doc && example && load?.loaded && (
            <div className="card">
              <h2 className="label">Simulation</h2>
              <Stepper
                key={`${chosen?.source}:${chosen?.file}/${example}`}
                wire={wire}
                doc={doc}
                example={example}
                beside={beside}
                now={player ? instantText(player.now) : ""}
                onChange={changed}
              />
            </div>
          )}
          {player && load?.loaded && (
            <div className="card">
              <h2 className="label">Free play</h2>
              <p className="mb-3 text-xs text-slate-500">
                From wherever the sandbox stands: after an example's steps, or
                from empty. Choose an object below to see what it offers.
              </p>
              <FreePlay
                wire={wire}
                player={player}
                types={installed?.types ?? load.types}
                evaluators={load.evaluators}
                actor={actor}
                setActor={setActor}
                tick={tick}
                onDone={done}
                editable={editable}
                editor={publishing}
                onSelect={setSelected}
                onInstalled={(files) =>
                  void wire
                    .check(files)
                    .then((c) => setInstalled({ files, ...c }))
                    .catch((e: unknown) => setError(String(e)))
                }
              />
              {last && (
                <p
                  className={`mt-3 rounded-md px-3 py-2 text-sm ${last.kind === "satisfied" ? "bg-emerald-50 text-emerald-900" : "bg-amber-50 text-amber-900"}`}
                >
                  {explain(last)}
                  {"remedy" in last && last.remedy ? ` · ${last.remedy}` : ""}
                </p>
              )}
            </div>
          )}
          {player && selected && (
            <div className="card">
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
          <div className="card">
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

/** An example's name as a sentence, `a_job_opens` as "A job opens"; the
 * name itself is the button's title. */
function readable(name: string): string {
  const words = name.replaceAll("_", " ");
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/** The studio's mark: a flow's states joined by its transitions. */
function Mark() {
  return (
    <svg
      viewBox="0 0 32 32"
      className="h-7 w-7 shrink-0"
      aria-hidden="true"
      focusable="false"
    >
      <rect width="32" height="32" rx="8" className="fill-indigo-600" />
      <path
        d="M9 16h6m4 0h4"
        className="stroke-indigo-200"
        strokeWidth="2"
        strokeLinecap="round"
      />
      <circle cx="8" cy="16" r="3" className="fill-white" />
      <rect x="14" y="12" width="6" height="8" rx="2" className="fill-white" />
      <circle cx="24.5" cy="16" r="3" className="fill-emerald-300" />
    </svg>
  );
}
