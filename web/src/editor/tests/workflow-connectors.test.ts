import assert from "node:assert/strict";

import { test } from "vitest";

import type { EditorPreview } from "../editor-state.js";
import {
  buildWorkflowConnectors,
  workflowConnectorDescription,
  workflowConnectorLabel,
} from "../workflow-connectors.js";

const preview = (
  example: string,
  from: string | null = null,
  replaySteps: string[] = [],
): EditorPreview => ({
  id: `page/feed/${example}`,
  identity: { kind: "page", subject: "feed", example },
  sourceFile: "pages/feed.uhura",
  default: from === null,
  pinned: false,
  derived: from !== null,
  inFlight: 0,
  from,
  replaySteps,
  replay: [],
  note: null,
  data: [],
  interactions: [],
  documentation: { declarationDocId: null, exampleDocId: null },
  provenance: { occurrences: [] },
  content: {
    protocol: "uhura-view/0",
    revision: 0,
    page: {
      route: "feed",
      root: { key: "root", element: "view", props: {} },
    },
    surfaces: [],
  },
});

test("builds direct checked provenance without repeating ancestor steps", () => {
  const connectors = buildWorkflowConnectors("page/feed", [
    preview("base"),
    preview("pending", "base", ["like-toggled"]),
    preview("refused", "pending", ["like-post.err"]),
  ]);

  assert.deepEqual(connectors, [{
    groupId: "page/feed",
    sourceId: "page/feed/base",
    targetId: "page/feed/pending",
    steps: ["like-toggled"],
    openedSurfaces: [],
    lane: 0,
  }, {
    groupId: "page/feed",
    sourceId: "page/feed/pending",
    targetId: "page/feed/refused",
    steps: ["like-post.err"],
    openedSurfaces: [],
    lane: 1,
  }]);
});

test("allocates deterministic lanes for nested and overlapping intervals", () => {
  const connectors = buildWorkflowConnectors("page/feed", [
    preview("base"),
    preview("first", "base", ["first"]),
    preview("second", "base", ["second"]),
    preview("third", "first", ["third"]),
    preview("fourth", "base", ["fourth"]),
  ]);

  assert.deepEqual(connectors.map((connector) => connector.lane), [0, 1, 2, 3]);
  assert.deepEqual(
    connectors.map(({ sourceId, targetId }) => [sourceId, targetId]),
    [
      ["page/feed/base", "page/feed/first"],
      ["page/feed/base", "page/feed/second"],
      ["page/feed/first", "page/feed/third"],
      ["page/feed/base", "page/feed/fourth"],
    ],
  );
});

test("skips unresolved parents and summarizes labels without hiding full order", () => {
  assert.deepEqual(buildWorkflowConnectors("page/feed", [
    preview("orphan", "missing", ["ignored"]),
  ]), []);
  assert.equal(workflowConnectorLabel([]), "derived");
  assert.equal(workflowConnectorLabel(["saved"]), "saved");
  assert.equal(workflowConnectorLabel(["near-end", "projection feed.page", "load.ok"]), "near-end +2");
  assert.equal(
    workflowConnectorDescription({
      steps: ["near-end", "projection feed.page", "load.ok"],
      openedSurfaces: [],
    }),
    "near-end → projection feed.page → load.ok",
  );
});

test("classifies a checked edge that opens a mounted child surface", () => {
  const child = preview("comments-open", "base", ["comments-requested"]);
  child.replay = [{
    label: "comments-requested",
    kind: "semantic",
    payload: {},
    dispatch: null,
    effects: {
      writes: [], commands: [], intents: [], projections: [],
      structural: [{ op: "open-surface", surface: "comments-sheet:1" }],
    },
  }];
  if (!("protocol" in child.content)) throw new Error("page fixture");
  child.content.surfaces = [{
    key: "comments-sheet:1",
    definition: "comments-sheet",
    modality: "sheet",
    dismiss: {
      kind: "input", event: "dismiss", emit: "dismiss", scope: "surface:1", payload: {},
    },
    root: { key: "surface", element: "view", props: {} },
  }];

  const connector = buildWorkflowConnectors("page/feed", [preview("base"), child])[0]!;
  assert.deepEqual(connector.openedSurfaces.map(({ definition, modality }) => ({
    definition, modality,
  })), [{ definition: "comments-sheet", modality: "sheet" }]);
  assert.equal(
    workflowConnectorLabel(connector.steps, connector.openedSurfaces),
    "comments-requested · opens comments-sheet",
  );
  assert.equal(
    workflowConnectorDescription(connector),
    "comments-requested; opens child sheet comments-sheet",
  );
});
