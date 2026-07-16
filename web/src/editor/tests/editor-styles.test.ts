import assert from "node:assert/strict";
import { test } from "vitest";

import { EDITOR_STYLES, scopePreviewSelector } from "../editor-styles.js";

test("maps document roots onto the isolated preview application boundary", () => {
  assert.equal(scopePreviewSelector(":root"), ":host");
  assert.equal(scopePreviewSelector("html body"), ":host #uh-app");
  assert.equal(
    scopePreviewSelector("body, body.preview > .card, :is(body, html) .title"),
    "#uh-app, #uh-app.preview > .card, :is(#uh-app, :host) .title",
  );
});

test("leaves ordinary selectors and root-like substrings alone", () => {
  assert.equal(
    scopePreviewSelector(".somebody .html-card, [data-root='body']"),
    ".somebody .html-card, [data-root='body']",
  );
});

test("annotations-hidden removes replay connectors but never structural arrows", () => {
  assert.ok(
    EDITOR_STYLES.includes(
      ".workflow-connectors.annotations-hidden .workflow-connector { display: none; }",
    ),
    "hiding the annotation layer must hide replay connector groups via CSS",
  );
  assert.ok(
    !EDITOR_STYLES.includes(".annotations-hidden .structure-connector"),
    "structural selection arrows stay outside the annotation visibility toggle",
  );
});

test("the structural draw-in animation respects prefers-reduced-motion", () => {
  const gate = EDITOR_STYLES.indexOf("@media (prefers-reduced-motion: no-preference)");
  assert.ok(gate >= 0, "motion styles must be gated on a reduced-motion query");
  assert.ok(
    EDITOR_STYLES.indexOf("@keyframes structure-draw") > gate
      && EDITOR_STYLES.indexOf("@keyframes structure-fade") > gate,
    "every structure animation keyframe lives inside the motion gate",
  );
  assert.ok(
    !EDITOR_STYLES.slice(0, gate).includes("animation: structure-"),
    "no structure animation applies outside the motion gate",
  );
});

test("spotlight dims only unrelated frames and hover hits stroke and pill only", () => {
  assert.ok(EDITOR_STYLES.includes(
    ".editor-board.is-spotlight .editor-frame:not(.is-selected):not(.is-related)",
  ));
  assert.ok(
    EDITOR_STYLES.includes(".structure-connector.is-active .workflow-connector-path { pointer-events: stroke; }"),
    "hover must hit the drawn stroke, never the group's whole bounding box",
  );
});

test("map mode hides non-map frames and selection styling can never reveal them", () => {
  assert.ok(
    EDITOR_STYLES.includes(
      ".editor-board.is-map-mode .editor-frame:not(.is-map-node) { display: none; }",
    ),
    "map mode must hide every frame that is not a map node",
  );
  // Selection and spotlight only dim or outline frames — neither touches
  // `display`, so a selection carried across the mode flip cannot re-show
  // the example frames the map hid.
  const spotlight = EDITOR_STYLES.match(
    /\.editor-board\.is-spotlight \.editor-frame[^{]*\{([^}]*)\}/,
  )?.[1];
  assert.ok(spotlight !== undefined, "spotlight rule must exist");
  assert.ok(!spotlight.includes("display"), "spotlight must never set display");
});

test("the map toggle carries a visible text label affordance", () => {
  assert.ok(
    EDITOR_STYLES.includes(".canvas-tool.map-toggle"),
    "the map toggle needs label-bearing styles",
  );
  assert.ok(
    EDITOR_STYLES.includes(".map-toggle-label"),
    "the visible Map label must be styled",
  );
});
