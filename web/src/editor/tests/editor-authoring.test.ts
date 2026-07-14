import assert from "node:assert/strict";
import { test } from "vitest";

import type {
  EditorPreview,
  EditorRender,
  EditorSourceSpan,
  SourceMetadataEntry,
  SourceTarget,
} from "../editor-state.js";
import {
  annotationRenderStatus,
  documentationForPreview,
  memberDocumentationForPreview,
  prepareAuthoring,
  presentedSourceGroups,
  presentedSourceTargets,
  renderedOccurrences,
  sourceActionsEnabled,
} from "../editor-authoring.js";

const span = (offset: number): EditorSourceSpan => ({
  offset,
  len: 4,
  start: { line: offset + 1, col: 1 },
  end: { line: offset + 1, col: 5 },
});

const target = (
  id: string,
  file: string,
  offset: number,
  targetClass: SourceTarget["class"],
  owner: SourceTarget["owner"] = { kind: "component", name: "card" },
): SourceTarget => ({
  id,
  class: targetClass,
  file,
  span: span(offset),
  label: id,
  owner,
});

const entry = (
  id: string,
  targetId: string,
  metadataClass: SourceMetadataEntry["class"],
  order: number,
): SourceMetadataEntry => ({
  id,
  class: metadataClass,
  kind: metadataClass === "doc" ? "doc" : id,
  text: `${id} text`,
  span: span(order),
  targetId,
  order,
});

const preview = (entries: { id: string; targetId: string; anchored: boolean }[]): EditorPreview => ({
  id: "preview",
  identity: { kind: "component", subject: "card", example: "default" },
  sourceFile: "components/card.uhura",
  default: true,
  pinned: false,
  derived: false,
  inFlight: 0,
  from: null,
  replaySteps: [],
  note: null,
  data: [],
  interactions: [],
  documentation: { declarationDocId: "doc", exampleDocId: null },
  provenance: {
    occurrences: entries.map((item) => ({
      id: item.id,
      targetId: item.targetId,
      anchors: item.anchored ? [{ root: { kind: "fragment" }, path: [] }] : [],
    })),
  },
  content: { key: "root", element: "view", props: {} },
});

const render = (): EditorRender => {
  const targets = [
    target("annotation", "z.uhura", 5, "catalog-element"),
    target(
      "declaration",
      "a.uhura",
      9,
      "page-declaration",
      { kind: "page", name: "home" },
    ),
    target("both", "a.uhura", 2, "component-invocation"),
  ];
  const entries = [
    entry("second", "both", "annotation", 2),
    entry("doc", "declaration", "doc", 0),
    entry("first", "both", "annotation", 1),
    entry("note", "annotation", "annotation", 0),
  ];
  return {
    revision: 3,
    freshness: "current",
    application: { name: "Example" },
    authoring: { targets, entries },
    groups: [],
    previews: [preview([
      { id: "visible", targetId: "both", anchored: true },
      { id: "empty", targetId: "both", anchored: false },
      { id: "only-empty", targetId: "annotation", anchored: false },
    ])],
    stylesheet: "",
    icons: {},
    assets: {},
  };
};

test("builds ordered doc, annotation, and occurrence indexes", () => {
  const prepared = prepareAuthoring(render());
  assert.deepEqual(
    prepared.annotationTargets.map((item) => item.target.id),
    ["both", "annotation"],
  );
  assert.deepEqual(
    prepared.entriesByTarget.get("both")?.map((item) => item.id),
    ["first", "second"],
  );
  assert.deepEqual(
    presentedSourceTargets(prepared).map((item) => item.id),
    ["both", "declaration", "annotation"],
    "docs and annotations share one file/span ordering",
  );
  assert.deepEqual(
    presentedSourceGroups(prepared).map((group) => ({
      owner: `${group.owner.kind}:${group.owner.name}`,
      targets: group.targets.map((item) => item.id),
    })),
    [
      { owner: "component:card", targets: ["both", "annotation"] },
      { owner: "page:home", targets: ["declaration"] },
    ],
  );
  assert.equal(
    documentationForPreview(prepared, render().previews[0]!).declaration?.id,
    "doc",
  );
});

test("reports partial and zero-anchor realization without inventing canvas nodes", () => {
  const prepared = prepareAuthoring(render());
  assert.equal(annotationRenderStatus(prepared.occurrencesByTarget.get("both") ?? []), "1 of 2 rendered");
  assert.equal(
    annotationRenderStatus(prepared.occurrencesByTarget.get("annotation") ?? []),
    "Not rendered in any preview",
  );
  assert.deepEqual(
    renderedOccurrences(prepared.annotationTargets[0]!).map((item) => item.occurrence.id),
    ["visible"],
    "Canvas instance navigation excludes zero-anchor occurrences",
  );
  assert.equal(sourceActionsEnabled(render()), true);
  assert.equal(sourceActionsEnabled({ ...render(), freshness: "stale" }), false);
});

test("presents selected declaration docs before source-ordered owning member docs", () => {
  const value = render();
  value.authoring.targets.push(
    target("component", "a.uhura", 0, "component-declaration"),
    target("later-member", "b.uhura", 8, "state-field"),
    target("first-member", "a.uhura", 4, "prop-declaration"),
    target("other-member", "a.uhura", 2, "prop-declaration", { kind: "page", name: "home" }),
  );
  value.authoring.entries.push(
    entry("component-doc", "component", "doc", 0),
    entry("later-doc", "later-member", "doc", 0),
    entry("first-doc", "first-member", "doc", 0),
    entry("other-doc", "other-member", "doc", 0),
  );
  value.previews[0]!.documentation.declarationDocId = "component-doc";

  const prepared = prepareAuthoring(value);
  assert.equal(documentationForPreview(prepared, value.previews[0]!).declaration?.id, "component-doc");
  assert.deepEqual(
    memberDocumentationForPreview(prepared, value.previews[0]!).map((item) => ({
      target: item.target.id,
      docs: item.entries.map((entryValue) => entryValue.id),
    })),
    [
      { target: "first-member", docs: ["first-doc"] },
      { target: "later-member", docs: ["later-doc"] },
    ],
  );
});
