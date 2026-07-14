import assert from "node:assert/strict";

import { test } from "vitest";

import {
  EDITOR_CHECKPOINT_KEY,
  EditorLiveSession,
  parseEditorLiveEvent,
  parseHostedBuildId,
  parseHostedGeneration,
  shouldReloadEditor,
  storeEditorCheckpoint,
  summarizeDiagnostics,
  takeEditorCheckpoint,
  type EditorShellState,
  type StorageLike,
} from "../live-preview.js";

const activeEvent = (
  candidateGeneration: number,
  activeBuildId: string,
) => ({
  candidateGeneration,
  activeGeneration: candidateGeneration,
  activeBuildId,
  status: "active" as const,
});

const rejectedEvent = (
  candidateGeneration: number,
  activeGeneration: number | null,
  activeBuildId: string | null,
) => ({
  candidateGeneration,
  activeGeneration,
  activeBuildId,
  status: "rejected" as const,
  diagnostics: { format: "uhura-diagnostics", diagnostics: [] },
});

class MemoryStorage implements StorageLike {
  readonly values = new Map<string, string>();

  getItem(key: string): string | null {
    return this.values.get(key) ?? null;
  }

  setItem(key: string, value: string): void {
    this.values.set(key, value);
  }

  removeItem(key: string): void {
    this.values.delete(key);
  }
}

const shellState: EditorShellState = {
  camera: { x: -320.5, y: 64, scale: 0.75 },
  tool: "hand",
  search: "profile",
  uiVisible: false,
  selection: { kind: "page", subject: "profile", example: "private" },
};

test("host generation uses zero only as the cold-invalid sentinel", () => {
  assert.equal(parseHostedGeneration(null), undefined);
  assert.equal(parseHostedGeneration(""), undefined);
  assert.equal(parseHostedGeneration("0"), null);
  assert.equal(parseHostedGeneration("17"), 17);
  assert.equal(parseHostedGeneration("-1"), undefined);
  assert.equal(parseHostedGeneration("1.5"), undefined);
});

test("host build identity uses an empty value only as the cold-invalid sentinel", () => {
  assert.equal(parseHostedBuildId(null), undefined);
  assert.equal(parseHostedBuildId(""), null);
  assert.equal(parseHostedBuildId("   "), null);
  assert.equal(parseHostedBuildId("sha256:canvas-a"), "sha256:canvas-a");
});

test("live events are checked at the untrusted SSE boundary", () => {
  assert.deepEqual(parseEditorLiveEvent({
    candidateGeneration: 5,
    activeGeneration: 4,
    activeBuildId: "sha256:canvas-a",
    status: "rejected",
    diagnostics: { format: "uhura-diagnostics", diagnostics: [] },
  }), {
    candidateGeneration: 5,
    activeGeneration: 4,
    activeBuildId: "sha256:canvas-a",
    status: "rejected",
    diagnostics: { format: "uhura-diagnostics", diagnostics: [] },
  });
  assert.equal(parseEditorLiveEvent({
    candidateGeneration: 5,
    activeGeneration: null,
    activeBuildId: null,
    status: "active",
  }), null);
  assert.equal(parseEditorLiveEvent({
    candidateGeneration: 5,
    activeGeneration: 0,
    activeBuildId: "sha256:canvas-a",
    status: "active",
  }), null);
  assert.equal(parseEditorLiveEvent({
    candidateGeneration: "5",
    activeGeneration: 4,
    activeBuildId: "sha256:canvas-a",
    status: "rejected",
  }), null);
  assert.equal(parseEditorLiveEvent({
    candidateGeneration: 5,
    activeGeneration: 4,
    activeBuildId: null,
    status: "rejected",
  }), null);
  assert.equal(parseEditorLiveEvent({
    candidateGeneration: 5,
    activeGeneration: 4,
    activeBuildId: "sha256:canvas-a",
    status: "active",
  }), null);
  assert.equal(parseEditorLiveEvent({
    candidateGeneration: 5,
    activeGeneration: 5,
    activeBuildId: "sha256:canvas-a",
    status: "rejected",
  }), null);
});

test("reload convergence follows content identity rather than process-local counters", () => {
  assert.equal(shouldReloadEditor("sha256:old", {
    candidateGeneration: 1,
    activeGeneration: 1,
    activeBuildId: "sha256:new",
    status: "active",
  }), true);
  assert.equal(shouldReloadEditor("sha256:same", {
    candidateGeneration: 9,
    activeGeneration: 8,
    activeBuildId: "sha256:same",
    status: "rejected",
  }), false);
});

test("live session reloads exactly once when the active content changes", () => {
  const live = new EditorLiveSession("sha256:old");
  const next = activeEvent(2, "sha256:new");

  assert.deepEqual(live.accept(next), { kind: "reload", event: next });
  assert.equal(live.reloading, true);
  assert.deepEqual(live.accept(activeEvent(3, "sha256:newer")), { kind: "ignored" });
});

test("live session surfaces rejected and warning-bearing active candidates without reload", () => {
  const live = new EditorLiveSession("sha256:current");
  const rejected = rejectedEvent(8, 7, "sha256:current");
  const active = {
    ...activeEvent(9, "sha256:current"),
    diagnostics: {
      format: "uhura-diagnostics",
      diagnostics: [{
        code: "UH0301",
        rule: "markup/unkeyed-each",
        severity: "warning",
        message: "use a stable key",
        file: "components/feed.uhura",
        span: { start: { line: 12, col: 7 } },
      }],
    },
  };

  assert.deepEqual(live.accept(rejected), { kind: "rejected", event: rejected });
  assert.deepEqual(live.accept(active), { kind: "active", event: active });
  assert.equal(summarizeDiagnostics(active.diagnostics)[0]?.severity, "warning");
});

test("cold-invalid session stays put for errors and reloads once for its first valid Canvas", () => {
  const live = new EditorLiveSession(null);
  const coldRejected = rejectedEvent(1, null, null);
  const firstValid = activeEvent(2, "sha256:first-valid");

  assert.deepEqual(live.accept(coldRejected), { kind: "rejected", event: coldRejected });
  assert.deepEqual(live.accept(firstValid), { kind: "reload", event: firstValid });
  assert.deepEqual(live.accept(activeEvent(3, "sha256:later")), { kind: "ignored" });
});

test("a stale document reloads to a newer last-good Canvas before showing its rejection", () => {
  const live = new EditorLiveSession("sha256:document-a");
  const rejectedOverNewerActive = rejectedEvent(3, 2, "sha256:active-b");

  assert.deepEqual(live.accept(rejectedOverNewerActive), {
    kind: "reload",
    event: rejectedOverNewerActive,
  });
});

test("live session ignores stale candidates but accepts a reset counter after reconnect", () => {
  const live = new EditorLiveSession("sha256:current");
  const current = rejectedEvent(12, 11, "sha256:current");

  assert.equal(live.accept(current).kind, "rejected");
  assert.deepEqual(
    live.accept(rejectedEvent(11, 10, "sha256:current")),
    { kind: "ignored" },
  );

  live.reconnect();
  const restarted = activeEvent(1, "sha256:restarted");
  assert.deepEqual(live.accept(restarted), { kind: "reload", event: restarted });
});

test("equal process-local generations still converge after a host restart", () => {
  const live = new EditorLiveSession("sha256:old-process");
  const restarted = activeEvent(1, "sha256:new-process");

  assert.deepEqual(live.accept(restarted), { kind: "reload", event: restarted });
});

test("checkpoints are one-use and scoped to the reload handoff", () => {
  const storage = new MemoryStorage();
  assert.equal(storeEditorCheckpoint(storage, "build-b", shellState), true);
  assert.deepEqual(takeEditorCheckpoint(storage, "build-b"), shellState);
  assert.equal(takeEditorCheckpoint(storage, "build-b"), null);

  assert.equal(storeEditorCheckpoint(storage, "build-b", shellState), true);
  assert.deepEqual(takeEditorCheckpoint(storage, "build-a"), shellState);
  assert.equal(storage.getItem(EDITOR_CHECKPOINT_KEY), null);
});

test("any build served by the pending reload may consume its one-use checkpoint", () => {
  const storage = new MemoryStorage();
  assert.equal(storeEditorCheckpoint(storage, "build-b", shellState), true);
  assert.deepEqual(takeEditorCheckpoint(storage, "build-c"), shellState);
});

test("malformed checkpoints are consumed without escaping bad camera state", () => {
  const storage = new MemoryStorage();
  storage.setItem(EDITOR_CHECKPOINT_KEY, JSON.stringify({
    version: 2,
    targetBuildId: "build-b",
    camera: { x: 0, y: 0, scale: "nan" },
    tool: "hand",
    search: "",
    uiVisible: true,
    selection: null,
  }));
  assert.equal(takeEditorCheckpoint(storage, "build-b"), null);
  assert.equal(storage.getItem(EDITOR_CHECKPOINT_KEY), null);
});

test("diagnostic summaries retain canonical code, rule, and source position", () => {
  assert.deepEqual(summarizeDiagnostics({
    diagnostics: [{
      code: "UH0301",
      rule: "markup/unkeyed-each",
      severity: "error",
      message: "missing a stable key",
      file: "components/feed.uhura",
      span: { start: { line: 12, col: 7 } },
    }],
  }), [{
    code: "UH0301",
    rule: "markup/unkeyed-each",
    severity: "error",
    message: "missing a stable key",
    location: "components/feed.uhura:12:7",
  }]);
});
