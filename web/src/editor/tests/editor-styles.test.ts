import assert from "node:assert/strict";
import { test } from "vitest";

import { scopePreviewSelector } from "../editor-styles.js";

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
