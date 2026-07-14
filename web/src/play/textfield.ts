// text-field mechanics (§8.4, normative): core owns the DRAFT; the
// renderer owns caret and IME. Per field, a counter of in-flight change
// emissions; while nonzero, external replacement never applies (stashed)
// — a tick-scheduled outcome landing mid-typing cannot eat keystrokes.
// IME composition buffers locally and emits one `change` at
// `compositionend`.
//
// State is keyed by the INPUT ELEMENT (WeakMap): teardown is automatic
// with the DOM node, and a remounted field starts fresh — paths embed
// freshly minted page serials, so they never made a stable key.

import type { Descriptor } from "../protocol/types.js";
import type {
  TextFieldController,
  TextFieldHolder,
} from "../renderer/contracts.js";

interface FieldState {
  /** Change emissions not yet stepped. */
  inFlight: number;
  /** Last external value seen while typing. */
  stash: string | undefined;
  composing: boolean;
}

interface TextFieldWiring {
  emit(
    descriptor: Descriptor,
    data?: Record<string, unknown>,
    onApplied?: () => void,
  ): void;
}

export function createTextFields({ emit }: TextFieldWiring): TextFieldController {
  const fields = new WeakMap<HTMLInputElement, FieldState>();

  function state(input: HTMLInputElement): FieldState {
    let s = fields.get(input);
    if (!s) {
      s = { inFlight: 0, stash: undefined, composing: false };
      fields.set(input, s);
    }
    return s;
  }

  /**
   * Wires one `<input>`; called once per mounted field. Descriptors are
   * read from `holder.on` at FIRE time — payloads rotate across steps.
   */
  function wire(input: HTMLInputElement, holder: TextFieldHolder): void {
    const emitChange = () => {
      const descriptor = holder.on["change"];
      if (!descriptor) return;
      const s = state(input);
      s.inFlight += 1;
      emit(descriptor, { value: input.value }, () => {
        s.inFlight -= 1;
        if (s.inFlight === 0 && s.stash !== undefined) {
          // The last external value wins only once typing settled AND
          // core did not echo the draft back meanwhile.
          if (s.stash !== input.value) input.value = s.stash;
          s.stash = undefined;
        }
      });
    };

    input.addEventListener("input", () => {
      if (state(input).composing) return;
      emitChange();
    });
    input.addEventListener("compositionstart", () => {
      state(input).composing = true;
    });
    input.addEventListener("compositionend", () => {
      state(input).composing = false;
      emitChange();
    });
    input.addEventListener("keydown", (event) => {
      // An Enter that commits an IME conversion is not a submit gesture.
      // WebKit fires it AFTER compositionend (isComposing false), but
      // stamps keyCode 229 — both tells are required (§8.4 normative:
      // one `change` per composition, nothing else).
      if (event.isComposing || event.keyCode === 229) return;
      if (event.key !== "Enter" || state(input).composing) return;
      const descriptor = holder.on["submit"];
      if (descriptor) {
        event.preventDefault();
        emit(descriptor);
      }
    });
  }

  /** Applies core's draft to the DOM — the reconciler's value applier. */
  function applyValue(input: HTMLInputElement, value: string): void {
    const s = state(input);
    if (s.composing || s.inFlight > 0) {
      // The user is ahead of the machine: never clobber, stash.
      s.stash = value;
      return;
    }
    s.stash = undefined;
    if (input.value !== value) input.value = value;
  }

  return { wire, applyValue };
}

export type { TextFieldController, TextFieldHolder } from "../renderer/contracts.js";
