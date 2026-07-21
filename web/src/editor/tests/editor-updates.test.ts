import assert from "node:assert/strict";

import { test } from "vitest";

import type {
  EditorPreview,
  EditorRender,
  EditorRevisionEvent,
  EditorState,
} from "../editor-state.js";
import {
  EditorUpdateSession,
  retainPreviewSelection,
  reusablePreviewFrameIds,
  reusablePreviewIds,
} from "../editor-updates.js";
import { elementNode, projectionContent, textNode } from "./fixtures/projection.js";

const state = (sourceRevision: number): EditorState => ({
  protocol: "uhura-editor-state/5",
  sourceRevision,
  diagnostics: null,
  render: null,
});

const event = (sourceRevision: number): EditorRevisionEvent => ({
  protocol: "uhura-editor-event/0",
  sourceRevision,
});

const preview = (id: string, content = id): EditorPreview => ({
  id,
  identity: { kind: "component", subject: id, example: "default" },
  sourceFile: `components/${id}.uhura`,
  default: true,
  pinned: false,
  derived: false,
  inFlight: 0,
  from: null,
  replaySteps: [],
  replay: [],
  note: null,
  data: [],
  interactions: [],
  documentation: { declarationDocId: null, exampleDocId: null },
  provenance: { occurrences: [] },
  evidence: null,
  content: projectionContent([
    elementNode("root", [textNode("content", content)]),
  ]),
});

const render = (
  revision: number,
  previews: EditorPreview[] = [preview("alpha"), preview("beta")],
): EditorRender => ({
  revision,
  freshness: "current",
  application: { name: "Example" },
  authoring: { targets: [], entries: [] },
  groups: [{
    id: "component/examples",
    kind: "component",
    subject: "examples",
    previews: previews.map((item) => item.id),
  }],
  previews,
  stylesheet: ":root { --accent: blue; }",
  assets: {
    photo: { dataUri: "data:image/png;base64,AA==", alt: "Photo" },
  },
  interactionGraph: { protocol: "uhura-interaction-graph/0", nodes: [], edges: [] },
  machine: null,
});

test("every connection open fetches, including equal counters after a restart", () => {
  const updates = new EditorUpdateSession();
  const initial = updates.opened();
  const initialState = state(2);
  assert.equal(updates.consider(initial, initialState).kind, "prepare");
  assert.equal(updates.commit(initial, initialState), true);

  const reopened = updates.opened();
  assert.notEqual(reopened, initial);
  assert.equal(updates.consider(reopened, state(2)).kind, "prepare");
});

test("events deduplicate the active and already requested revision", () => {
  const updates = new EditorUpdateSession();
  const initial = updates.opened();
  assert.equal(updates.commit(initial, state(4)), true);
  assert.equal(updates.announced(event(4)), null);

  const next = updates.announced(event(5));
  assert.ok(next);
  assert.equal(updates.announced(event(5)), null);
});

test("an older overlapping response cannot replace a newer request", () => {
  const updates = new EditorUpdateSession();
  const older = updates.opened();
  const newer = updates.announced(event(8));
  assert.ok(newer);

  assert.deepEqual(updates.consider(older, state(7)), { kind: "ignored" });
  assert.equal(updates.commit(older, state(7)), false);
  assert.equal(updates.consider(newer, state(8)).kind, "prepare");
  assert.equal(updates.commit(newer, state(8)), true);
  assert.equal(updates.activeRevision, 8);
});

test("a response behind its announcement retries only while still current", () => {
  const updates = new EditorUpdateSession();
  const expected = updates.announced(event(6));
  assert.ok(expected);
  assert.deepEqual(updates.consider(expected, state(5)), {
    kind: "behind",
    expectedRevision: 6,
    receivedRevision: 5,
  });

  const retry = updates.retry(expected, 6);
  assert.ok(retry);
  assert.equal(updates.retry(expected, 6), null);
  assert.equal(updates.consider(retry, state(6)).kind, "prepare");
});

test("a failed install keeps the revision retryable and unpublished", () => {
  const updates = new EditorUpdateSession();
  const token = updates.opened();
  const nextState = state(9);

  assert.throws(
    () => updates.commit(token, nextState, () => {
      throw new Error("detached board install failed");
    }),
    /install failed/,
  );
  assert.equal(updates.activeRevision, null);
  assert.equal(updates.isCurrent(token), true);

  let installed = false;
  assert.equal(updates.commit(token, nextState, () => { installed = true; }), true);
  assert.equal(installed, true);
  assert.equal(updates.activeRevision, 9);
});

test("semantic selection survives replacement and disappears with its preview", () => {
  const selection = { kind: "page", subject: "feed", example: "default" } as const;
  const matching: EditorState = {
    ...state(3),
    render: {
      revision: 3,
      freshness: "current",
      application: { name: "Example" },
      authoring: { targets: [], entries: [] },
      groups: [],
      previews: [{
        id: "new-dom-independent-id",
        identity: selection,
        sourceFile: "app/feed/page.uhura",
        default: true,
        pinned: false,
        derived: false,
        inFlight: 0,
        from: null,
        replaySteps: [],
        replay: [],
        note: null,
        data: [],
        interactions: [],
        documentation: { declarationDocId: null, exampleDocId: null },
        provenance: { occurrences: [] },
        evidence: null,
        content: projectionContent(),
      }],
      stylesheet: "",
      assets: {},
      interactionGraph: { protocol: "uhura-interaction-graph/0", nodes: [], edges: [] },
      machine: null,
    },
  };

  assert.deepEqual(retainPreviewSelection(selection, matching), selection);
  assert.equal(retainPreviewSelection(
    { kind: "page", subject: "profile", example: "private" },
    matching,
  ), null);
});

test("reuses only structurally unchanged previews with compatible resources", () => {
  const previous = render(3);
  const next = render(4, [preview("alpha"), preview("beta", "changed"), preview("gamma")]);

  assert.deepEqual([...reusablePreviewIds(previous, next)], ["alpha"]);
});

test("stylesheet changes retain semantic frames", () => {
  const previous = render(3);
  const next = { ...render(4), stylesheet: "body { color: rebeccapurple; }" };

  assert.deepEqual([...reusablePreviewIds(previous, next)], ["alpha", "beta"]);
});

test("authoring-only changes reuse semantic DOM", () => {
  const previous = render(3);
  const next = structuredClone(render(4));
  next.authoring.targets.push({
    id: "target",
    class: "ui-element",
    file: "card.uhura",
    span: {
      offset: 10,
      len: 6,
      start: { line: 2, col: 3 },
      end: { line: 2, col: 9 },
    },
    label: "button",
    owner: { kind: "component", name: "card" },
  });
  next.previews[0]!.provenance.occurrences.push({
    id: "occurrence",
    targetId: "target",
    anchors: ["root"],
  });

  assert.deepEqual([...reusablePreviewIds(previous, next)], ["alpha", "beta"]);
  assert.deepEqual([...reusablePreviewFrameIds(previous, next)], ["alpha", "beta"]);
});

test("caption changes replace frame chrome but retain semantic realization", () => {
  const previous = render(3);
  const next = structuredClone(render(4));
  next.previews[0]!.note = "Updated caption";

  assert.deepEqual([...reusablePreviewIds(previous, next)], ["alpha", "beta"]);
  assert.deepEqual([...reusablePreviewFrameIds(previous, next)], ["beta"]);
});

test("asset changes conservatively invalidate every realized frame", () => {
  const previous = render(3);
  const changedAssets = structuredClone(render(4));
  changedAssets.assets["photo"]!.dataUri = "data:image/png;base64,BB==";

  assert.deepEqual([...reusablePreviewIds(previous, changedAssets)], []);
});

test("stale transitions retain semantic frames", () => {
  const previous = render(3);
  const next = { ...structuredClone(previous), freshness: "stale" as const };

  assert.deepEqual([...reusablePreviewIds(previous, next)], ["alpha", "beta"]);
});

test("equal revisions never hide changed preview content", () => {
  const previous = render(1);
  const restartedHost = render(1, [preview("alpha", "different"), preview("beta")]);

  assert.deepEqual([...reusablePreviewIds(previous, restartedHost)], ["beta"]);
});

test("group order changes retain semantic frame identities", () => {
  const previous = render(3);
  const next = structuredClone(render(4));
  next.groups[0]!.previews.reverse();

  assert.deepEqual([...reusablePreviewIds(previous, next)], ["alpha", "beta"]);
});
