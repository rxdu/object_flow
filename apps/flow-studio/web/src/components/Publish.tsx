import { useEffect, useState } from "react";
import type {
  ModuleEntry,
  ModuleFile,
  PublishReport,
  Verdict,
  Wire,
} from "../api/wire";
import { explain } from "../session/explain";
import type { Player } from "../session/player";

/** A version the editor asks to publish: its files, the loaded module's
 * imports and the edited text, which no file beside the examples file
 * holds until it is saved. */
export interface EditorVersion {
  files: ModuleFile[];
  label: string;
}

/** Why a session publishing the editor's text cannot be an example. */
export const UNSAVED =
  "A version published from the editor's unsaved text is no file beside the examples file, so no publish step can name it; save it to the workspace and publish the saved module.";

interface Drafted {
  change: string;
  submitted: Verdict;
  report: PublishReport | null;
  label: string;
  files: ModuleFile[];
  /** the files a `publish` step names, none for the editor's text */
  names: string[] | null;
}

const SEVERITY: Record<string, string> = {
  fatal: "bg-red-50 text-red-800",
  decision: "bg-amber-50 text-amber-800",
  notice: "bg-slate-50 text-slate-700",
};

/** What the version declares that its reviewer reads before approving it
 * (publish-and-import.md §2): each invariant with its form and the
 * transitions that could break it, and each metric dimension that names a
 * person. Names only, as the report gives them; nothing restated. */
function Declared({ report }: { report: PublishReport }) {
  const forms = report.invariant_forms ?? {};
  const violators = report.could_violate ?? {};
  const invariants = [
    ...new Set([...Object.keys(forms), ...Object.keys(violators)]),
  ].sort();
  const persons = report.person_dimensions ?? [];
  if (invariants.length === 0 && persons.length === 0) return null;
  return (
    <div className="space-y-1 text-xs">
      {invariants.length > 0 && (
        <ul className="space-y-1">
          {invariants.map((inv) => (
            <li key={inv}>
              <span className="text-slate-500">invariant</span>{" "}
              <span className="font-mono">{inv}</span>
              {forms[inv] && ` · ${forms[inv]}`}
              {violators[inv] && (
                <span className="block text-slate-600">
                  {violators[inv].length > 0
                    ? `could be broken by ${violators[inv].join(", ")}`
                    : "no transition could break it"}
                </span>
              )}
            </li>
          ))}
        </ul>
      )}
      {persons.length > 0 && (
        <p>
          <span className="text-slate-500">
            metric dimensions naming a person
          </span>
          <span className="flex flex-wrap gap-x-3 font-mono text-slate-600">
            {persons.map((d) => (
              <span key={d} className="[overflow-wrap:anywhere]">
                {d}
              </span>
            ))}
          </span>
        </p>
      )}
    </div>
  );
}

/** Publishing a second version into the running sandbox (ADR-0166 decision
 * 4, docs/design/flow-studio.md §4): a module of the catalogue, its imports
 * with it, or the editor's text; its dry run's report read before anything
 * is installed; its approval, which migrates the session's objects, each of
 * which the panel lists, or its withdrawal, which leaves the sandbox as it
 * was (publish-and-import.md §1, §2). */
export function Publish({
  wire,
  player,
  editor,
  onDone,
  onSelect,
  onInstalled,
}: {
  wire: Wire;
  player: Player;
  /** the editor's text, where the editor asked to publish it */
  editor: EditorVersion | null;
  onDone: (v: Verdict | null) => void;
  onSelect: (id: string) => void;
  /** the files of a version installed, which the page then shows */
  onInstalled: (files: ModuleFile[]) => void;
}) {
  const [modules, setModules] = useState<ModuleEntry[]>([]);
  const [choice, setChoice] = useState("");
  const [drafted, setDrafted] = useState<Drafted | null>(null);
  const [migrated, setMigrated] = useState<string[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let live = true;
    void wire.modules().then((ms) => {
      if (live) setModules(ms);
    });
    return () => {
      live = false;
    };
  }, [wire]);

  // the editor's request, when a new one arrives, chooses its text: state
  // adjusted as the prop changes, during the render (React's own pattern)
  const [asked, setAsked] = useState<EditorVersion | null>(null);
  if (editor !== asked) {
    setAsked(editor);
    if (editor) setChoice("editor");
  }

  async function act(action: () => Promise<void>) {
    setBusy(true);
    setError(null);
    try {
      await action();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const dryRun = () =>
    act(async () => {
      let files: ModuleFile[];
      let label: string;
      let names: string[] | null;
      if (choice === "editor" && editor) {
        ({ files, label } = editor);
        names = null;
      } else {
        const [source, file] = choice.split(":", 2) as [
          "design" | "workspace",
          string,
        ];
        const bundle = await wire.module(file, source);
        files = bundle.files.map((f) => ({ name: f.name, text: f.text }));
        label = file;
        names = files.map((f) => f.name);
      }
      const d = await player.draftVersion(files, label);
      setMigrated(null);
      setDrafted({ ...d, label, files, names });
    });

  const approve = (d: Drafted) =>
    act(async () => {
      const v = await player.approveVersion(
        d.change,
        d.label,
        d.names ? { source: d.names } : null,
        UNSAVED,
      );
      onDone(v);
      if (v?.kind === "satisfied") {
        onInstalled(d.files);
        // every object a mapping, an admission or a discharge reached
        const reached = Object.entries(d.report?.affected ?? {})
          .filter(([code]) => /^(mapping:|invariant:|discharged:)/.test(code))
          .flatMap(([, ids]) => ids);
        setMigrated([...new Set(reached)]);
        setDrafted(null);
      }
    });

  const withdraw = (d: Drafted) =>
    act(async () => {
      await player.withdrawVersion(d.change);
      setDrafted(null);
    });

  const r = drafted?.report ?? null;
  const approvable =
    drafted !== null && drafted.submitted.kind === "satisfied" && !!r?.accepted;

  return (
    <div>
      <h3 className="label">Publish a version</h3>
      <p className="mb-1 text-xs text-slate-500">
        Another version installed over this sandbox&apos;s objects, as a publish
        installs one: its dry run&apos;s report first, then its approval, which
        migrates them. The version is the files given, its imports with it.
      </p>
      {error && <p className="text-red-700">{error}</p>}
      <div className="flex gap-2">
        <select
          className="field flex-1"
          value={choice}
          disabled={busy || drafted !== null}
          onChange={(e) => setChoice(e.target.value)}
        >
          <option value="">(a version)</option>
          {editor && <option value="editor">the editor&apos;s text</option>}
          {modules.map((m) => (
            <option
              key={`${m.source}:${m.file}`}
              value={`${m.source}:${m.file}`}
            >
              {m.file} ({m.source})
            </option>
          ))}
        </select>
        <button
          className="btn-quiet"
          disabled={busy || !choice || drafted !== null}
          onClick={() => void dryRun()}
        >
          Dry run
        </button>
      </div>

      {drafted && (
        <div className="mt-2 space-y-2 rounded border border-slate-200 p-2">
          <p className="text-xs">
            <span className="font-mono">{drafted.label}</span> as version{" "}
            {r?.version ?? "?"}:{" "}
            {r?.accepted
              ? "it may be approved"
              : "it may not be approved while a finding below is fatal or a decision unresolved"}
          </p>
          {drafted.submitted.kind !== "satisfied" && (
            <p className="text-xs text-amber-800">
              Its submission is refused: {explain(drafted.submitted)}
            </p>
          )}
          <ul className="space-y-1">
            {(r?.findings ?? []).map((f, i) => (
              <li key={i} className="text-xs">
                <span className={`rounded px-1 ${SEVERITY[f.severity] ?? ""}`}>
                  {f.severity}
                </span>{" "}
                <span className="font-mono">{f.code}</span>
                {f.affected > 0 &&
                  ` · ${f.affected} object${f.affected === 1 ? "" : "s"}`}
                <span className="block text-slate-600">{f.message}</span>
              </li>
            ))}
          </ul>
          {r && <Declared report={r} />}
          <div className="flex gap-2">
            <button
              className="btn"
              disabled={busy || !approvable}
              onClick={() => void approve(drafted)}
            >
              Publish
            </button>
            <button
              className="btn-quiet"
              disabled={busy}
              onClick={() => void withdraw(drafted)}
            >
              Withdraw
            </button>
          </div>
        </div>
      )}

      {migrated && (
        <div className="mt-2 text-xs">
          {migrated.length === 0 ? (
            <p className="text-slate-500">
              Installed; no object of the session was migrated.
            </p>
          ) : (
            <>
              <p className="text-slate-500">
                Installed; migrated, each with its migration in its history:
              </p>
              <ul className="flex flex-wrap gap-1">
                {migrated.map((id) => (
                  <li key={id}>
                    <button
                      className="btn-quiet font-mono"
                      onClick={() => onSelect(id)}
                    >
                      {id}
                    </button>
                  </li>
                ))}
              </ul>
            </>
          )}
        </div>
      )}
    </div>
  );
}
