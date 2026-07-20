import {
  physicalAttribute,
  semanticAttributes,
  textAttribute,
  UNIT_EVENT,
} from "./common.js";
import type { PrimitiveAdapter } from "./types.js";

export const regionAdapter: PrimitiveAdapter = {
  id: "region",
  tag: "div",
  attributes(node, mode) {
    return semanticAttributes(node, [
      { name: "role", value: "button" },
      mode === "play" ? { name: "tabindex", value: "0" } : null,
      physicalAttribute(
        "aria-label",
        textAttribute(node.attributes, "label"),
      ),
    ]);
  },
  bindSupplementalEvents(context) {
    const activationIndex = context.events.findIndex((candidate) =>
      candidate.event === "activate"
      || candidate.event === "press"
      || candidate.event === "activate-double"
    );
    if (activationIndex < 0) return [];
    return [{
      type: "keydown",
      listener: (domEvent) => {
        if (
          !(domEvent instanceof KeyboardEvent)
          || (domEvent.key !== "Enter" && domEvent.key !== " ")
          || !context.eventAllowed(context.target)
        ) {
          return;
        }
        domEvent.preventDefault();
        context.dispatchAt(activationIndex, UNIT_EVENT);
      },
    }];
  },
};
