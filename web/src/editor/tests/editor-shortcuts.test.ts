import assert from "node:assert/strict";
import { test } from "vitest";

import { commentShortcutAction } from "../editor-shortcuts.js";

const key = (overrides: Partial<Parameters<typeof commentShortcutAction>[0]> = {}) => ({
  code: "KeyY",
  repeat: false,
  shiftKey: false,
  metaKey: false,
  ctrlKey: false,
  altKey: false,
  ...overrides,
});

test("Y opens Source and Shift+Y toggles Canvas comments", () => {
  assert.equal(commentShortcutAction(key(), false), "open-source");
  assert.equal(
    commentShortcutAction(key({ shiftKey: true }), false),
    "toggle-canvas-comments",
  );
});

test("comments shortcuts do not hijack text entry, repeats, or modifier chords", () => {
  assert.equal(commentShortcutAction(key(), true), null);
  assert.equal(commentShortcutAction(key({ repeat: true }), false), null);
  assert.equal(commentShortcutAction(key({ metaKey: true }), false), null);
  assert.equal(commentShortcutAction(key({ ctrlKey: true }), false), null);
  assert.equal(commentShortcutAction(key({ altKey: true }), false), null);
  assert.equal(commentShortcutAction(key({ code: "KeyV" }), false), null);
});
