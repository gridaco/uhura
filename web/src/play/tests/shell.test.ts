import assert from "node:assert/strict";
import { test } from "vitest";

import { PLAY_SHELL_MARKUP } from "../shell.js";
import { PLAY_ARTIFACT_URLS } from "../main.js";

test("the route-built Play shell owns every runtime host and Editor navigation", () => {
  const requiredIds = [
    "uh-shell-toolbar",
    "uh-runtime-status",
    "uh-provider-control",
    "uh-provider-select",
    "uh-actor-select",
    "uh-fullscreen",
    "uh-restart",
    "uh-stage",
    "uh-frame-label",
    "uh-frame-sizer",
    "uh-frame",
    "uh-page",
    "uh-surfaces",
    "uh-overlay",
  ];
  for (const id of requiredIds) {
    const occurrences = PLAY_SHELL_MARKUP.match(new RegExp(`id="${id}"`, "g"));
    assert.equal(occurrences?.length, 1, `${id} appears exactly once`);
  }
  assert.match(PLAY_SHELL_MARKUP, /class="uh-editor-link" href="\/"/);
  assert.doesNotMatch(PLAY_SHELL_MARKUP, /<script\b/i);
});

test("Play fetches one namespaced coherent artifact set including app CSS", () => {
  assert.deepEqual(PLAY_ARTIFACT_URLS, [
    "/api/play/ir.json",
    "/api/play/boot.json",
    "/api/play/fixture.json",
    "/api/play/script.json",
    "/api/play/icons.json",
    "/api/play/config.json",
    "/api/play/stylesheet.css",
  ]);
});
