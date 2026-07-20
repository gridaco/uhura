import { describe, expect, it } from "vitest";

import { projectionNeedsIconFonts } from "../editor.js";
import {
  elementNode,
  textNode,
} from "./fixtures/projection.js";

describe("Editor projection icon resources", () => {
  it("loads icon fonts only when a canonical projection contains an icon", () => {
    expect(projectionNeedsIconFonts([
      elementNode("root", [
        textNode("copy", "No icon"),
      ]),
    ])).toBe(false);

    expect(projectionNeedsIconFonts([
      elementNode("root", [
        elementNode("nested", [
          elementNode("heart", [], { element: "icon" }),
        ]),
      ]),
    ])).toBe(true);
  });
});
