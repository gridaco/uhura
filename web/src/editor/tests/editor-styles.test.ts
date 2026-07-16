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
