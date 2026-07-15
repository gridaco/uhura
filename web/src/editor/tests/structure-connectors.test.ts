import assert from "node:assert/strict";

import { test } from "vitest";

import type { InteractionGraph, InteractionGraphEdge } from "../../protocol/types.js";
import type { EditorPreview, PreviewKind } from "../editor-state.js";
import {
  buildStructureConnectors,
  layoutStructureConnectors,
  structureConnectorDescription,
  structureConnectorLabel,
  visibleStructureConnectors,
} from "../structure-connectors.js";

const preview = (
  kind: PreviewKind,
  subject: string,
  example: string,
): EditorPreview => ({
  id: `${kind}/${subject}/${example}`,
  identity: { kind, subject, example },
  sourceFile: `pages/${subject}.uhura`,
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
  content: kind === "page"
    ? {
      protocol: "uhura-view/0",
      revision: 0,
      page: {
        route: subject,
        root: { key: "root", element: "view", props: {} },
      },
      surfaces: [],
    }
    : { key: "root", element: "view", props: {} },
});

const graph = (edges: InteractionGraphEdge[]): InteractionGraph => ({
  protocol: "uhura-interaction-graph/0",
  nodes: [
    { id: "page:feed", kind: "page", label: "feed" },
    { id: "page:profile", kind: "page", label: "profile" },
    { id: "surface:comments-sheet", kind: "surface", label: "comments-sheet" },
    { id: "command:posts.like", kind: "command", label: "posts.like" },
    { id: "dynamic:opener", kind: "dynamic", label: "surface opener" },
  ],
  edges,
});

const boardPreviews = [
  preview("page", "feed", "base"),
  preview("page", "feed", "extra"),
  preview("page", "profile", "default"),
  preview("surface", "comments-sheet", "open"),
];

const frameIndex = new Map(
  boardPreviews.map((preview, index) => [preview.id, index] as const),
);

test("builds only navigate and present candidates between mapped frames", () => {
  const connectors = buildStructureConnectors(graph([
    { kind: "navigate", from: "page:feed", to: "page:profile", event: "profile-opened" },
    { kind: "present", from: "page:feed", to: "surface:comments-sheet", event: "comments-requested" },
    { kind: "state-change", from: "page:feed", to: "page:feed", event: "liked" },
    { kind: "send-command", from: "page:feed", to: "command:posts.like", event: "liked" },
    { kind: "receive-outcome", from: "command:posts.like", to: "page:feed", event: "like.ok" },
    { kind: "dismiss", from: "surface:comments-sheet", to: "dynamic:opener", event: "dismiss-requested" },
    { kind: "navigate-back", from: "page:profile", to: "dynamic:previous-page", event: "back-requested" },
  ]), boardPreviews);

  assert.deepEqual(connectors, [{
    kind: "navigate",
    sourceNode: "page:feed",
    targetNode: "page:profile",
    sourceId: "page/feed/base",
    targetId: "page/profile/default",
    event: "profile-opened",
    extraCount: 0,
    lane: 0,
    sourcePort: { slot: 0, count: 1 },
    targetPort: { slot: 0, count: 1 },
  }, {
    kind: "present",
    sourceNode: "page:feed",
    targetNode: "surface:comments-sheet",
    sourceId: "page/feed/base",
    targetId: "surface/comments-sheet/open",
    event: "comments-requested",
    extraCount: 0,
    lane: 0,
    sourcePort: { slot: 0, count: 1 },
    targetPort: { slot: 0, count: 1 },
  }]);
});

test("maps a node to the subject's first frame and skips unmapped or self edges", () => {
  const connectors = buildStructureConnectors(graph([
    { kind: "navigate", from: "page:profile", to: "page:feed", event: "home-requested" },
    { kind: "navigate", from: "page:feed", to: "page:feed", event: "self-refresh" },
    { kind: "navigate", from: "page:feed", to: "page:missing", event: "gone" },
    { kind: "present", from: "page:missing", to: "surface:comments-sheet", event: "gone" },
  ]), boardPreviews);

  assert.deepEqual(
    connectors.map(({ sourceId, targetId }) => [sourceId, targetId]),
    [["page/profile/default", "page/feed/base"]],
    "the first feed example is the frame; missing and self edges draw nothing",
  );
});

test("dedupes edges sharing endpoints and kind into one labeled connector", () => {
  const connectors = buildStructureConnectors(graph([
    { kind: "navigate", from: "page:feed", to: "page:profile", event: "b-event" },
    { kind: "present", from: "page:feed", to: "surface:comments-sheet", event: "open-comments" },
    { kind: "navigate", from: "page:feed", to: "page:profile", event: "a-event" },
    { kind: "navigate", from: "page:feed", to: "page:profile", event: "c-event" },
  ]), boardPreviews);

  assert.equal(connectors.length, 2);
  assert.equal(connectors[0]?.event, "a-event");
  assert.equal(connectors[0]?.extraCount, 2);
  assert.equal(structureConnectorLabel(connectors[0]!), "a-event +2");
  assert.equal(structureConnectorLabel(connectors[1]!), "open-comments");
  assert.equal(
    structureConnectorDescription(connectors[0]!),
    "navigates on a-event (+2 more)",
  );
  assert.equal(
    structureConnectorDescription(connectors[1]!),
    "presents on open-comments",
  );
});

test("orders candidates deterministically regardless of edge input order", () => {
  const edges: InteractionGraphEdge[] = [
    { kind: "navigate", from: "page:feed", to: "page:profile", event: "profile-opened" },
    { kind: "present", from: "page:feed", to: "surface:comments-sheet", event: "comments-requested" },
    { kind: "navigate", from: "page:profile", to: "page:feed", event: "home-requested" },
  ];

  assert.deepEqual(
    buildStructureConnectors(graph(edges), boardPreviews),
    buildStructureConnectors(graph([...edges].reverse()), boardPreviews),
  );
});

const fullGraph = graph([
  { kind: "navigate", from: "page:feed", to: "page:profile", event: "profile-opened" },
  { kind: "present", from: "page:feed", to: "surface:comments-sheet", event: "comments-requested" },
  { kind: "navigate", from: "page:profile", to: "page:feed", event: "home-requested" },
  { kind: "present", from: "page:profile", to: "surface:comments-sheet", event: "profile-comments" },
]);

test("scopes visibility to the selected definition, incoming and outgoing", () => {
  const all = buildStructureConnectors(fullGraph, boardPreviews);

  const feed = visibleStructureConnectors(all, { kind: "page", subject: "feed" });
  assert.deepEqual(
    feed.map(({ sourceNode, targetNode }) => [sourceNode, targetNode]),
    [
      ["page:feed", "page:profile"],
      ["page:feed", "surface:comments-sheet"],
      ["page:profile", "page:feed"],
    ],
    "feed keeps its outgoing arrows and the incoming one from profile",
  );

  const sheet = visibleStructureConnectors(all, {
    kind: "surface",
    subject: "comments-sheet",
  });
  assert.deepEqual(
    sheet.map(({ sourceNode, targetNode }) => [sourceNode, targetNode]),
    [
      ["page:feed", "surface:comments-sheet"],
      ["page:profile", "surface:comments-sheet"],
    ],
  );
});

test("selection scoping matches kind and subject, never subject alone", () => {
  const all = buildStructureConnectors(fullGraph, boardPreviews);
  assert.deepEqual(
    visibleStructureConnectors(all, { kind: "surface", subject: "feed" }),
    [],
    "a surface named like a page must not adopt the page's arrows",
  );
  assert.deepEqual(
    visibleStructureConnectors(all, { kind: "component", subject: "feed" }),
    [],
  );
});

test("empty or unrelated selection hides every structural connector", () => {
  const all = buildStructureConnectors(fullGraph, boardPreviews);
  assert.deepEqual(visibleStructureConnectors(all, null), []);
  assert.deepEqual(
    visibleStructureConnectors(all, { kind: "page", subject: "settings" }),
    [],
  );
});

test("packs lanes over the visible subset only, above the replay lanes", () => {
  const all = buildStructureConnectors(fullGraph, boardPreviews);
  const feed = layoutStructureConnectors(
    visibleStructureConnectors(all, { kind: "page", subject: "feed" }),
    frameIndex,
    3,
  );

  // All three visible spans overlap the feed frame, so each takes its own
  // lane starting right after the three replay lanes.
  assert.deepEqual(feed.map((connector) => connector.lane), [3, 4, 5]);

  const sheet = layoutStructureConnectors(
    visibleStructureConnectors(all, { kind: "surface", subject: "comments-sheet" }),
    frameIndex,
    3,
  );
  assert.deepEqual(
    sheet.map((connector) => connector.lane),
    [3, 4],
    "the hidden connectors never deepen the visible subset's rails",
  );
});

test("fans out ports over the visible subset and keeps inputs unchanged", () => {
  const all = buildStructureConnectors(fullGraph, boardPreviews);
  const visible = visibleStructureConnectors(all, { kind: "page", subject: "feed" });
  const laid = layoutStructureConnectors(visible, frameIndex, 2);

  const outgoing = laid.filter((connector) => connector.sourceNode === "page:feed");
  assert.deepEqual(
    outgoing.map((connector) => connector.sourcePort),
    [{ slot: 1, count: 2 }, { slot: 0, count: 2 }],
    "feed's two visible outgoing arrows share its two source ports",
  );

  assert.ok(laid.every((connector, index) => connector !== visible[index]));
  assert.ok(visible.every((connector) => connector.lane === 0
    && connector.sourcePort.slot === 0
    && connector.sourcePort.count === 1));
});
