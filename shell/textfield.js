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

/**
 * @typedef {Object} FieldState
 * @property {number} inFlight   change emissions not yet stepped
 * @property {string | undefined} stash last external value seen while typing
 * @property {boolean} composing
 */

/**
 * @param {{ emit: (descriptor: import("./types.js").Descriptor,
 *                  data?: Record<string, unknown>,
 *                  onApplied?: () => void) => void }} wiring
 */
export function createTextFields({ emit }) {
  /** @type {WeakMap<HTMLInputElement, FieldState>} */
  const fields = new WeakMap();

  /** @param {HTMLInputElement} input */
  function state(input) {
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
   * @param {HTMLInputElement} input
   * @param {{ on: Record<string, import("./types.js").Descriptor> }} holder
   */
  function wire(input, holder) {
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

  /**
   * Applies core's draft to the DOM — the reconciler's value applier.
   * @param {HTMLInputElement} input
   * @param {string} value
   */
  function applyValue(input, value) {
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
