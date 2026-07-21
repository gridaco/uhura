import {
  booleanAttribute,
  semanticAttributes,
  textAttribute,
  textEvent,
} from "./common.js";
import type {
  PrimitiveAdapter,
} from "./types.js";

interface TextFieldState {
  composing: boolean;
  inFlight: number;
  stash: string | undefined;
}

const textFieldStates = new WeakMap<HTMLInputElement, TextFieldState>();

const textFieldState = (input: HTMLInputElement): TextFieldState => {
  let state = textFieldStates.get(input);
  if (!state) {
    state = { composing: false, inFlight: 0, stash: undefined };
    textFieldStates.set(input, state);
  }
  return state;
};

const inputMechanic = (
  wrapper: HTMLElement,
): HTMLInputElement | undefined =>
  Array.from(wrapper.children).find(
    (child): child is HTMLInputElement =>
      child.localName === "input"
      && (child as HTMLElement).dataset["uhMechanic"] === "input",
  );

const ensureInput = (
  wrapper: HTMLElement,
): HTMLInputElement => {
  const existing = inputMechanic(wrapper);
  const input = existing ?? wrapper.ownerDocument.createElement("input");
  if (!existing) {
    input.type = "text";
    input.dataset["uhMechanic"] = "input";
    wrapper.append(input);
  }
  return input;
};

export const textfieldAdapter: PrimitiveAdapter = {
  id: "textfield",
  tag: "div",
  attributes: (node) => semanticAttributes(node),
  hosts(element, node, context) {
    const input = ensureInput(element);
    context.applyAttributes(input, [
      ...(textAttribute(node.attributes, "placeholder") === undefined
        ? []
        : [{
          name: "placeholder",
          value: textAttribute(node.attributes, "placeholder") ?? "",
        }]),
      ...(textAttribute(node.attributes, "label") === undefined
        ? []
        : [{
          name: "aria-label",
          value: textAttribute(node.attributes, "label") ?? "",
        }]),
      ...(booleanAttribute(node.attributes, "disabled") === true
        ? [{ name: "disabled", value: true }]
        : []),
    ]);
    const disabled = booleanAttribute(node.attributes, "disabled") === true;
    const readOnly = context.mode === "editor";
    if (input.disabled !== disabled) input.disabled = disabled;
    if (input.readOnly !== readOnly) input.readOnly = readOnly;

    const value = textAttribute(node.attributes, "value") ?? "";
    const state = textFieldState(input);
    if (context.mode === "editor") {
      state.stash = undefined;
      if (input.value !== value) input.value = value;
    } else if (state.composing || state.inFlight > 0) {
      state.stash = value;
    } else {
      state.stash = undefined;
      if (input.value !== value) input.value = value;
    }
    return { children: null, events: input };
  },
  bindEvent(context) {
    if (context.event.event !== "change") return undefined;
    const input = context.target as HTMLInputElement;
    const dispatchChange = (): void => {
      const state = textFieldState(input);
      state.inFlight += 1;
      try {
        context.dispatch(textEvent(input.value));
      } finally {
        state.inFlight -= 1;
        if (state.inFlight === 0 && state.stash !== undefined) {
          if (state.stash !== input.value) input.value = state.stash;
          state.stash = undefined;
        }
      }
    };
    const onInput: EventListener = (domEvent) => {
      const composing =
        textFieldState(input).composing
        || ("isComposing" in domEvent && domEvent.isComposing === true);
      if (!composing && context.eventAllowed(input)) dispatchChange();
    };
    const onCompositionStart: EventListener = () => {
      textFieldState(input).composing = true;
    };
    const onCompositionEnd: EventListener = () => {
      textFieldState(input).composing = false;
      if (context.eventAllowed(input)) dispatchChange();
    };
    return [
      { type: "input", listener: onInput },
      { type: "compositionstart", listener: onCompositionStart },
      { type: "compositionend", listener: onCompositionEnd },
    ];
  },
};
