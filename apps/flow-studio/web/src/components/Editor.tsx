import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { yaml } from "@codemirror/lang-yaml";
import {
  defaultHighlightStyle,
  syntaxHighlighting,
} from "@codemirror/language";
import { type Diagnostic, lintGutter, setDiagnostics } from "@codemirror/lint";
import { EditorState } from "@codemirror/state";
import {
  EditorView,
  highlightActiveLine,
  keymap,
  lineNumbers,
} from "@codemirror/view";
import { useEffect, useRef, useState } from "react";
import type {
  Finding,
  LoadResult,
  ModuleFile,
  Source,
  Wire,
} from "../api/wire";
import { Diagram } from "./Diagram";

const PAUSE = 600; // ms after the last keystroke before the text is checked

/** A module's description, edited as text (docs/design/flow-studio.md §3.1):
 * the flow checker's findings marked against their lines as it changes, the
 * diagram following the text, and saving and loading into the sandbox as two
 * separate acts, so a half-written module never replaces a running one. */
export function Editor({
  wire,
  file,
  source,
  editable,
  text: initial,
  deps,
  type,
  onLoaded,
  onPublish,
  onSaved,
  onCopied,
}: {
  wire: Wire;
  file: string;
  source: Source;
  editable: boolean;
  text: string;
  /** the files it imports, in import order, which it is checked with */
  deps: ModuleFile[];
  type: string | null;
  onLoaded: (result: LoadResult) => void;
  /** publish the text as the next version over the running sandbox, where
   * one runs (ADR-0166) */
  onPublish: ((files: ModuleFile[]) => void) | null;
  onSaved: () => void;
  onCopied: () => void;
}) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const [text, setText] = useState(initial);
  const [findings, setFindings] = useState<Finding[]>([]);
  const [diagram, setDiagram] = useState("");
  const [status, setStatus] = useState<string | null>(null);

  useEffect(() => {
    if (!host.current) return;
    const v = new EditorView({
      parent: host.current,
      state: EditorState.create({
        doc: initial,
        extensions: [
          lineNumbers(),
          highlightActiveLine(),
          history(),
          keymap.of([...defaultKeymap, ...historyKeymap]),
          yaml(),
          syntaxHighlighting(defaultHighlightStyle),
          lintGutter(),
          EditorState.readOnly.of(!editable),
          EditorView.updateListener.of((u) => {
            if (u.docChanged) setText(u.state.doc.toString());
          }),
        ],
      }),
    });
    view.current = v;
    return () => v.destroy();
  }, [initial, editable]);

  // checked and drawn once the author pauses
  useEffect(() => {
    let live = true;
    const timer = setTimeout(() => {
      void wire
        .check([...deps, { name: file, text }])
        .then((r) => {
          if (!live) return;
          setFindings(r.findings);
          const v = view.current;
          if (!v) return;
          const doc = v.state.doc;
          const marks: Diagnostic[] = r.findings
            .filter((f) => f.file === file)
            .map((f) => {
              const line = doc.line(Math.min(Math.max(f.line, 1), doc.lines));
              return {
                from: line.from,
                to: line.to,
                severity: f.severity === "fatal" ? "error" : "info",
                message: `${f.code}: ${f.message}`,
              };
            });
          v.dispatch(setDiagnostics(v.state, marks));
        })
        .catch((e: unknown) => {
          if (live) setStatus(String(e));
        });
      if (type) {
        void wire
          .draw(text, type)
          .then((d) => {
            if (live) setDiagram(d.mermaid);
          })
          .catch(() => undefined); // a text that does not parse keeps the last drawing
      }
    }, PAUSE);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [wire, deps, file, text, type]);

  const fatal = findings.filter((f) => f.severity === "fatal");

  async function act(what: () => Promise<string>) {
    try {
      setStatus(await what());
    } catch (e) {
      setStatus(String(e));
    }
  }

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        {editable ? (
          <button
            className="btn"
            onClick={() =>
              void act(async () => {
                await wire.save(file, text);
                onSaved();
                return `Saved ${file} to the workspace.`;
              })
            }
          >
            Save
          </button>
        ) : (
          <button
            className="btn"
            onClick={() =>
              void act(async () => {
                const r = await wire.copy(file);
                onCopied();
                return `Copied ${r.copied.join(" and ")} to the workspace, where it can be edited.`;
              })
            }
          >
            Copy to the workspace to edit
          </button>
        )}
        <button
          className="btn-quiet"
          disabled={fatal.length > 0}
          title={
            fatal.length
              ? "The checker finds the text wrong; it cannot be loaded"
              : ""
          }
          onClick={() =>
            void act(async () => {
              const r = await wire.load([...deps, { name: file, text }]);
              onLoaded(r);
              return r.loaded
                ? "Loaded into a fresh sandbox."
                : "Not loaded: the checker finds it wrong.";
            })
          }
        >
          Load into the sandbox
        </button>
        {onPublish && (
          <button
            className="btn-quiet"
            disabled={fatal.length > 0}
            title="Publish the text as the next version over the running sandbox's objects, its report read first"
            onClick={() => onPublish([...deps, { name: file, text }])}
          >
            Publish into the sandbox
          </button>
        )}
        <span className="text-xs text-slate-500">
          {source === "design"
            ? "the design's module, read-only"
            : "your workspace"}{" "}
          ·{" "}
          {fatal.length
            ? `${fatal.length} finding${fatal.length > 1 ? "s" : ""}`
            : "the checker finds nothing wrong"}
        </span>
      </div>
      {status && <p className="text-xs text-slate-700">{status}</p>}
      <div
        ref={host}
        className="max-h-[32rem] overflow-auto rounded border border-slate-300 text-sm"
      />
      {findings.length > 0 && (
        <ul className="max-h-48 space-y-0.5 overflow-auto text-xs">
          {findings.map((f, i) => (
            <li
              key={i}
              className={
                f.severity === "fatal" ? "text-red-800" : "text-slate-500"
              }
            >
              <span className="font-mono">
                {f.file}:{f.line} {f.code}
              </span>{" "}
              {f.message}
            </li>
          ))}
        </ul>
      )}
      {diagram && (
        <div className="rounded border border-slate-200 bg-white p-3">
          <Diagram source={diagram} />
        </div>
      )}
    </div>
  );
}
