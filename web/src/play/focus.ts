// Focus mechanics (§8.4): the machine owns WHAT gets focus back
// (`focus-restore` intents with a key-path); the renderer owns HOW.
// Key-paths are `<scope>/<node keys joined "/">` (§8.1) — the reconciler
// stamps them as `data-path`.

const FOCUSABLE = 'button, input, [tabindex="0"]';

import type { Intent } from "../protocol/types.js";

/**
 * Executes one step's intents. History intents are visible no-ops in the
 * spike shell — the machine already owns the nav stack (§7.4); the
 * contract stays visible in the trace.
 */
export function handleIntents(intents: Intent[]): void {
  for (const intent of intents) {
    if (intent.intent === "focus-restore") {
      const path = intent["key-path"];
      // After the pump drains: the dismissing step's DOM must be in
      // place before the node search runs.
      queueMicrotask(() => restoreFocus(path));
    }
  }
}

export function restoreFocus(keyPath: string): void {
  const el = document.querySelector(`[data-path="${CSS.escape(keyPath)}"]`);
  if (!(el instanceof HTMLElement)) return;
  const target = el.matches(FOCUSABLE) ? el : el.querySelector(FOCUSABLE);
  if (target instanceof HTMLElement) target.focus();
}

/**
 * A freshly mounted surface takes focus (§11.4 step 5: "focus enters").
 */
export function enterSurface(surfaceEl: HTMLElement): void {
  const target = surfaceEl.querySelector(FOCUSABLE);
  if (target instanceof HTMLElement) target.focus();
  else surfaceEl.focus();
}
