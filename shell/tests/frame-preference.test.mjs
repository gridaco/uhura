import assert from "node:assert/strict";
import test from "node:test";

import {
  FRAME_PREFERENCE_KEY,
  readFramePreference,
  writeFramePreference,
} from "../frame-preference.js";

function memoryStorage(entries = []) {
  const values = new Map(entries);
  return {
    getItem(key) {
      return values.get(key) ?? null;
    },
    setItem(key, value) {
      values.set(key, String(value));
    },
  };
}

test("persists the Play frame under a shell-owned key", () => {
  const storage = memoryStorage();

  writeFramePreference("desktop", storage);

  assert.equal(storage.getItem(FRAME_PREFERENCE_KEY), "desktop");
  assert.equal(readFramePreference(storage), "desktop");
  assert.equal(FRAME_PREFERENCE_KEY, "uhura:play:frame");
});

test("defaults unknown or absent preferences to mobile", () => {
  assert.equal(readFramePreference(memoryStorage()), "mobile");
  assert.equal(
    readFramePreference(memoryStorage([[FRAME_PREFERENCE_KEY, "tablet"]])),
    "mobile",
  );
});

test("storage failures do not stop Play", () => {
  const unavailable = {
    getItem() {
      throw new Error("storage blocked");
    },
    setItem() {
      throw new Error("storage blocked");
    },
  };

  assert.equal(readFramePreference(unavailable), "mobile");
  assert.doesNotThrow(() => writeFramePreference("desktop", unavailable));
});
