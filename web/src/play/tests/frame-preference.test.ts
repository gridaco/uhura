import assert from "node:assert/strict";
import { test } from "vitest";

import {
  FRAME_PREFERENCE_KEY,
  readFramePreference,
  type FramePreferenceStorage,
  writeFramePreference,
} from "../frame-preference.js";

function memoryStorage(
  entries: readonly (readonly [string, string])[] = [],
): FramePreferenceStorage {
  const values = new Map<string, string>(entries);
  return {
    getItem(key: string) {
      return values.get(key) ?? null;
    },
    setItem(key: string, value: string) {
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
