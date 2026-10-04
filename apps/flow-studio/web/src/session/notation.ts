// The model's notation for the values an example writes and the wire carries
// as strings (ADR-0150 decision 2): instants, durations and exact decimals.

const SECONDS: Record<string, number> = {
  s: 1,
  min: 60,
  h: 3600,
  day: 86400,
  days: 86400,
  week: 604800,
  weeks: 604800,
};

/** A duration written as a count and a unit, `2 days`, in seconds. */
export function durationSeconds(text: string): number {
  const m = /^(-?\d+) (\w+)$/.exec(text.trim());
  const unit = m?.[2];
  if (!m || unit === undefined || !(unit in SECONDS)) {
    throw new Error(`not a duration: ${text}`);
  }
  return Number(m[1]) * (SECONDS[unit] ?? 0);
}

/** An RFC 3339 instant, or a minute-precision one as examples write it, in
 * seconds since 1970. */
export function instantSeconds(text: string): number {
  const ms = Date.parse(text);
  if (Number.isNaN(ms)) throw new Error(`not a timestamp: ${text}`);
  return Math.round(ms / 1000);
}

export function instantText(seconds: number): string {
  return new Date(seconds * 1000).toISOString().replace(/\.\d{3}Z$/, "Z");
}

/** An example's timestamp, `now`, `now - 1 day` or an instant, against the
 * clock the session knows. */
export function resolveInstant(text: string, now: number): string {
  const m = /^now(?: ([+-]) (\d+) (\w+))?$/.exec(text);
  if (!m) return instantText(instantSeconds(text));
  const shift = m[1] ? durationSeconds(`${m[2]} ${m[3]}`) : 0;
  return instantText(m[1] === "-" ? now - shift : now + shift);
}

/** Whether two decimals, or amounts with a currency, are the same number;
 * compared exactly, never through a binary float. */
export function sameDecimal(a: string | number, b: string | number): boolean {
  const scaled = (v: string | number): [bigint, number] => {
    const text = String(v).trim().split(/\s+/).pop() ?? "";
    const m = /^(-?)(\d+)(?:\.(\d+))?$/.exec(text);
    if (!m) throw new Error(`not a decimal: ${v}`);
    const frac = m[3] ?? "";
    return [BigInt(`${m[1]}${m[2]}${frac}`), frac.length];
  };
  const [x, xs] = scaled(a);
  const [y, ys] = scaled(b);
  const places = Math.max(xs, ys);
  return x * 10n ** BigInt(places - xs) === y * 10n ** BigInt(places - ys);
}
