import { describe, expect, it } from "vitest";
import { inputsOf } from "../src/components/InputForm";

const schema = {
  note: "string",
  count: "int",
  urgent: "bool",
  tags: "string[]",
};

describe("the inputs a form sends", () => {
  it("leaves out a field left empty and fits each other to its type", () => {
    expect(
      inputsOf(schema, { note: "", count: "3", urgent: "true", tags: "a, b" }),
    ).toEqual({ count: 3, urgent: true, tags: ["a", "b"] });
  });

  it("sends an input marked cleared as null, whatever was typed in it (ADR-0262)", () => {
    expect(
      inputsOf(schema, { note: "kept?", count: "3" }, new Set(["note"])),
    ).toEqual({ note: null, count: 3 });
    expect(inputsOf(schema, {}, new Set(["tags"]))).toEqual({ tags: null });
  });
});
