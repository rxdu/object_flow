import { useState } from "react";
import type { WireValue } from "../api/wire";

export interface Known {
  id: string;
  name: string;
  type: string;
}

const SCALAR =
  /^(string|text|bool|int|counter|decimal\(.*\)|money\(.*\)|timestamp|date|duration|identity|actor_kind|file|event|state|invariant)$/;

const HINT: Record<string, string> = {
  timestamp: "2026-01-01T09:00:00Z",
  duration: "1 h",
  date: "2026-01-01",
};

/** The inputs a form sends: each field fitted to its type, a field left
 * empty not sent, and an input marked cleared sent `null`, which clears its
 * attribute where the offer names it clearable (ADR-0262). */
export function inputsOf(
  schema: Record<string, string>,
  values: Record<string, string | string[]>,
  cleared: ReadonlySet<string> = new Set(),
): Record<string, WireValue> {
  const out: Record<string, WireValue> = {};
  for (const [k, typ] of Object.entries(schema)) {
    if (cleared.has(k)) {
      out[k] = null;
      continue;
    }
    const v = values[k];
    if (v === undefined || v === "" || (Array.isArray(v) && v.length === 0))
      continue;
    const base = typ.replace(/\[\]$/, "");
    if (Array.isArray(v)) out[k] = v;
    else if (typ.endsWith("[]"))
      out[k] = v
        .split(",")
        .map((x) => x.trim())
        .filter(Boolean);
    else if (base === "bool") out[k] = v === "true";
    else if (base === "int" || base === "counter") out[k] = Number(v);
    else out[k] = v;
  }
  return out;
}

/** The inputs an offer's schema names, each field fitted to its type: an
 * object chosen from those the session knows, an enumeration's member, or a
 * value in the model's notation (ADR-0150 decision 2). Fields left empty
 * are not sent, and each input the offer names clearable has a control that
 * sends it `null`, so a person can clear what a platform's own form clears
 * (ADR-0262). */
export function InputForm({
  schema,
  clearable = [],
  enumerations,
  known,
  onSubmit,
  onCancel,
  action = "Request",
}: {
  schema: Record<string, string>;
  /** the inputs a `null` clears, as the offer names them */
  clearable?: string[];
  enumerations: Record<string, string[]>;
  known: Known[];
  onSubmit: (inputs: Record<string, WireValue>) => void;
  onCancel: () => void;
  /** what submitting does, as its button says it */
  action?: string;
}) {
  const [values, setValues] = useState<Record<string, string | string[]>>({});
  const [cleared, setCleared] = useState<ReadonlySet<string>>(new Set());
  const set = (k: string, v: string | string[]) =>
    setValues((prev) => ({ ...prev, [k]: v }));
  const clear = (k: string, on: boolean) =>
    setCleared((prev) => {
      const next = new Set(prev);
      if (on) next.add(k);
      else next.delete(k);
      return next;
    });

  function submit() {
    onSubmit(inputsOf(schema, values, cleared));
  }

  return (
    <form
      className="mt-2 space-y-2 rounded bg-slate-50 p-2"
      onSubmit={(e) => {
        e.preventDefault();
        submit();
      }}
    >
      {Object.entries(schema).map(([k, typ]) => {
        const base = typ.replace(/\[\]$/, "");
        const many = typ.endsWith("[]");
        const members = enumerations[base];
        // an object of the type named, or, where the session knows none of
        // it, any it knows, a subtype's among them; the request decides
        const exact = known.filter((o) => o.type === base);
        const objects = exact.length > 0 ? exact : known;
        const isObject = !SCALAR.test(base) && !members;
        const off = cleared.has(k);
        return (
          <label key={k} className="block text-xs">
            <span className="font-mono">{k}</span>{" "}
            <span className="text-slate-500">{typ}</span>
            {clearable.includes(k) && (
              <span className="ml-2 inline-flex items-center gap-1">
                <input
                  type="checkbox"
                  checked={off}
                  aria-label={`clear ${k}`}
                  onChange={(e) => clear(k, e.target.checked)}
                />
                clear
              </span>
            )}
            {isObject && many ? (
              <select
                multiple
                className="field"
                disabled={off}
                onChange={(e) =>
                  set(
                    k,
                    [...e.target.selectedOptions].map((o) => o.value),
                  )
                }
              >
                {objects.map((o) => (
                  <option key={o.id} value={o.id}>
                    {o.name}
                  </option>
                ))}
              </select>
            ) : isObject || members ? (
              <select
                className="field"
                disabled={off}
                defaultValue=""
                onChange={(e) => set(k, e.target.value)}
              >
                <option value="">(none)</option>
                {(members ?? []).map((m) => (
                  <option key={m}>{m}</option>
                ))}
                {objects.map((o) => (
                  <option key={o.id} value={o.id}>
                    {o.name}
                  </option>
                ))}
              </select>
            ) : base === "bool" ? (
              <select
                className="field"
                disabled={off}
                defaultValue=""
                onChange={(e) => set(k, e.target.value)}
              >
                <option value="">(none)</option>
                <option>true</option>
                <option>false</option>
              </select>
            ) : (
              <input
                className="field"
                disabled={off}
                placeholder={
                  many
                    ? "comma-separated"
                    : (HINT[base] ??
                      (base.startsWith("money(")
                        ? `${base.slice(6, -1)} 0.00`
                        : base))
                }
                onChange={(e) => set(k, e.target.value)}
              />
            )}
          </label>
        );
      })}
      <div className="flex gap-2">
        <button className="btn" type="submit">
          {action}
        </button>
        <button className="btn-quiet" type="button" onClick={onCancel}>
          Cancel
        </button>
      </div>
    </form>
  );
}
