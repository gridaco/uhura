import { semanticAttributes, textAttribute } from "./common.js";
import type { PrimitiveAdapter } from "./types.js";

export const viewAdapter: PrimitiveAdapter = {
  id: "view",
  tag: "div",
  attributes(node) {
    const role = textAttribute(node.attributes, "role");
    return semanticAttributes(node, [
      role === "list" || role === "navigation"
        ? { name: "role", value: role }
        : null,
    ]);
  },
  childrenAreList(node) {
    return textAttribute(node.attributes, "role") === "list";
  },
};
