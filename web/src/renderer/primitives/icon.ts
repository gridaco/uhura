import { semanticAttributes, textAttribute } from "./common.js";
import type { PrimitiveAdapter } from "./types.js";

export const iconAdapter: PrimitiveAdapter = {
  id: "icon",
  tag: "span",
  capabilities: ["icon-fonts"],
  attributes: (node) =>
    semanticAttributes(node, [{ name: "aria-hidden", value: "true" }]),
  hosts: (element) => ({ children: null, events: element }),
  sync(element, node, _hosts, context) {
    const name = textAttribute(node.attributes, "name") ?? "";
    const family = textAttribute(node.attributes, "family");
    const icons = context.options.icons;
    if (icons && name.length > 0) {
      const realizedFamily = family ?? icons.defaultFamily;
      if (
        element.dataset["icon"] !== name
        || element.dataset["iconFamily"] !== realizedFamily
        || element.dataset["iconResource"] !== icons.fingerprint
      ) {
        icons.apply(element, family, name);
        element.dataset["icon"] = name;
        element.dataset["iconFamily"] = realizedFamily;
        element.dataset["iconResource"] = icons.fingerprint;
      }
    } else {
      element.textContent = "";
      delete element.dataset["icon"];
      delete element.dataset["iconFamily"];
      delete element.dataset["iconResource"];
    }
  },
};
