import { useState } from "react";
import type { Finding, Wire } from "../api/wire";
import { type Draft, draftExample } from "../session/draft";
import type { Player } from "../session/player";

/** A name an example may take, from its description. */
function nameFrom(description: string): string {
  return description
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "")
    .replace(/^(\d)/, "n$1")
    .slice(0, 60);
}

/** The session saved as an example of the module (docs/design/flow-studio.md
 * §5): drafted from what was done, the expectation proposed from what the
 * last request did, the description the author's, and written only once the
 * flow checker finds that it holds. */
export function SaveExample({
  wire,
  player,
  module,
}: {
  wire: Wire;
  player: Player;
  module: string;
}) {
  const [description, setDescription] = useState("");
  const [name, setName] = useState("");
  const [draft, setDraft] = useState<Draft | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [findings, setFindings] = useState<Finding[]>([]);

  async function propose() {
    setSaved(null);
    setFindings([]);
    setDraft(await draftExample(player, description));
  }

  async function save() {
    if (!draft?.example) return;
    const r = await wire.addExample(
      module,
      name || nameFrom(description),
      draft.example,
    );
    if (r.saved) {
      setSaved(
        `Saved to ${module.replace(".yaml", ".examples.yaml")}: the flow checker ran it, and it holds.`,
      );
      setDraft(null);
    } else {
      setFindings((r.findings ?? []).filter((f) => f.severity === "fatal"));
    }
  }

  return (
    <div className="space-y-2">
      <h3 className="label">Save the session as an example</h3>
      <textarea
        className="field h-16"
        placeholder="What the example shows, in your words"
        value={description}
        onChange={(e) => setDescription(e.target.value)}
      />
      <input
        className="field font-mono"
        placeholder={nameFrom(description) || "its_name"}
        value={name}
        onChange={(e) => setName(e.target.value)}
      />
      <button
        className="btn-quiet"
        disabled={!description.trim()}
        onClick={() => void propose()}
      >
        Draft it
      </button>
      {draft && (
        <div className="space-y-2 text-xs">
          {draft.problems.map((p, i) => (
            <p key={i} className="text-red-800">
              {p}
            </p>
          ))}
          {draft.notes.map((n, i) => (
            <p key={i} className="text-slate-500">
              {n}
            </p>
          ))}
          {draft.example && (
            <>
              <p className="text-slate-600">
                The claim below is what the last request did. Save it only if
                that is what the flow should do.
              </p>
              <pre className="max-h-64 overflow-auto rounded bg-slate-50 p-2">
                {JSON.stringify(draft.example, null, 2)}
              </pre>
              <button className="btn" onClick={() => void save()}>
                It should: save it
              </button>
            </>
          )}
        </div>
      )}
      {findings.length > 0 && (
        <div className="text-xs text-red-800">
          <p>Not saved; the flow checker finds:</p>
          {findings.map((f, i) => (
            <p key={i} className="font-mono">
              {f.code} {f.message}
            </p>
          ))}
        </div>
      )}
      {saved && <p className="text-xs text-emerald-800">{saved}</p>}
    </div>
  );
}
