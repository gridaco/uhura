import {
  booleanAttribute,
  physicalAttribute,
  presentBooleanAttribute,
  semanticAttributes,
  textAttribute,
} from "./common.js";
import type { PrimitiveAdapter } from "./types.js";

export const buttonAdapter: PrimitiveAdapter = {
  id: "button",
  tag: "button",
  attributes(node) {
    const pressed =
      booleanAttribute(node.attributes, "pressed")
      ?? booleanAttribute(node.attributes, "aria-pressed");
    return semanticAttributes(node, [
      { name: "type", value: "button" },
      physicalAttribute(
        "aria-label",
        textAttribute(node.attributes, "label")
          ?? textAttribute(node.attributes, "aria-label"),
      ),
      booleanAttribute(node.attributes, "busy") === true
        ? { name: "aria-busy", value: "true" }
        : null,
      pressed === undefined
        ? null
        : { name: "aria-pressed", value: String(pressed) },
      booleanAttribute(node.attributes, "current") === true
        ? { name: "aria-current", value: "true" }
        : null,
      presentBooleanAttribute(
        "disabled",
        booleanAttribute(node.attributes, "disabled"),
      ),
    ]);
  },
  sync(element, node) {
    const button = element as HTMLButtonElement;
    const disabled = booleanAttribute(node.attributes, "disabled") === true;
    if (button.disabled !== disabled) button.disabled = disabled;
  },
};
