// Focus mechanics (§8.4), scoped to one mounted Play route. The machine owns
// WHAT gets focus back; this controller owns HOW and cancels queued work when
// the route is disposed.

import type { Intent } from "../protocol/types.js";

const FOCUSABLE = 'button, input, [tabindex="0"]';

export interface FocusController {
  handleIntents(intents: Intent[]): void;
  enterSurface(surfaceEl: HTMLElement): void;
  dispose(): void;
}

export function createFocusController(root: HTMLElement): FocusController {
  let active = true;

  function restoreFocus(keyPath: string): void {
    if (!active) return;
    const escaped = CSS.escape(keyPath);
    const element = root.querySelector(`[data-path="${escaped}"]`);
    if (!(element instanceof HTMLElement)) return;
    const target = element.matches(FOCUSABLE)
      ? element
      : element.querySelector(FOCUSABLE);
    if (target instanceof HTMLElement) target.focus();
  }

  function handleIntents(intents: Intent[]): void {
    for (const intent of intents) {
      if (intent.intent !== "focus-restore") continue;
      const path = intent["key-path"];
      queueMicrotask(() => restoreFocus(path));
    }
  }

  function enterSurface(surfaceEl: HTMLElement): void {
    if (!active) return;
    const target = surfaceEl.querySelector(FOCUSABLE);
    if (target instanceof HTMLElement) target.focus();
    else surfaceEl.focus();
  }

  function dispose(): void {
    active = false;
  }

  return { handleIntents, enterSurface, dispose };
}
