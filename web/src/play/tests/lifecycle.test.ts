import assert from "node:assert/strict";
import { test } from "vitest";

import { createFocusController } from "../focus.js";
import { createSurfaces } from "../surfaces.js";

test("disposing focus cancels an already queued restore", async () => {
  let queries = 0;
  const root = {
    querySelector() {
      queries += 1;
      return null;
    },
  } as unknown as HTMLElement;
  const focus = createFocusController(root);

  focus.handleIntents([
    { intent: "focus-restore", "key-path": "page:1/root/button" },
  ]);
  focus.dispose();
  await Promise.resolve();

  assert.equal(queries, 0);
});

test("disposing surfaces removes its document listener and clears inert state", () => {
  let added: EventListenerOrEventListenerObject | null = null;
  let removed: EventListenerOrEventListenerObject | null = null;
  const ownerDocument = {
    addEventListener(type: string, listener: EventListenerOrEventListenerObject) {
      if (type === "keydown") added = listener;
    },
    removeEventListener(type: string, listener: EventListenerOrEventListenerObject) {
      if (type === "keydown") removed = listener;
    },
  } as unknown as Document;
  let cleared = 0;
  let disposedSubtrees = 0;
  const host = {
    ownerDocument,
    replaceChildren() {
      cleared += 1;
    },
  } as unknown as HTMLElement;
  const pageHost = { inert: true } as HTMLElement;
  const surfaces = createSurfaces({
    host,
    pageHost,
    emit: () => {},
    reconcileChildren: () => {},
    disposeSubtree: () => {
      disposedSubtrees += 1;
    },
    enterSurface: () => {},
  });

  surfaces.dispose();

  assert.ok(added);
  assert.equal(removed, added);
  assert.equal(cleared, 1);
  assert.equal(disposedSubtrees, 1);
  assert.equal(pageHost.inert, false);
});
