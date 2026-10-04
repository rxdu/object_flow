import { useEffect, useState } from "react";
import { type Declaration, NotRun, type Wire } from "../api/wire";

/** A type's declared metrics, each read over every row it selects, after
 * every step: how the flow's numbers move as its objects do. A metric the
 * backend cannot read says so (docs/design/flow-studio.md §3.3). */
export function Metrics({
  wire,
  type,
  decl,
  actor,
  tick,
}: {
  wire: Wire;
  type: string;
  decl: Declaration;
  actor: string;
  tick: number;
}) {
  const names = Object.keys(decl.metrics ?? {});
  const [read, setRead] = useState<Record<string, string>>({});

  useEffect(() => {
    let live = true;
    void Promise.all(
      Object.keys(decl.metrics ?? {}).map(async (n) => {
        try {
          const page = await wire.metric(actor, `${type}.${n}`);
          return [n, JSON.stringify(page.rows[0]?.value ?? null)] as const;
        } catch (e) {
          return [n, e instanceof NotRun ? "not run" : "fault"] as const;
        }
      }),
    ).then((pairs) => {
      if (live) setRead(Object.fromEntries(pairs));
    });
    return () => {
      live = false;
    };
  }, [wire, type, decl, actor, tick]);

  if (names.length === 0)
    return <p className="text-sm text-slate-500">{type} declares no metric.</p>;
  return (
    <dl className="grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1 text-sm tabular-nums">
      {names.map((n) => (
        <div key={n} className="contents">
          <dt className="font-mono text-slate-600">{n}</dt>
          <dd>{read[n] ?? "…"}</dd>
        </div>
      ))}
    </dl>
  );
}
