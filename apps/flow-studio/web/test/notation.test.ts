import { describe, expect, it } from "vitest";
import {
  durationSeconds,
  instantText,
  resolveInstant,
  sameDecimal,
} from "../src/session/notation";

describe("the model's notation", () => {
  it("reads a duration in every unit the format writes", () => {
    expect(durationSeconds("90 s")).toBe(90);
    expect(durationSeconds("30 min")).toBe(1800);
    expect(durationSeconds("1 h")).toBe(3600);
    expect(durationSeconds("1 day")).toBe(86400);
    expect(durationSeconds("2 days")).toBe(172800);
    expect(durationSeconds("1 week")).toBe(604800);
    expect(durationSeconds("-1 h")).toBe(-3600);
    expect(() => durationSeconds("1 fortnight")).toThrow();
  });

  it("resolves an example's instant against the session's clock", () => {
    const now = 1767225600; // 2026-01-01T00:00:00Z, the examples' clock start
    expect(instantText(now)).toBe("2026-01-01T00:00:00Z");
    expect(resolveInstant("now", now)).toBe("2026-01-01T00:00:00Z");
    expect(resolveInstant("now - 1 day", now)).toBe("2025-12-31T00:00:00Z");
    expect(resolveInstant("now + 2 h", now)).toBe("2026-01-01T02:00:00Z");
    expect(resolveInstant("2026-03-01T09:30Z", now)).toBe(
      "2026-03-01T09:30:00Z",
    );
  });

  it("compares decimals and amounts exactly, whatever their scale", () => {
    expect(sameDecimal("12.5", "12.50")).toBe(true);
    expect(sameDecimal("SGD 12000.00", "SGD 12000")).toBe(true);
    expect(sameDecimal(0.1, "0.10")).toBe(true);
    expect(sameDecimal("0.1", "0.01")).toBe(false);
    expect(sameDecimal("-0.05", "0.05")).toBe(false);
    expect(sameDecimal("9007199254740993.01", "9007199254740993.010")).toBe(
      true,
    );
  });
});
