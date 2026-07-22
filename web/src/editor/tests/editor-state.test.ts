import assert from "node:assert/strict";

import { test } from "vitest";

import {
  decodeEditorRevisionEvent,
  decodeEditorState,
  EDITOR_STATE_PROTOCOL,
  EditorContractError,
  type EditorState,
} from "../editor-state.js";

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

const stateFixture = (): {
  protocol: string;
  sourceRevision: number;
  diagnostics: unknown;
  render: Record<string, any> | null;
} => ({
  protocol: "uhura-editor-state/5",
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
        file: "web.uhura",
        span: span(20, 9, 2, 1),
        label: "Feed",
        owner: { kind: "page", name: "example@1::Feed" },
      }, {
        id: "target:primary-action",
        class: "ui-element",
        file: "web.uhura",
        span: span(80, 10, 6, 1),
        label: "button",
        owner: { kind: "page", name: "example@1::Feed" },
      }, {
        id: "target:feed-example",
        class: "example-declaration",
        file: "evidence.uhura",
        span: span(20, 15, 2, 1),
        label: "default",
        owner: { kind: "examples", name: "evidence.uhura" },
      }],
      entries: [{
        id: "doc:feed",
        class: "doc",
        kind: "doc",
        text: "The feed presentation.",
        span: span(0, 18, 1, 1),
        targetId: "target:feed",
        order: 0,
      }, {
        id: "annotation:primary-action:0",
        class: "annotation",
        kind: "review-note",
        text: "The primary action.",
        span: span(50, 28, 5, 1),
        targetId: "target:primary-action",
        order: 0,
      }, {
        id: "doc:feed-example",
        class: "doc",
        kind: "doc",
        text: "The default evidence pin.",
        span: span(0, 18, 1, 1),
        targetId: "target:feed-example",
        order: 0,
      }],
    },
    groups: [{
      id: "page-feed",
      kind: "page",
      subject: "example@1::Feed",
      previews: ["page-feed-default"],
    }],
    previews: [{
      id: "page-feed-default",
      identity: { kind: "page", subject: "example@1::Feed", example: "default" },
      sourceFile: "web.uhura",
      default: true,
      pinned: true,
      derived: false,
      inFlight: 0,
      from: null,
      replaySteps: ["opened"],
      replay: [{
        label: "opened",
        kind: "semantic",
        payload: { id: "post-1" },
        dispatch: {
          scope: "entry/example",
          definition: "example@1::App",
          on: "opened",
          guards: [
            { handler: 0, result: "unsatisfied" },
            { handler: 1, result: "satisfied" },
          ],
          selected: 1,
          aborted: null,
        },
        effects: {
          writes: [{ field: "selected", value: "post-1" }],
          commands: [],
          intents: [],
          structural: [],
          projections: [],
        },
      }],
      note: null,
      data: [{
        group: "properties",
        name: "title",
        key: null,
        status: "ready",
        value: "Feed",
        source: {
          kind: "inline",
          declaredIn: "web.uhura",
          timeline: false,
        },
      }],
      interactions: [{
        nodeKey: "root",
        element: "button",
        kind: "input",
        event: "press",
        emit: "opened",
        scope: "entry/example",
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
          anchors: ["root"],
        }],
      },
      evidence: {
        scenario: "ready",
        pin: "default",
        sourceId: "evidence/default",
        sources: {
          registration: { path: "evidence.uhura", start: 0, end: 8 },
          pin: { path: "evidence.uhura", start: 9, end: 12 },
        },
      },
      content: {
        kind: "projection",
        value: {
          document: {
            protocol: "uhura-view/1",
            presentation: "example@1::Feed",
            machine: "example@1::App",
            instance: "entry/example",
            sequence: "0",
            nodes: [{
              kind: "element",
              key: "root",
              element: "main",
              attributes: [],
              events: [],
              children: [{ kind: "text", key: "label", text: "Ready" }],
              surface: false,
            }],
          },
          sources: {
            protocol: "uhura-projection-sources/0",
            presentation: "example@1::Feed",
            nodes: {
              root: { id: "ui/root", path: "web.uhura", start: 0, end: 4 },
              label: { id: "ui/label", path: "web.uhura", start: 5, end: 10 },
            },
          },
        },
      },
    }],
    stylesheet: ":root { --accent: blue; }",
    assets: {
      avatar: { dataUri: "data:image/png;base64,AA==", alt: "Avatar" },
    },
    interactionGraph: {
      protocol: "uhura-interaction-graph/0",
      app: "example",
      entry: "page:feed",
      nodes: [
        { id: "page:feed", kind: "page", label: "feed" },
        { id: "surface:comments", kind: "surface", label: "comments", modality: "dialog" },
      ],
      edges: [{
        id: "edge/0",
        kind: "present",
        from: "page:feed",
        to: "surface:comments",
        event: "comments-requested",
      }],
    },
    machine: {
      protocol: "uhura-machine-inspection/1",
      identityProtocol: "uhura-machine-program/0",
      deployment: { machine: "example@1::App" },
      sources: [],
      provenance: {
        protocol: "uhura-provenance/0",
        sources: [],
        occurrences: [],
        topology: {
          protocol: "uhura-authored-interaction-topology/0",
          nodes: [],
          edges: [],
        },
      },
      interactionGraph: {
        protocol: "uhura-interaction-graph/0",
        identity_protocol: "uhura-machine-program/0",
        machine_program_hashes: {},
        presentation_hashes: {},
        outcome_policies: {},
        nodes: [],
        edges: [],
      },
      graphSources: {
        protocol: "uhura-interaction-graph-provenance/0",
        nodes: [],
        edges: [],
      },
      checkpoints: {},
      evidence: {
        protocol: "uhura-evidence-summary/0",
        passed: true,
        scenarios: { total: 1, passed: 1, failed: 0 },
        artifacts: { pins: 1, examples: 1, checkpoints: 0 },
        failureCount: 0,
      },
    },
  },
});

test("decodes the canonical projection-only EditorState/5 contract", () => {
  const state = decodeEditorState(stateFixture());
  const preview = state.render?.previews[0];

  assert.equal(state.protocol, EDITOR_STATE_PROTOCOL);
  assert.equal(preview?.content.kind, "projection");
  assert.equal(preview?.content.value.document.protocol, "uhura-view/1");
  assert.deepEqual(preview?.provenance.occurrences[0]?.anchors, ["root"]);
  assert.equal(preview?.evidence?.scenario, "ready");
  assert.equal(state.render?.machine?.identityProtocol, "uhura-machine-program/0");
  assert.equal(state.render?.machine?.evidence.scenarios.passed, 1);
  assert.deepEqual(state.render?.interactionGraph.edges[0], {
    kind: "present",
    from: "page:feed",
    to: "surface:comments",
    event: "comments-requested",
  });
});

test("strictly decodes the same machine graph artifact consumed by Play", () => {
  const invalid = stateFixture();
  invalid.render!.machine.interactionGraph = {};
  assert.throws(
    () => decodeEditorState(invalid),
    /interaction graph has the wrong fields/u,
  );
});

test("keeps generated provenance and evidence-only sources in their honest inventories", () => {
  const state = stateFixture();
  const authoredHash = "a".repeat(64);
  const generatedHash = "b".repeat(64);
  const evidenceHash = "c".repeat(64);
  state.render!.machine.sources = [{
    path: "web.uhura",
    sha256: authoredHash,
    bytes: 100,
  }, {
    path: "evidence.uhura",
    sha256: evidenceHash,
    bytes: 50,
  }];
  state.render!.machine.provenance.sources = [{
    source: 0,
    package: "example@1",
    module: "web",
    path: "web.uhura",
    sha256: authoredHash,
    bytes: 100,
  }, {
    source: 1,
    package: "example@1",
    module: "framework::application",
    path: ".uhura/generated/web-app/application.uhura",
    sha256: generatedHash,
    bytes: 80,
  }];

  const decoded = decodeEditorState(state);
  assert.deepEqual(decoded.render?.machine?.sources, [{
    path: "web.uhura",
    sha256: authoredHash,
    bytes: 100,
  }, {
    path: "evidence.uhura",
    sha256: evidenceHash,
    bytes: 50,
  }]);
  assert.equal(decoded.render?.machine?.provenance.sources.length, 2);

  state.render!.machine.sources[0].sha256 = "d".repeat(64);
  assert.throws(
    () => decodeEditorState(state),
    /overlapping source paths/u,
  );
});

test("rejects every retired Editor view and structural anchor encoding", () => {
  const oldProtocol = stateFixture();
  oldProtocol.protocol = "uhura-editor-state/4";
  assert.throws(() => decodeEditorState(oldProtocol), /uhura-editor-state\/5/);

  for (const kind of ["snapshot", "fragment"]) {
    const oldContent = stateFixture();
    oldContent.render!.previews[0].content = { kind, value: {} };
    assert.throws(() => decodeEditorState(oldContent), /"projection"/);
  }

  const pathAnchor = stateFixture();
  pathAnchor.render!.previews[0].provenance.occurrences[0].anchors = [{
    kind: "path",
    root: { kind: "page" },
    path: [],
  }];
  assert.throws(() => decodeEditorState(pathAnchor), /non-empty string/);
});

test("rejects unbounded machine and preview evidence from the retired transport", () => {
  const rawMachineEvidence = stateFixture();
  rawMachineEvidence.render!.machine.evidence = {
    passed: true,
    scenarios: [],
    failures: [],
  };
  assert.throws(
    () => decodeEditorState(rawMachineEvidence),
    /evidence has the wrong fields/u,
  );

  for (const field of ["observation", "snapshot", "scenarioReceiptLog"]) {
    const rawPreviewEvidence = stateFixture();
    rawPreviewEvidence.render!.previews[0].evidence[field] = {};
    assert.throws(
      () => decodeEditorState(rawPreviewEvidence),
      /no unknown property/u,
    );
  }
});

test("validates projection source coverage and semantic anchor keys", () => {
  const missingSource = stateFixture();
  delete missingSource.render!.previews[0].content.value.sources.nodes.label;
  assert.throws(() => decodeEditorState(missingSource), /must address every rendered key exactly/);

  const unknownAnchor = stateFixture();
  unknownAnchor.render!.previews[0].provenance.occurrences[0].anchors = ["missing"];
  assert.throws(() => decodeEditorState(unknownAnchor), /semantic node key/);

  const duplicateAnchor = stateFixture();
  duplicateAnchor.render!.previews[0].provenance.occurrences[0].anchors = ["root", "root"];
  assert.throws(() => decodeEditorState(duplicateAnchor), /unique values/);
});

test("accepts cold-invalid and stale render states with strict revisions", () => {
  const cold = stateFixture();
  cold.sourceRevision = 4;
  cold.diagnostics = diagnostics("broken source");
  cold.render = null;
  assert.equal(decodeEditorState(cold).render, null);

  const stale = stateFixture();
  stale.sourceRevision = 4;
  stale.render!.revision = 3;
  stale.render!.freshness = "stale";
  assert.equal(decodeEditorState(stale).render?.freshness, "stale");

  const invalidCurrent = stateFixture();
  invalidCurrent.sourceRevision = 4;
  assert.throws(() => decodeEditorState(invalidCurrent), /sourceRevision 4/);

  const invalidStale = stateFixture();
  invalidStale.render!.freshness = "stale";
  assert.throws(() => decodeEditorState(invalidStale), /less than sourceRevision/);
});

test("strictly validates diagnostics, authoring, replay, and group references", () => {
  const wrongCounts = stateFixture();
  wrongCounts.diagnostics = diagnostics("broken source");
  (wrongCounts.diagnostics as { summary: { errors: number } }).summary.errors = 0;
  assert.throws(() => decodeEditorState(wrongCounts), /counts matching diagnostics/);

  const unknownTarget = stateFixture();
  unknownTarget.render!.previews[0].provenance.occurrences[0].targetId = "missing";
  assert.throws(() => decodeEditorState(unknownTarget), /annotatable source target/);

  const mismatchedReplay = stateFixture();
  mismatchedReplay.render!.previews[0].replaySteps[0] = "other";
  assert.throws(() => decodeEditorState(mismatchedReplay), /matching replaySteps/);

  const missingPreview = stateFixture();
  missingPreview.render!.groups[0].previews = ["missing"];
  assert.throws(() => decodeEditorState(missingPreview), /existing preview id/);
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
