import { useEffect, useState } from "react";
import {
  type FlowObject,
  type HistoryItem,
  type Wire,
  isLegacy,
} from "../api/wire";
import { OPERATOR, type Player } from "../session/player";

/** Every object the session's requests wrote, each read through `get`,
 * named as the example names it where it does, and the selected one's
 * history through `history`. */
export function Objects({
  wire,
  player,
  tick,
  selected,
  onSelect,
}: {
  wire: Wire;
  player: Player | null;
  tick: number;
  selected: string | null;
  onSelect: (id: string) => void;
}) {
  const [objects, setObjects] = useState<[string, FlowObject][]>([]);
  const [events, setEvents] = useState<HistoryItem[]>([]);

  useEffect(() => {
    let live = true;
    const aliases = player?.aliases ?? new Map<string, string>();
    const names = new Map([...aliases].map(([a, id]) => [id, a]));
    const ids = [
      ...[...aliases]
        .filter(([a, id]) => a !== OPERATOR && !id.startsWith("proposal-"))
        .map(([, id]) => id),
      ...(player?.touched ?? []),
    ];
    const named = [...new Set(ids)].map(
      (id) => [names.get(id) ?? id, id] as const,
    );
    void Promise.all(
      named.map(async ([alias, id]) => [alias, await wire.get(id)] as const),
    ).then((rows) => {
      if (live)
        setObjects(
          rows.filter((r): r is [string, FlowObject] => r[1] !== null),
        );
    });
    return () => {
      live = false;
    };
  }, [wire, player, tick]);

  useEffect(() => {
    let live = true;
    if (selected) {
      void wire.history(selected).then((h) => {
        if (live) setEvents(h?.events ?? []);
      });
    }
    return () => {
      live = false;
    };
  }, [wire, selected, tick]);

  // an id written as the example names its object, so a value reads as the
  // example does
  const names = new Map(
    [...(player?.aliases ?? new Map<string, string>())].map(([a, id]) => [
      id,
      a,
    ]),
  );
  const shown = (v: unknown): string =>
    Array.isArray(v)
      ? `[${v.map(shown).join(", ")}]`
      : typeof v === "string" && names.has(v)
        ? (names.get(v) ?? v)
        : JSON.stringify(v);

  if (objects.length === 0)
    return (
      <p className="text-sm leading-relaxed text-slate-500">
        No object yet. Run an example's steps, or make one under{" "}
        <em>Free play</em>.
      </p>
    );
  return (
    <div className="space-y-3">
      <ul className="space-y-1">
        {objects.map(([alias, o]) => (
          <li key={o.id}>
            <button
              className={`w-full rounded-md border-l-2 px-2 py-1.5 text-left text-sm transition-colors ${selected === o.id ? "border-indigo-600 bg-indigo-50" : "border-transparent hover:bg-slate-100"}`}
              onClick={() => onSelect(o.id)}
            >
              <span className="font-mono">{alias}</span>{" "}
              <span className="text-slate-500">{o.type}</span>{" "}
              <span className="rounded-full bg-slate-700 px-2 py-0.5 text-[11px] font-medium tracking-wide text-white">
                {o.state}
              </span>
              <div className="font-mono text-xs break-all text-slate-600">
                {Object.entries(o.attributes)
                  .filter(
                    ([, v]) =>
                      v !== null && !(Array.isArray(v) && v.length === 0),
                  )
                  .map(([k, v]) => `${k}=${shown(v)}`)
                  .join("  ")}
              </div>
            </button>
          </li>
        ))}
      </ul>
      {selected && (
        <table className="w-full text-xs tabular-nums">
          <thead>
            <tr className="text-left text-slate-500">
              <th className="pr-2 font-medium">#</th>
              <th className="pr-2 font-medium">Transition</th>
              <th className="pr-2 font-medium">State</th>
              <th className="pr-2 font-medium">By</th>
              <th className="font-medium">Occurred</th>
            </tr>
          </thead>
          <tbody>
            {events.map((e) =>
              isLegacy(e) ? (
                // history from before the store, attached at import
                <tr
                  key={`L${e.ordinal}`}
                  className="border-t border-slate-200 text-slate-500"
                >
                  <td className="pr-2">L{e.ordinal}</td>
                  <td className="pr-2 font-mono">legacy {e.kind}</td>
                  <td className="pr-2 font-mono" colSpan={2}>
                    {Object.entries(e.payload)
                      .map(([k, v]) => `${k}=${JSON.stringify(v)}`)
                      .join("  ")}
                  </td>
                  <td className="font-mono">{e.occurred_at}</td>
                </tr>
              ) : (
                <tr key={e.position} className="border-t border-slate-200">
                  <td className="pr-2">{e.position}</td>
                  <td className="pr-2 font-mono">{e.transition}</td>
                  <td className="pr-2">
                    {e.from_state ?? "●"} → {e.to_state}
                  </td>
                  <td className="pr-2">
                    {e.actor_id} ({e.actor_kind})
                  </td>
                  <td className="font-mono">{e.occurred_at}</td>
                </tr>
              ),
            )}
          </tbody>
        </table>
      )}
    </div>
  );
}
