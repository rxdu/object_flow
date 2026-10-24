import { describe, expect, it } from "vitest";
import { version as react } from "react";
import { version as reactDom } from "react-dom";

describe("the React the page renders with", () => {
  it("is one version of react and react-dom, which react-dom requires", async () => {
    // react-dom throws "Incompatible React versions" as it loads beside a
    // react of any other version, so the page renders nothing; a bump of
    // one without the other passes every other check
    expect(reactDom).toBe(react);
    await expect(import("react-dom/client")).resolves.toHaveProperty(
      "createRoot",
    );
  });
});
