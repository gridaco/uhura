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
    "uh-debug-toggle",
    "uh-debug-panel",
    "uh-debug-panel-resize",
    "uh-debug-title",
    "uh-debug-close",
    "uh-debug-definition",
    "uh-debug-follow-live",
    "uh-debug-summary",
    "uh-debug-graph",
    "uh-debug-graph-content",
    "uh-debug-zoom-out",
    "uh-debug-zoom-reset",
    "uh-debug-zoom-level",
    "uh-debug-zoom-in",
    "uh-debug-details-resize",
    "uh-debug-details",
    "uh-debug-details-title",
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
  assert.match(
    PLAY_SHELL_MARKUP,
    /id="uh-debug-toggle"[\s\S]*?aria-controls="uh-debug-panel"[\s\S]*?aria-expanded="false"/,
  );
  assert.match(
    PLAY_SHELL_MARKUP,
    /<aside id="uh-debug-panel" hidden aria-labelledby="uh-debug-title">/,
  );
  assert.match(
    PLAY_SHELL_MARKUP,
    /id="uh-debug-close"[\s\S]*?aria-label="Close runtime debugger"/,
  );
  assert.match(
    PLAY_SHELL_MARKUP,
    /id="uh-debug-follow-live"[^>]*aria-pressed="true"/,
  );
  assert.match(
    PLAY_SHELL_MARKUP,
    /id="uh-debug-summary" aria-live="off" aria-atomic="true"/,
  );
  assert.match(
    PLAY_SHELL_MARKUP,
    /id="uh-debug-graph"[\s\S]*?role="region"[\s\S]*?tabindex="0"/,
  );
  assert.match(
    PLAY_SHELL_MARKUP,
    /id="uh-debug-panel-resize"[\s\S]*?role="separator"[\s\S]*?aria-orientation="vertical"/,
  );
  assert.match(
    PLAY_SHELL_MARKUP,
    /id="uh-debug-details-resize"[\s\S]*?role="separator"[\s\S]*?aria-orientation="horizontal"/,
  );
  assert.doesNotMatch(PLAY_SHELL_MARKUP, /Live inspection/);
  assert.ok(
    PLAY_SHELL_MARKUP.indexOf('id="uh-debug-summary"')
      > PLAY_SHELL_MARKUP.indexOf('id="uh-debug-details"'),
    "runtime summary is the debugger footer",
  );
  assert.ok(
    PLAY_SHELL_MARKUP.indexOf('id="uh-debug-panel"')
      > PLAY_SHELL_MARKUP.indexOf("</main>"),
    "debug chrome stays outside the scaled prototype stage",
  );
  assert.doesNotMatch(PLAY_SHELL_MARKUP, /<script\b/i);
});

test("Play fetches one namespaced coherent artifact set including app CSS", () => {
  assert.deepEqual(PLAY_ARTIFACT_URLS, [
    "/api/play/ir.json",
    "/api/play/inspect.json",
    "/api/play/boot.json",
    "/api/play/fixture.json",
    "/api/play/script.json",
    "/api/play/config.json",
    "/api/play/stylesheet.css",
  ]);
});
