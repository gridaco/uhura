import assert from "node:assert/strict";
import { test } from "vitest";

import {
  editorIdentifierLabel,
  editorPreviewLabels,
  editorSubjectLabel,
} from "../display-labels.js";
import type { PreviewIdentity } from "../editor-state.js";

const identity = (
  kind: PreviewIdentity["kind"],
  subject: string,
  example: string,
): PreviewIdentity => ({ kind, subject, example });

test("qualified 0.4 page and evidence identities become compact Editor labels", () => {
  const preview = identity(
    "page",
    "app.instagram@1::FeedPage",
    "app.instagram.evidence@1::feed_first_page",
  );

  assert.deepEqual(editorPreviewLabels(preview), {
    subject: "feed",
    example: "first-page",
    combined: "feed / first-page",
  });
  assert.deepEqual(preview, {
    kind: "page",
    subject: "app.instagram@1::FeedPage",
    example: "app.instagram.evidence@1::feed_first_page",
  }, "display derivation never mutates semantic identity");
});

test("identifier labels humanize PascalCase, acronyms, and snake case", () => {
  assert.equal(editorIdentifierLabel("app.example@2::HTTPStatusCard"), "http-status-card");
  assert.equal(editorIdentifierLabel("app.example@2::save_in_flight"), "save-in-flight");
  assert.equal(
    editorSubjectLabel({ kind: "surface", subject: "app.example@2::CommentsSheet" }),
    "comments-sheet",
  );
  assert.equal(
    editorSubjectLabel({ kind: "component", subject: "app.example@2::LandingPage" }),
    "landing-page",
    "Page is semantic for non-page subjects",
  );
});

test("page suffix removal is total for a subject named only Page", () => {
  assert.equal(
    editorSubjectLabel({ kind: "page", subject: "app.example@2::Page" }),
    "page",
  );
});

test("example prefixes are retained when shortening would collide", () => {
  const prefixed = identity(
    "page",
    "app.example@1::FeedPage",
    "app.example.evidence@1::feed_first_page",
  );
  const alreadyShort = identity(
    "page",
    "app.example@1::FeedPage",
    "app.example.evidence@1::first_page",
  );
  const peers = [prefixed, alreadyShort];

  assert.equal(editorPreviewLabels(prefixed, peers).example, "feed-first-page");
  assert.equal(editorPreviewLabels(alreadyShort, peers).example, "first-page");
});

test("only an exact subject-token prefix is removed", () => {
  const preview = identity(
    "page",
    "app.example@1::FeedPage",
    "app.example.evidence@1::feedback_default",
  );
  assert.equal(editorPreviewLabels(preview).example, "feedback-default");
});
