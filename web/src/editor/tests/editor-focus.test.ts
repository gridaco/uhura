import assert from "node:assert/strict";

import { test } from "vitest";

import {
  enterPreviewFocus,
  exitPreviewFocus,
  fitPreviewCamera,
  retainPreviewFocus,
} from "../editor-focus.js";
import type {
  EditorPreview,
  EditorRender,
  EditorState,
  PreviewIdentity,
} from "../editor-state.js";

const identity = (
  subject: string,
  example = "default",
): PreviewIdentity => ({ kind: "page", subject, example });

const preview = (id: string, previewIdentity: PreviewIdentity): EditorPreview => ({
  id,
  identity: previewIdentity,
  sourceFile: `app/${previewIdentity.subject}/page.uhura`,
  default: true,
  pinned: false,
  derived: false,
  inFlight: 0,
  from: null,
  replaySteps: [],
  note: null,
  data: [],
  interactions: [],
  documentation: { declarationDocId: null, exampleDocId: null },
  provenance: { occurrences: [] },
  content: { key: "root", element: "view", props: {} },
});

const render = (previews: EditorPreview[]): EditorRender => ({
  revision: 1,
  freshness: "current",
  application: { name: "Example" },
  authoring: { targets: [], entries: [] },
  groups: [],
  previews,
  stylesheet: "",
  icons: {},
  assets: {},
});

const state = (value: EditorRender | null): EditorState => ({
  protocol: "uhura-editor-state/1",
  sourceRevision: 1,
  diagnostics: null,
  render: value,
});

test("entering focus captures the canvas camera by value", () => {
  const camera = { x: 120, y: -40, scale: 0.75 };
  const focus = enterPreviewFocus(null, identity("feed"), camera);

  camera.x = 999;
  assert.deepEqual(focus, {
    identity: identity("feed"),
    restoreCamera: { x: 120, y: -40, scale: 0.75 },
  });
});

test("switching focus never overwrites the camera captured on first entry", () => {
  const initial = enterPreviewFocus(
    null,
    identity("feed"),
    { x: 120, y: -40, scale: 0.75 },
  );
  const switched = enterPreviewFocus(
    initial,
    identity("profile", "private"),
    { x: 0, y: 0, scale: 2 },
  );

  assert.deepEqual(switched, {
    identity: identity("profile", "private"),
    restoreCamera: { x: 120, y: -40, scale: 0.75 },
  });
});

test("focus survives replacement by semantic identity, not preview id", () => {
  const focus = enterPreviewFocus(
    null,
    identity("feed"),
    { x: 4, y: 8, scale: 1 },
  );
  const replacementIdentity = identity("feed");
  const retained = retainPreviewFocus(
    focus,
    state(render([preview("replacement-frame-id", replacementIdentity)])),
  );

  assert.equal(retained?.identity, replacementIdentity);
  assert.deepEqual(retained?.restoreCamera, { x: 4, y: 8, scale: 1 });
});

test("focus can be retained directly against an EditorRender", () => {
  const focus = enterPreviewFocus(
    null,
    identity("feed"),
    { x: 4, y: 8, scale: 1 },
  );

  assert.deepEqual(
    retainPreviewFocus(focus, render([preview("feed-v2", identity("feed"))]))?.identity,
    identity("feed"),
  );
});

test("focus is dropped when its preview or render disappears", () => {
  const focus = enterPreviewFocus(
    null,
    identity("feed"),
    { x: 4, y: 8, scale: 1 },
  );

  assert.equal(retainPreviewFocus(focus, render([preview("profile", identity("profile"))])), null);
  assert.equal(retainPreviewFocus(focus, state(null)), null);
  assert.equal(retainPreviewFocus(focus, null), null);
  assert.equal(retainPreviewFocus(null, render([])), null);
});

test("exiting returns an independent restore snapshot", () => {
  const focus = enterPreviewFocus(
    null,
    identity("feed"),
    { x: 4, y: 8, scale: 1 },
  );
  const camera = exitPreviewFocus(focus);

  assert.deepEqual(camera, { x: 4, y: 8, scale: 1 });
  assert.notEqual(camera, focus.restoreCamera);
  assert.equal(exitPreviewFocus(null), null);
});

test("fits and centers a preview within the padded viewport", () => {
  assert.deepEqual(
    fitPreviewCamera(
      { x: 100, y: 50, width: 400, height: 200 },
      1_000,
      700,
      100,
      0.25,
      2,
    ),
    { x: -100, y: 50, scale: 2 },
  );
});

test("fit camera clamps to the configured scale range", () => {
  assert.deepEqual(
    fitPreviewCamera(
      { x: 0, y: 0, width: 100, height: 100 },
      1_000,
      1_000,
      0,
      0.25,
      1.5,
    ),
    { x: 425, y: 425, scale: 1.5 },
  );
  assert.deepEqual(
    fitPreviewCamera(
      { x: 100, y: 100, width: 1_000, height: 1_000 },
      100,
      100,
      0,
      0.25,
      2,
    ),
    { x: -100, y: -100, scale: 0.25 },
  );
});

test("zero-sized fit inputs deterministically use minimum scale", () => {
  assert.deepEqual(
    fitPreviewCamera(
      { x: 20, y: 30, width: 0, height: 0 },
      200,
      100,
      20,
      0.25,
      2,
    ),
    { x: 95, y: 42.5, scale: 0.25 },
  );
  assert.deepEqual(
    fitPreviewCamera(
      { x: 20, y: 30, width: 100, height: 50 },
      0,
      0,
      0,
      0.25,
      2,
    ),
    { x: -17.5, y: -13.75, scale: 0.25 },
  );
});

test("fit camera rejects invalid geometry and scale bounds", () => {
  const rect = { x: 0, y: 0, width: 100, height: 100 };

  assert.throws(() => fitPreviewCamera(
    { ...rect, width: -1 },
    100,
    100,
    0,
    0.25,
    2,
  ), /rect\.width/);
  assert.throws(() => fitPreviewCamera(
    rect,
    100,
    100,
    0,
    2,
    1,
  ), /maxScale/);
});
