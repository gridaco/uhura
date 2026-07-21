import assert from "node:assert/strict";

import { test } from "vitest";

import type { RenderNode } from "../../renderer/projection.js";
import type { EditorPreview } from "../editor-state.js";
import { introducedSurfaces, surfaceHierarchy } from "../surface-hierarchy.js";
import { elementNode, projectionContent } from "./fixtures/projection.js";

const preview = (
  example: string,
  nodes: readonly RenderNode[],
  from: string | null = null,
): EditorPreview => ({
  id: `page/feed/${example}`,
  identity: { kind: "page", subject: "feed", example },
  sourceFile: "web.uhura",
  default: from === null,
  pinned: false,
  derived: from !== null,
  inFlight: 0,
  from,
  replaySteps: [],
  replay: [],
  note: null,
  data: [],
  interactions: [],
  documentation: { declarationDocId: null, exampleDocId: null },
  provenance: { occurrences: [] },
  evidence: null,
  content: projectionContent(nodes, "instagram@1::Feed"),
});

const comments = (children: readonly RenderNode[] = []): RenderNode =>
  elementNode("comments-sheet", children, {
    element: "dialog",
    surface: true,
    attributes: [{ name: "aria-label", value: "Comments" }],
  });

test("derives mounted surfaces and readable labels from canonical projection nodes", () => {
  const current = preview("comments", [
    elementNode("root", [comments()]),
  ]);

  assert.deepEqual(surfaceHierarchy(current), {
    presentation: "instagram@1::Feed",
    surfaces: [{
      key: "comments-sheet",
      definition: "Comments",
      modality: "dialog",
      stackIndex: 0,
      relation: "present",
    }],
    roots: [{
      surface: {
        key: "comments-sheet",
        definition: "Comments",
        modality: "dialog",
        stackIndex: 0,
        relation: "present",
      },
      opener: null,
      children: [],
    }],
  });
});

test("keeps nested hierarchy and compares exact keys with the evidence parent", () => {
  const parent = preview("comments", [elementNode("root", [comments()])]);
  const report = elementNode("report-dialog", [], {
    element: "dialog",
    surface: true,
    attributes: [{ name: "data-modality", value: "alert dialog" }],
  });
  const child = preview(
    "report",
    [elementNode("root", [comments([report])])],
    "comments",
  );

  const hierarchy = surfaceHierarchy(child, [parent, child]);
  assert.deepEqual(
    hierarchy?.surfaces.map(({ key, definition, modality, relation }) => ({
      key,
      definition,
      modality,
      relation,
    })),
    [{
      key: "comments-sheet",
      definition: "Comments",
      modality: "dialog",
      relation: "retained",
    }, {
      key: "report-dialog",
      definition: "Surface 2",
      modality: "alert dialog",
      relation: "introduced",
    }],
  );
  assert.equal(hierarchy?.roots[0]?.children[0]?.opener, "comments-sheet");
  assert.deepEqual(
    introducedSurfaces(child, [parent, child]).map((surface) => surface.key),
    ["report-dialog"],
  );
});

test("returns no hierarchy when a projection has no semantic surfaces", () => {
  assert.equal(surfaceHierarchy(preview("plain", [elementNode("root")])), null);
});
