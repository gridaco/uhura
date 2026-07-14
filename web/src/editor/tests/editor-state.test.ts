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
  protocol: "uhura-editor-state/0",
  sourceRevision: 3,
  diagnostics: null,
  render: {
    revision: 3,
    freshness: "current",
    application: { name: "Example" },
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

  assert.equal(state.protocol, "uhura-editor-state/0");
  assert.equal(state.sourceRevision, 3);
  assert.equal(state.render?.previews[0]?.data[0]?.source?.kind, "fixture");
  assert.deepEqual(state.render?.previews[0]?.interactions[0]?.payload, { id: "post-1" });
  assert.equal(state.render?.icons["heart"]?.commands[0]?.kind, "path");
});

test("decodes the native model's canonical contract fixture", () => {
  const fixture = JSON.parse(readFileSync(new URL(
    "../../../../crates/uhura-editor-model/tests/fixtures/editor-state.json",
    import.meta.url,
  ), "utf8")) as unknown;

  const state = decodeEditorState(fixture);
  assert.equal(state.render?.previews.length, 3);
  assert.equal(state.render?.previews[1]?.identity.kind, "surface");
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
  assert.equal(decodeEditorState(stale).render?.freshness, "stale");
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
