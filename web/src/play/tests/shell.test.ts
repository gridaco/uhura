import assert from "node:assert/strict";
import { test } from "vitest";

import { PLAY_SHELL_MARKUP } from "../shell.js";
import { PLAY_ARTIFACT_URLS } from "../main.js";

test("the route-built Play shell owns every runtime host and Editor navigation", () => {
  const requiredIds = [
    "uh-shell-toolbar",
    "uh-runtime-status",
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
    "uh-app",
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
    "/api/play/config.json",
    "/api/play/icon-fonts.json",
    "/api/play/stylesheet.css",
  ]);
});

import { retargetApplicationStyles } from "../shell.js";

test("retargets document-level authored selectors to :host", () => {
  const authored = [
    ":root {\n  --color-ink: #111;\n}",
    "body {\n  margin: 0;\n  color: var(--color-ink);\n}",
    "#uh-app {\n  font-size: 15px;\n}",
    "#uh-app button.uh-button,\n#uh-app .uh-textfield input { min-block-size: 44px; }",
    ".post-body { padding: 8px; }",
    ".uh-body-text { color: red; }",
  ].join("\n\n");
  const out = retargetApplicationStyles(authored);
  assert.doesNotMatch(out, /:root\s*\{/u);
  assert.doesNotMatch(out, /(^|[},\s])body\s*\{/u);
  assert.doesNotMatch(out, /(^|[\s,{}])#uh-app(?!\)|[\w-])/mu);
  assert.match(out, /:host \{\n {2}--color-ink: #111;/u);
  assert.match(out, /:host\(#uh-app\) button\.uh-button,\n:host\(#uh-app\) \.uh-textfield input/u);
  // 오폭 금지: 클래스 이름 속 body/app 문자열은 건드리지 않는다
  assert.match(out, /\.post-body \{ padding: 8px; \}/u);
  assert.match(out, /\.uh-body-text \{ color: red; \}/u);
});

test("retargets frame-keyed authored selectors to the host", () => {
  const out = retargetApplicationStyles(
    '#uh-frame[data-frame="desktop"] .bottom-nav { inline-size: 240px; }\n#uh-frame .anything { color: red; }',
  );
  assert.match(out, /:host\(#uh-app\[data-frame="desktop"\]\) \.bottom-nav/u);
  assert.match(out, /:host\(#uh-app\) \.anything/u);
  assert.doesNotMatch(out, /#uh-frame/u);
});

test("comments do not shield document-level selectors from retargeting", () => {
  const out = retargetApplicationStyles(
    "/* tokens */\n:root {\n  --nav-rail-width: 244px;\n}\n\n/* base */\nbody { margin: 0; }",
  );
  assert.match(out, /:host \{\n {2}--nav-rail-width: 244px;/u);
  assert.doesNotMatch(out, /:root/u);
  assert.doesNotMatch(out, /(^|[\s,{}])body\s*\{/u);
});
