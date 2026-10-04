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

/** The inputs an offer's schema names, each field fitted to its type: an
 * object chosen from those the session knows, an enumeration's member, or a
 * value in the model's notation (ADR-0150 decision 2). Fields left empty
 * are not sent. */
export function InputForm({
  schema,
  enumerations,
  known,
  onSubmit,
  onCancel,
}: {
  schema: Record<string, string>;
  enumerations: Record<string, string[]>;
  known: Known[];
  onSubmit: (inputs: Record<string, WireValue>) => void;
  onCancel: () => void;
}) {
  const [values, setValues] = useState<Record<string, string | string[]>>({});
  const set = (k: string, v: string | string[]) =>
    setValues((prev) => ({ ...prev, [k]: v }));

  function submit() {
    const out: Record<string, WireValue> = {};
    for (const [k, typ] of Object.entries(schema)) {
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
    onSubmit(out);
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
        return (
          <label key={k} className="block text-xs">
            <span className="font-mono">{k}</span>{" "}
            <span className="text-slate-500">{typ}</span>
            {isObject && many ? (
              <select
                multiple
                className="field"
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
          Request
        </button>
        <button className="btn-quiet" type="button" onClick={onCancel}>
          Cancel
        </button>
      </div>
    </form>
  );
}
