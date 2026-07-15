import assert from "node:assert/strict";

import { test } from "vitest";

import type { InteractionGraph, InteractionGraphEdge } from "../../protocol/types.js";
import type { EditorPreview, PreviewKind } from "../editor-state.js";
import {
  buildStructureConnectors,
  structureConnectorDescription,
  structureConnectorLabel,
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

test("draws only navigate and present edges between mapped frames", () => {
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
    sourceId: "page/feed/base",
    targetId: "page/profile/default",
    event: "profile-opened",
    extraCount: 0,
    lane: 0,
    sourcePort: { slot: 1, count: 2 },
    targetPort: { slot: 0, count: 1 },
  }, {
    kind: "present",
    sourceId: "page/feed/base",
    targetId: "surface/comments-sheet/open",
    event: "comments-requested",
    extraCount: 0,
    lane: 1,
    sourcePort: { slot: 0, count: 2 },
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

test("orders connectors deterministically regardless of edge input order", () => {
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

test("stacks structural lanes above the replay lanes via the lane offset", () => {
  const connectors = buildStructureConnectors(graph([
    { kind: "navigate", from: "page:feed", to: "page:profile", event: "profile-opened" },
    { kind: "present", from: "page:feed", to: "surface:comments-sheet", event: "comments-requested" },
  ]), boardPreviews, 3);

  assert.deepEqual(connectors.map((connector) => connector.lane), [3, 4]);
});
