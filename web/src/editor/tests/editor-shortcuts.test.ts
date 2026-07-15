import assert from "node:assert/strict";
import { test } from "vitest";

import { sourceShortcutAction } from "../editor-shortcuts.js";

const key = (overrides: Partial<Parameters<typeof sourceShortcutAction>[0]> = {}) => ({
  code: "KeyY",
  repeat: false,
  shiftKey: false,
  metaKey: false,
  ctrlKey: false,
  altKey: false,
  ...overrides,
});

test("Y opens Source and Shift+Y toggles workflow connectors", () => {
  assert.equal(sourceShortcutAction(key(), false), "open-source");
  assert.equal(
    sourceShortcutAction(key({ shiftKey: true }), false),
    "toggle-workflow-connectors",
  );
});

test("Source shortcuts do not hijack text entry, repeats, or modifier chords", () => {
  assert.equal(sourceShortcutAction(key(), true), null);
  assert.equal(sourceShortcutAction(key({ repeat: true }), false), null);
  assert.equal(sourceShortcutAction(key({ metaKey: true }), false), null);
  assert.equal(sourceShortcutAction(key({ ctrlKey: true }), false), null);
  assert.equal(sourceShortcutAction(key({ altKey: true }), false), null);
  assert.equal(sourceShortcutAction(key({ code: "KeyV" }), false), null);
});
