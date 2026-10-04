import { useCallback, useEffect, useMemo, useState } from "react";
import {
  type Declaration,
  type LoadResult,
  type ModuleEntry,
  httpWire,
} from "./api/wire";
import { Diagram } from "./components/Diagram";
import { Objects } from "./components/Objects";
import { RuleSet } from "./components/RuleSet";
import { Stepper } from "./components/Stepper";
import type { ExamplesDoc, Player } from "./session/player";

/** Slice 1 (docs/design/flow-studio.md §6): open one of the design's example
 * modules, see each type's lifecycle and rules, and step through one of its
 * examples against the sandbox. */
export function App() {
  const wire = useMemo(() => httpWire(), []);
  const [modules, setModules] = useState<ModuleEntry[]>([]);
  const [file, setFile] = useState<string | null>(null);
  const [load, setLoad] = useState<LoadResult | null>(null);
  const [doc, setDoc] = useState<ExamplesDoc | null>(null);
  const [type, setType] = useState<string | null>(null);
  const [example, setExample] = useState<string | null>(null);
  const [diagram, setDiagram] = useState("");
  const [decl, setDecl] = useState<Declaration | null>(null);
  const [player, setPlayer] = useState<Player | null>(null);
  const [tick, setTick] = useState(0);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    wire
      .modules()
      .then((ms) => {
        setModules(ms);
        setFile((f) => f ?? ms.find((m) => m.examples)?.file ?? null);
      })
      .catch((e: unknown) =>
        setError(`The studio's server does not answer: ${String(e)}`),
      );
  }, [wire]);

  const entry = modules.find((m) => m.file === file);

  useEffect(() => {
    if (!file) return;
    let live = true;
    void (async () => {
      const bundle = await wire.module(file);
      const result = await wire.load(bundle.files);
      if (!live) return;
      setLoad(result);
      const examples = (bundle.examples?.doc ?? null) as ExamplesDoc | null;
      setDoc(examples);
      setExample(examples ? (Object.keys(examples.examples)[0] ?? null) : null);
      setType(modules.find((m) => m.file === file)?.types[0] ?? null);
    })().catch((e: unknown) => setError(String(e)));
    return () => {
      live = false;
    };
  }, [wire, file, modules]);

  useEffect(() => {
    if (!file || !type || !load?.loaded) return;
    let live = true;
    void Promise.all([wire.diagram(file, type), wire.declaration(type)])
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
  }, [wire, file, type, load]);

  // a module chosen anew starts with nothing loaded until the sandbox says so
  const choose = (next: string) => {
    if (next === file) return;
    setLoad(null);
    setDoc(null);
    setDiagram("");
    setDecl(null);
    setFile(next);
  };

  const changed = useCallback((p: Player) => {
    setPlayer(p);
    setTick((t) => t + 1);
  }, []);

  return (
    <div className="min-h-full bg-stone-50 text-slate-900">
      <header className="flex items-baseline gap-3 border-b border-slate-200 bg-white px-4 py-2">
        <h1 className="text-lg font-semibold">Flow Studio</h1>
        <span className="text-sm text-slate-500">
          ObjectFlow's example flows, run against the reference runner
        </span>
      </header>
      {error && (
        <p className="m-4 rounded bg-red-50 px-3 py-2 text-sm text-red-800">
          {error}
        </p>
      )}
      <main className="grid gap-4 p-4 lg:grid-cols-[14rem_minmax(0,1fr)_minmax(0,28rem)]">
        <nav className="space-y-4">
          <section>
            <h2 className="label">Modules</h2>
            <ul>
              {modules.map((m) => (
                <li key={m.file}>
                  <button
                    className={`w-full rounded px-2 py-1 text-left text-sm ${m.file === file ? "bg-slate-200" : "hover:bg-slate-100"}`}
                    onClick={() => choose(m.file)}
                  >
                    {m.file}
                    {!m.examples && (
                      <span className="text-xs text-slate-400">
                        {" "}
                        no examples
                      </span>
                    )}
                  </button>
                </li>
              ))}
            </ul>
          </section>
          {doc && (
            <section>
              <h2 className="label">Examples</h2>
              <ul className="max-h-96 overflow-y-auto">
                {Object.keys(doc.examples).map((name) => (
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
                  onClick={() => setType(t)}
                >
                  {t}
                </button>
              ))}
            </div>
          )}
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
        </section>

        <aside className="min-w-0 space-y-4">
          {doc && example && load?.loaded && (
            <div className="rounded border border-slate-200 bg-white p-3">
              <h2 className="label">Simulation</h2>
              <Stepper
                key={`${file}/${example}`}
                wire={wire}
                doc={doc}
                example={example}
                onChange={changed}
              />
            </div>
          )}
          <div className="rounded border border-slate-200 bg-white p-3">
            <h2 className="label">Objects</h2>
            <Objects wire={wire} player={player} tick={tick} />
          </div>
        </aside>
      </main>
    </div>
  );
}
