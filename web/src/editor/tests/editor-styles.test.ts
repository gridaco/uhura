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
