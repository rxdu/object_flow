import type { Declaration } from "../api/wire";

/** A type's transitions and the conditions guarding them, as declared: each
 * guard is read as written, never restated (renderers.md §1). */
export function RuleSet({ decl }: { decl: Declaration }) {
  const conditions = decl.conditions ?? {};
  const from = (f: string[] | string | undefined) =>
    Array.isArray(f) ? f.join(", ") : (f ?? "");
  return (
    <table className="w-full text-sm">
      <thead>
        <tr className="text-left text-slate-500">
          <th className="py-1 pr-3 font-medium">Transition</th>
          <th className="py-1 pr-3 font-medium">From → to</th>
          <th className="py-1 font-medium">Guards</th>
        </tr>
      </thead>
      <tbody>
        {Object.entries(decl.transitions ?? {}).map(([name, x]) => (
          <tr key={name} className="border-t border-slate-200 align-top">
            <td className="py-1 pr-3 font-mono">{name}</td>
            <td className="py-1 pr-3 text-slate-600">
              {x.kind === "initial" ? "●" : from(x.from)} → {x.to ?? "(stays)"}
            </td>
            <td className="py-1">
              {Object.entries(x.guards ?? {}).map(([g, mode]) => (
                <div key={g}>
                  <span className="font-mono">{g}</span>{" "}
                  <span className="text-xs text-slate-500">
                    {mode}, {conditions[g]?.remedy ?? "no remedy declared"}
                  </span>
                  <div className="font-mono text-xs text-slate-600">
                    {conditions[g]?.expression}
                  </div>
                </div>
              ))}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
