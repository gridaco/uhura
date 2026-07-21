import {
  booleanAttribute,
  semanticAttributes,
  textAttribute,
} from "./common.js";
import type { PrimitiveAdapter } from "./types.js";

export const imageAdapter: PrimitiveAdapter = {
  id: "img",
  tag: "img",
  attributes(node) {
    return semanticAttributes(node, [{
      name: "alt",
      value: booleanAttribute(node.attributes, "decorative") === true
        ? ""
        : textAttribute(node.attributes, "alt") ?? "",
    }]);
  },
  hosts: (element) => ({ children: null, events: element }),
  sync(element, node, _hosts, context) {
    const image = element as HTMLImageElement;
    if (context.options.assets) {
      context.options.assets.applyImage(
        image,
        textAttribute(node.attributes, "src"),
      );
    } else {
      image.removeAttribute("src");
    }
  },
};
