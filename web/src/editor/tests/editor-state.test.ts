import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { test } from "vitest";

import {
  decodeEditorRevisionEvent,
  decodeEditorState,
  EditorContractError,
  type EditorState,
} from "../editor-state.js";

const node = {
  key: "root",
  element: "text",
  props: { content: { t: "plain", v: "Hello" } },
};

const span = (offset: number, len: number, line: number, col: number) => ({
  offset,
  len,
  start: { line, col },
  end: { line, col: col + len },
});

const diagnostics = (message: string): Record<string, unknown> => ({
  format: "uhura-diagnostics",
  version: 0,
  summary: { errors: 1, warnings: 0 },
  diagnostics: [{
    code: "UH9000",
    rule: "editor/test",
    severity: "error",
    message,
  }],
});

const stateFixture = (): unknown => ({
  protocol: "uhura-editor-state/1",
  sourceRevision: 3,
  diagnostics: null,
  render: {
    revision: 3,
    freshness: "current",
    application: { name: "Example" },
    authoring: {
      targets: [{
        id: "target:feed",
        class: "page-declaration",
        file: "pages/feed.uhura",
        span: span(20, 9, 2, 1),
        label: "page feed",
        owner: { kind: "page", name: "feed" },
      }, {
        id: "target:primary-action",
        class: "catalog-element",
        file: "pages/feed.uhura",
        span: span(80, 10, 6, 1),
        label: "button",
        owner: { kind: "page", name: "feed" },
      }, {
        id: "target:feed-example",
        class: "example-declaration",
        file: "pages/feed.examples.uhura",
        span: span(20, 15, 2, 1),
        label: "default",
        owner: { kind: "examples", name: "pages/feed.examples.uhura" },
      }],
      entries: [{
        id: "doc:feed",
        class: "doc",
        kind: "doc",
        text: "The feed page.",
        span: span(0, 18, 1, 1),
        targetId: "target:feed",
        order: 0,
      }, {
        id: "annotation:primary-action:0",
        class: "annotation",
        kind: "doc",
        text: "The primary action.",
        span: span(50, 28, 5, 1),
        targetId: "target:primary-action",
        order: 0,
      }, {
        id: "doc:feed-example",
        class: "doc",
        kind: "doc",
        text: "The default feed example.",
        span: span(0, 18, 1, 1),
        targetId: "target:feed-example",
        order: 0,
      }],
    },
    groups: [{
      id: "page-feed",
      kind: "page",
      subject: "feed",
      previews: ["page-feed-default"],
    }],
    previews: [{
      id: "page-feed-default",
      identity: { kind: "page", subject: "feed", example: "default" },
      default: true,
      pinned: false,
      derived: false,
      inFlight: 0,
      from: null,
      note: null,
      data: [{
        group: "properties",
        name: "title",
        key: null,
        status: "ready",
        value: "Feed",
        source: {
          kind: "fixture",
          declaredIn: "pages/feed.uhura",
          timeline: false,
          fixture: "feed-default",
          path: ["viewer", "feed"],
        },
      }],
      interactions: [{
        nodeKey: "root",
        element: "button",
        kind: "input",
        event: "press",
        emit: "opened",
        scope: "page:1",
        payload: { id: "post-1" },
        carries: { query: "text" },
      }],
      documentation: {
        declarationDocId: "doc:feed",
        exampleDocId: "doc:feed-example",
      },
      provenance: {
        occurrences: [{
          id: "occurrence:primary-action:0",
          targetId: "target:primary-action",
          anchors: [{ root: { kind: "page" }, path: [] }],
        }],
      },
      content: {
        protocol: "uhura-view/0",
        revision: 0,
        page: { route: "feed", root: node },
        surfaces: [],
      },
    }],
    stylesheet: ":root { --accent: blue; }",
    icons: {
      heart: {
        viewBox: [0, 0, 24, 24],
        commands: [{
          kind: "path",
          d: "M12 20L4 12",
          fill: "none",
          stroke: "currentColor",
          strokeWidth: "1.8",
        }],
      },
    },
    assets: {
      avatar: { dataUri: "data:image/png;base64,AA==", alt: "Avatar" },
    },
  },
});

test("decodes the complete fixed EditorState contract", () => {
  const state = decodeEditorState(stateFixture());

  assert.equal(state.protocol, "uhura-editor-state/1");
  assert.equal(state.sourceRevision, 3);
  assert.equal(state.render?.previews[0]?.data[0]?.source?.kind, "fixture");
  assert.deepEqual(state.render?.previews[0]?.interactions[0]?.payload, { id: "post-1" });
  assert.equal(state.render?.icons["heart"]?.commands[0]?.kind, "path");
  assert.equal(state.render?.authoring.entries[1]?.class, "annotation");
  assert.deepEqual(state.render?.previews[0]?.provenance.occurrences[0]?.anchors[0], {
    root: { kind: "page" },
    path: [],
  });
  const preview = state.render?.previews[0];
  const declarationDoc = state.render?.authoring.entries.find((entry) =>
    entry.id === preview?.documentation.declarationDocId);
  const declarationTarget = state.render?.authoring.targets.find((target) =>
    target.id === declarationDoc?.targetId);
  assert.equal(declarationDoc?.class, "doc");
  assert.equal(declarationTarget?.class, "page-declaration");
  assert.equal(declarationTarget?.owner.name, preview?.identity.subject);

  const exampleDoc = state.render?.authoring.entries.find((entry) =>
    entry.id === preview?.documentation.exampleDocId);
  const exampleTarget = state.render?.authoring.targets.find((target) =>
    target.id === exampleDoc?.targetId);
  assert.equal(exampleDoc?.class, "doc");
  assert.equal(exampleTarget?.class, "example-declaration");
  assert.equal(exampleTarget?.label, preview?.identity.example);
});

test("decodes the native model's canonical contract fixture", () => {
  const fixture = JSON.parse(readFileSync(new URL(
    "../../../../crates/uhura-editor-model/tests/fixtures/editor-state.json",
    import.meta.url,
  ), "utf8")) as unknown;

  const state = decodeEditorState(fixture);
  const render = state.render;
  assert.ok(render);
  assert.equal(render.previews.length, 3);
  assert.equal(render.previews[1]?.identity.kind, "surface");
  assert.equal(render.authoring.targets.length, 3);
  assert.equal(render.authoring.entries.length, 3);

  const page = render.previews.find((preview) => preview.id === "page/home/default");
  assert.ok(page);
  const declarationDoc = render.authoring.entries.find((entry) =>
    entry.id === page.documentation.declarationDocId);
  const exampleDoc = render.authoring.entries.find((entry) =>
    entry.id === page.documentation.exampleDocId);
  assert.equal(declarationDoc?.class, "doc");
  assert.equal(exampleDoc?.class, "doc");
  assert.equal(
    render.authoring.targets.find((target) => target.id === declarationDoc?.targetId)?.class,
    "page-declaration",
  );
  assert.equal(
    render.authoring.targets.find((target) => target.id === exampleDoc?.targetId)?.class,
    "example-declaration",
  );

  const annotation = render.authoring.entries.find((entry) => entry.class === "annotation");
  const occurrence = page.provenance.occurrences[0];
  assert.ok(annotation);
  assert.ok(occurrence);
  assert.equal(occurrence.targetId, annotation.targetId);
  assert.deepEqual(occurrence.anchors, [{ root: { kind: "page" }, path: [] }]);
});

test("accepts explicit cold-invalid and stale render states", () => {
  const cold = stateFixture() as Record<string, unknown>;
  cold["sourceRevision"] = 4;
  cold["diagnostics"] = diagnostics("broken source");
  cold["render"] = null;
  assert.equal(decodeEditorState(cold).render, null);

  const stale = stateFixture() as {
    sourceRevision: number;
    render: { revision: number; freshness: string };
  };
  stale.sourceRevision = 4;
  stale.render.revision = 3;
  stale.render.freshness = "stale";
  const decodedStale = decodeEditorState(stale);
  assert.equal(decodedStale.render?.freshness, "stale");
  assert.equal(decodedStale.render?.authoring.entries.length, 3);
  assert.equal(
    decodedStale.render?.previews[0]?.provenance.occurrences.length,
    1,
    "stale metadata and provenance stay owned by the retained render",
  );
});

test("rejects malformed or internally inconsistent diagnostics envelopes", () => {
  const missingVersion = stateFixture() as Record<string, unknown>;
  missingVersion["diagnostics"] = { diagnostics: [] };
  assert.throws(() => decodeEditorState(missingVersion), /no unknown property|format/);

  const wrongCounts = stateFixture() as Record<string, unknown>;
  wrongCounts["diagnostics"] = diagnostics("broken source");
  (wrongCounts["diagnostics"] as { summary: { errors: number } }).summary.errors = 0;
  assert.throws(() => decodeEditorState(wrongCounts), /counts matching diagnostics/);
});

test("enforces current and stale revision invariants", () => {
  const current = stateFixture() as {
    sourceRevision: number;
    render: { revision: number; freshness: string };
  };
  current.sourceRevision = 4;
  assert.throws(() => decodeEditorState(current), EditorContractError);

  const stale = stateFixture() as {
    sourceRevision: number;
    render: { revision: number; freshness: string };
  };
  stale.render.freshness = "stale";
  assert.throws(() => decodeEditorState(stale), /less than sourceRevision/);
});

test("rejects unknown properties, malformed data variants, and content-kind drift", () => {
  const unknown = stateFixture() as { render: { previews: Array<Record<string, unknown>> } };
  unknown.render.previews[0]!["html"] = "<p>not semantic</p>";
  assert.throws(() => decodeEditorState(unknown), /no unknown property/);

  const waitingWithValue = stateFixture() as {
    render: { previews: Array<{ data: Array<Record<string, unknown>> }> };
  };
  waitingWithValue.render.previews[0]!.data[0]!["status"] = "waiting";
  assert.throws(() => decodeEditorState(waitingWithValue), /no value unless status is ready/);

  const fragmentPage = stateFixture() as {
    render: { previews: Array<Record<string, unknown>> };
  };
  fragmentPage.render.previews[0]!["content"] = node;
  assert.throws(() => decodeEditorState(fragmentPage), /uhura-view\/0 snapshot/);
});

test("enforces group references, identity matching, and unique IDs", () => {
  const missing = stateFixture() as {
    render: { groups: Array<{ previews: string[] }> };
  };
  missing.render.groups[0]!.previews = ["unknown"];
  assert.throws(() => decodeEditorState(missing), /existing preview id/);

  const duplicate = stateFixture() as {
    render: { previews: unknown[]; groups: Array<{ previews: string[] }> };
  };
  duplicate.render.previews.push(structuredClone(duplicate.render.previews[0]));
  duplicate.render.groups[0]!.previews.push("page-feed-default");
  assert.throws(() => decodeEditorState(duplicate), /unique values/);
});

test("strictly validates authoring classes, ranges, kinds, and references", () => {
  const malformedKind = stateFixture() as {
    render: { authoring: { entries: Array<Record<string, unknown>> } };
  };
  malformedKind.render.authoring.entries[1]!["kind"] = "Review_Note";
  assert.throws(() => decodeEditorState(malformedKind), /annotation metadata/);

  const missingTarget = stateFixture() as {
    render: { authoring: { entries: Array<Record<string, unknown>> } };
  };
  missingTarget.render.authoring.entries[0]!["targetId"] = "missing";
  assert.throws(() => decodeEditorState(missingTarget), /existing source target id/);

  const invalidRange = stateFixture() as {
    render: { authoring: { targets: Array<{ span: { start: { line: number } } }> } };
  };
  invalidRange.render.authoring.targets[0]!.span.start.line = 0;
  assert.throws(() => decodeEditorState(invalidRange), /positive integer/);

  const annotationOnDocTarget = stateFixture() as {
    render: { authoring: { entries: Array<Record<string, unknown>> } };
  };
  annotationOnDocTarget.render.authoring.entries[1]!["targetId"] = "target:feed";
  assert.throws(() => decodeEditorState(annotationOnDocTarget), /annotation metadata/);

  const unusedTarget = stateFixture() as {
    render: { authoring: { targets: Array<Record<string, unknown>> } };
  };
  const extra = structuredClone(unusedTarget.render.authoring.targets[0]!);
  extra["id"] = "target:unused";
  unusedTarget.render.authoring.targets.push(extra);
  assert.throws(() => decodeEditorState(unusedTarget), /only metadata-referenced targets/);
});

test("annotation kinds use the full ASCII lower-kebab grammar", () => {
  for (const kind of ["a", "a0", "a-0", "review-note", "a".repeat(64)]) {
    const fixture = stateFixture() as {
      render: { authoring: { entries: Array<{ kind: string }> } };
    };
    fixture.render.authoring.entries[1]!.kind = kind;
    assert.equal(decodeEditorState(fixture).render?.authoring.entries[1]?.kind, kind);
  }
  for (const kind of [
    "",
    "0note",
    "Review",
    "review_note",
    "-note",
    "note-",
    "note--later",
    "nöté",
    "a".repeat(65),
  ]) {
    const fixture = stateFixture() as {
      render: { authoring: { entries: Array<{ kind: string }> } };
    };
    fixture.render.authoring.entries[1]!.kind = kind;
    assert.throws(
      () => decodeEditorState(fixture),
      /annotation metadata|non-empty string/,
      kind,
    );
  }
});

test("validates documentation and semantic provenance while allowing zero anchors", () => {
  const zeroAnchors = stateFixture() as {
    render: { previews: Array<{ provenance: { occurrences: Array<{ anchors: unknown[] }> } }> };
  };
  zeroAnchors.render.previews[0]!.provenance.occurrences[0]!.anchors = [];
  assert.equal(
    decodeEditorState(zeroAnchors).render?.previews[0]?.provenance.occurrences[0]?.anchors.length,
    0,
  );

  const wrongDoc = stateFixture() as {
    render: { previews: Array<{ documentation: { declarationDocId: string } }> };
  };
  wrongDoc.render.previews[0]!.documentation.declarationDocId = "annotation:primary-action:0";
  assert.throws(() => decodeEditorState(wrongDoc), /doc entry for page-declaration/);

  const wrongDeclarationOwner = stateFixture() as {
    render: { authoring: { targets: Array<{ owner: { name: string } }> } };
  };
  wrongDeclarationOwner.render.authoring.targets[0]!.owner.name = "another-page";
  assert.throws(() => decodeEditorState(wrongDeclarationOwner), /doc entry for page-declaration/);

  const wrongExample = stateFixture() as {
    render: { authoring: { targets: Array<{ label: string }> } };
  };
  wrongExample.render.authoring.targets[2]!.label = "another-example";
  assert.throws(() => decodeEditorState(wrongExample), /doc entry for example-declaration/);

  const wrongRoot = stateFixture() as {
    render: { previews: Array<{ provenance: { occurrences: Array<{ anchors: unknown[] }> } }> };
  };
  wrongRoot.render.previews[0]!.provenance.occurrences[0]!.anchors = [{
    root: { kind: "fragment" },
    path: [],
  }];
  assert.throws(() => decodeEditorState(wrongRoot), /semantic node path/);

  const wrongPath = stateFixture() as {
    render: { previews: Array<{ provenance: { occurrences: Array<{ anchors: unknown[] }> } }> };
  };
  wrongPath.render.previews[0]!.provenance.occurrences[0]!.anchors = [{
    root: { kind: "page" },
    path: [9],
  }];
  assert.throws(() => decodeEditorState(wrongPath), /semantic node path/);
});

test("resolves surface roots by semantic key and rejects malformed root variants", () => {
  const withSurface = stateFixture() as {
    render: {
      previews: Array<{
        content: { surfaces: unknown[] };
        provenance: { occurrences: Array<{ anchors: unknown[] }> };
      }>;
    };
  };
  withSurface.render.previews[0]!.content.surfaces.push({
    key: "sheet:1",
    definition: "sheet",
    modality: "sheet",
    dismiss: {
      kind: "input",
      event: "dismiss",
      emit: "dismissed",
      scope: "surface:1",
      payload: {},
    },
    root: { key: "surface-root", element: "view", props: {} },
  });
  withSurface.render.previews[0]!.provenance.occurrences[0]!.anchors = [{
    root: { kind: "surface", key: "sheet:1" },
    path: [],
  }];
  assert.equal(
    decodeEditorState(withSurface).render?.previews[0]
      ?.provenance.occurrences[0]?.anchors[0]?.root.kind,
    "surface",
  );

  const missingSurface = structuredClone(withSurface);
  missingSurface.render.previews[0]!.provenance.occurrences[0]!.anchors = [{
    root: { kind: "surface", key: "missing" },
    path: [],
  }];
  assert.throws(() => decodeEditorState(missingSurface), /semantic node path/);

  const duplicateSurface = structuredClone(withSurface);
  duplicateSurface.render.previews[0]!.content.surfaces.push(structuredClone(
    duplicateSurface.render.previews[0]!.content.surfaces[0],
  ));
  assert.throws(() => decodeEditorState(duplicateSurface), /semantic node path/);

  const pageWithKey = structuredClone(withSurface);
  pageWithKey.render.previews[0]!.provenance.occurrences[0]!.anchors = [{
    root: { kind: "page", key: "illegal" },
    path: [],
  }];
  assert.throws(() => decodeEditorState(pageWithKey), /no unknown property/);
});

test("decodes only the versioned revision event", () => {
  assert.deepEqual(decodeEditorRevisionEvent({
    protocol: "uhura-editor-event/0",
    sourceRevision: 7,
  }), {
    protocol: "uhura-editor-event/0",
    sourceRevision: 7,
  });
  assert.throws(() => decodeEditorRevisionEvent({ sourceRevision: 7 }), EditorContractError);
  assert.throws(() => decodeEditorRevisionEvent({
    protocol: "uhura-editor-event/0",
    sourceRevision: 0,
  }), /integer at least 1/);
});

test("decoded result is a strongly typed EditorState", () => {
  const state: EditorState = decodeEditorState(stateFixture());
  assert.equal(state.render?.application.name, "Example");
});
