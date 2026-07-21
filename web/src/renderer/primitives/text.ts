import { semanticAttributes } from "./common.js";
import type { PrimitiveAdapter } from "./types.js";

export const textAdapter: PrimitiveAdapter = {
  id: "text",
  tag: "p",
  attributes: (node) => semanticAttributes(node),
};
