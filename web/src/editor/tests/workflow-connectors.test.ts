import assert from "node:assert/strict";

import { test } from "vitest";

import type { EditorPreview } from "../editor-state.js";
import {
  buildWorkflowConnectors,
  routeWorkflowConnector,
  workflowRailHeight,
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
    sourcePort: { slot: 0, count: 1 },
    targetPort: { slot: 0, count: 1 },
  }, {
    groupId: "page/feed",
    sourceId: "page/feed/pending",
    targetId: "page/feed/refused",
    steps: ["like-post.err"],
    openedSurfaces: [],
    lane: 1,
    sourcePort: { slot: 0, count: 1 },
    targetPort: { slot: 0, count: 1 },
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
  assert.deepEqual(connectors.map((connector) => connector.sourcePort), [
    { slot: 2, count: 3 },
    { slot: 1, count: 3 },
    { slot: 0, count: 1 },
    { slot: 0, count: 3 },
  ]);
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

test("fans sibling edges across ports and routes above intervening frames", () => {
  const connectors = buildWorkflowConnectors("page/feed", [
    preview("base"),
    preview("first", "base", ["first"]),
    preview("second", "base", ["second"]),
    preview("third", "base", ["third"]),
  ]);
  const source = { x: 0, y: 100, width: 100, height: 200 };
  const targets = [
    { x: 300, y: 100, width: 100, height: 200 },
    { x: 500, y: 100, width: 100, height: 200 },
    { x: 700, y: 100, width: 100, height: 200 },
  ];
  const origins = connectors.map((connector, index) =>
    routeWorkflowConnector(connector, source, targets[index]!, [
      source,
      { x: 140, y: 80, width: 120, height: 220 },
      targets[index]!,
    ]));

  assert.deepEqual(origins.map((route) => route.origin.x), [75, 50, 25]);
  assert.deepEqual(origins.map((route) => route.railY), [62, 42, 22]);
  assert.equal(
    origins[0]?.path,
    "M 75 100 L 75 62 L 350 62 L 350 100",
  );
  assert.equal(origins[0]?.label.y, 56);
  assert.equal(workflowRailHeight(3), 88);
  assert.equal(workflowRailHeight(0), 0);
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
