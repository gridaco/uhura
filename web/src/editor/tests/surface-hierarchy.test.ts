import assert from "node:assert/strict";

import { test } from "vitest";

import type { EditorPreview, ReplayStep } from "../editor-state.js";
import { directlyOpenedSurfaces, surfaceHierarchy } from "../surface-hierarchy.js";

const replay = (structural: ReplayStep["effects"]["structural"]): ReplayStep => ({
  label: "comments-requested",
  kind: "semantic",
  payload: { post: "post-1" },
  dispatch: null,
  effects: {
    writes: [],
    commands: [],
    intents: [],
    structural,
    projections: [],
  },
});

const page = (steps: ReplayStep[], from: string | null = "first-page"): EditorPreview => ({
  id: "page/feed/comments-open",
  identity: { kind: "page", subject: "feed", example: "comments-open" },
  sourceFile: "pages/feed.uhura",
  default: false,
  pinned: false,
  derived: true,
  inFlight: 0,
  from,
  replaySteps: steps.map((step) => step.label),
  replay: steps,
  note: null,
  data: [],
  interactions: [],
  documentation: { declarationDocId: null, exampleDocId: null },
  provenance: { occurrences: [] },
  content: {
    protocol: "uhura-view/0",
    revision: 2,
    page: { route: "feed", root: { key: "root", element: "view", props: {} } },
    surfaces: [{
      key: "comments-sheet:1",
      definition: "comments-sheet",
      modality: "sheet",
      dismiss: {
        kind: "input",
        event: "dismiss",
        emit: "dismiss",
        scope: "surface:1",
        payload: {},
      },
      root: { key: "surface", element: "view", props: {} },
    }],
  },
});

test("matches a direct open-surface effect to the mounted child by instance key", () => {
  const preview = page([replay([{
    op: "open-surface",
    opener: "page:1",
    surface: "comments-sheet:1",
  }])]);

  assert.deepEqual(surfaceHierarchy(preview), {
    page: "feed",
    surfaces: [{
      key: "comments-sheet:1",
      definition: "comments-sheet",
      modality: "sheet",
      stackIndex: 0,
      relation: "direct",
    }],
    roots: [{
      surface: {
        key: "comments-sheet:1",
        definition: "comments-sheet",
        modality: "sheet",
        stackIndex: 0,
        relation: "direct",
      },
      opener: "page:1",
      children: [],
    }],
  });
  assert.equal(directlyOpenedSurfaces(preview)[0]?.definition, "comments-sheet");
});

test("does not infer direct parentage from a matching definition alone", () => {
  const preview = page([replay([{
    op: "open-surface",
    opener: "page:1",
    surface: "comments-sheet:2",
  }])]);
  assert.equal(surfaceHierarchy(preview)?.surfaces[0]?.relation, "inherited");
  assert.deepEqual(directlyOpenedSurfaces(preview), []);
});

test("keeps parentless snapshot surfaces distinct from inherited replay children", () => {
  const preview = page([], null);
  assert.equal(surfaceHierarchy(preview)?.surfaces[0]?.relation, "mounted");
  assert.deepEqual(directlyOpenedSurfaces(preview), []);
});

test("reconstructs nested surface ancestry from direct replay instance keys", () => {
  const sheet = page([replay([{
    op: "open-surface",
    opener: "page:1",
    surface: "comments-sheet:1",
  }])]);
  const dialog = structuredClone(sheet);
  dialog.id = "page/feed/report-open";
  dialog.identity.example = "report-open";
  dialog.from = "comments-open";
  dialog.replaySteps = ["report-requested"];
  dialog.replay = [replay([{
    op: "open-surface",
    opener: "surface:1",
    surface: "report-dialog:2",
  }])];
  if (!("protocol" in dialog.content)) throw new Error("page fixture");
  dialog.content.surfaces.push({
    key: "report-dialog:2",
    definition: "report-dialog",
    modality: "dialog",
    dismiss: {
      kind: "input", event: "dismiss", emit: "dismiss", scope: "surface:2", payload: {},
    },
    root: { key: "dialog", element: "view", props: {} },
  });

  const hierarchy = surfaceHierarchy(dialog, [sheet, dialog]);
  assert.equal(hierarchy?.roots.length, 1);
  assert.equal(hierarchy?.roots[0]?.surface.definition, "comments-sheet");
  assert.equal(hierarchy?.roots[0]?.surface.relation, "inherited");
  assert.equal(hierarchy?.roots[0]?.opener, "page:1");
  assert.equal(hierarchy?.roots[0]?.children[0]?.surface.definition, "report-dialog");
  assert.equal(hierarchy?.roots[0]?.children[0]?.surface.relation, "direct");
  assert.equal(hierarchy?.roots[0]?.children[0]?.opener, "surface:1");
});
