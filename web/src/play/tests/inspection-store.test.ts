import assert from "node:assert/strict";
import { test } from "vitest";

import type {
  InspectProgram,
  InspectSnapshot,
  InspectionArtifacts,
  InspectionState,
  StepResult,
} from "../../protocol/types.js";
import {
  DEFAULT_INSPECTION_HISTORY_LIMIT,
  createInspectionStore,
} from "../inspection-store.js";

const IR_HASH = "ir-hash";

function program(): InspectProgram {
  return {
    protocol: "uhura-inspect/0",
    kind: "program",
    "span-offset-encoding": "utf-8-bytes",
    ir: {
      protocol: "uhura-ir/0",
      hash: IR_HASH,
      app: "test-app",
      entry: "feed",
    },
    nodes: [],
    edges: [],
    spans: {
      "pages.feed": { file: "feed.uhura", start: 0, end: 12 },
    },
  };
}

function artifacts(generation = 7): InspectionArtifacts {
  return { generation, program: program() };
}

function result(
  revision: number,
  uHash = `u-${revision}`,
  vHash = `v-${revision}`,
): StepResult {
  return {
    c: [],
    i: [],
    g: [],
    t: {
      event: { kind: "init" },
      "u-hash": uHash,
      "v-hash": vHash,
    },
    v: {
      protocol: "uhura-view/0",
      revision,
      page: {
        route: "feed",
        root: { key: "page:1/0", element: "view", props: {} },
      },
      surfaces: [],
    },
  };
}

interface InspectionOverrides {
  irHash?: string;
  uHash?: string;
  vHash?: string;
  uRevision?: number;
  viewRevision?: number;
  nullView?: boolean;
}

function inspection(
  revision: number,
  overrides: InspectionOverrides = {},
): InspectSnapshot {
  const uHash = overrides.uHash ?? `u-${revision}`;
  const vHash = overrides.vHash ?? `v-${revision}`;
  return {
    protocol: "uhura-inspect/0",
    kind: "snapshot",
    "ir-hash": overrides.irHash ?? IR_HASH,
    revision,
    "configuration-hash": `configuration-${revision}`,
    "u-hash": uHash,
    "x-hash": `x-${revision}`,
    u: {
      rev: overrides.uRevision ?? revision,
      nav: [
        { serial: 1, route: "feed", params: {}, state: { count: revision } },
      ],
      surfaces: [],
      pending: {},
      counters: { tag: 0, "page-serial": 1, "surface-serial": 0 },
    },
    x: { snapshots: [], failed: [] },
    view: overrides.nullView
      ? null
      : {
          revision: overrides.viewRevision ?? revision,
          route: "feed",
          "surface-count": 0,
          "v-hash": vHash,
        },
    "pending-applies": [],
  };
}

function installedStore(options: Parameters<typeof createInspectionStore>[0] = {}) {
  const store = createInspectionStore(options);
  store.installArtifacts(artifacts());
  return store;
}

test("publishes immutable state immediately and retains a bounded step window", () => {
  const store = createInspectionStore({ historyLimit: 2 });
  const published: InspectionState[] = [];
  const unsubscribe = store.handle.subscribe((state) => published.push(state));

  assert.equal(published.length, 1, "subscription immediately publishes loading state");
  assert.equal(published[0]?.artifacts, null);
  store.installArtifacts(artifacts());
  store.record(result(1), inspection(1));
  store.record(result(2), inspection(2));
  store.record(result(3), inspection(3));

  const state = store.handle.state;
  assert.equal(published.length, 5);
  assert.deepEqual(
    state.history.map((step) => step.inspection.revision),
    [2, 3],
  );
  assert.equal(state.latest?.inspection.revision, 3);
  assert.equal(state.evictedSteps, 1);
  assert.equal(state.historyLimit, 2);
  assert.equal(state.artifacts?.generation, 7);
  assert.ok(Object.isFrozen(state));
  assert.ok(Object.isFrozen(state.history));
  assert.ok(Object.isFrozen(state.artifacts));
  assert.ok(Object.isFrozen(state.artifacts?.program));
  assert.ok(Object.isFrozen(state.artifacts?.program.ir));
  assert.ok(Object.isFrozen(state.latest));
  assert.ok(Object.isFrozen(state.latest?.trace));
  assert.ok(Object.isFrozen(state.latest?.inspection));
  assert.ok(Object.isFrozen(state.latest?.inspection.u.nav));
  assert.ok(Object.isFrozen(state.latest?.inspection.u.nav[0]?.state));
  assert.strictEqual(published.at(-1), state);

  unsubscribe();
  unsubscribe();
});

test("isolates listener failures, supports unsubscribe, and replays current state", () => {
  const listenerErrors: unknown[] = [];
  const store = installedStore({
    onListenerError: (error) => listenerErrors.push(error),
  });
  let failingCalls = 0;
  store.handle.subscribe(() => {
    failingCalls += 1;
    throw new Error(`listener-${failingCalls}`);
  });
  let healthyCalls = 0;
  const unsubscribe = store.handle.subscribe(() => {
    healthyCalls += 1;
  });

  assert.equal(failingCalls, 1);
  assert.equal(healthyCalls, 1);
  store.record(result(1), inspection(1));
  assert.equal(failingCalls, 2);
  assert.equal(healthyCalls, 2);
  assert.equal(listenerErrors.length, 2);

  unsubscribe();
  unsubscribe();
  store.record(result(2), inspection(2));
  assert.equal(failingCalls, 3);
  assert.equal(healthyCalls, 2);

  let replayedRevision = 0;
  store.handle.subscribe((state) => {
    replayedRevision = state.latest?.inspection.revision ?? 0;
  })();
  assert.equal(replayedRevision, 2, "late listeners immediately receive current state");
});

test("strictly correlates inspection snapshots with artifacts, traces, and views", () => {
  assert.throws(
    () => createInspectionStore({ historyLimit: 0 }),
    /history limit must be a positive safe integer/,
  );
  assert.equal(
    createInspectionStore().handle.state.historyLimit,
    DEFAULT_INSPECTION_HISTORY_LIMIT,
  );

  const missingArtifacts = createInspectionStore();
  assert.throws(
    () => missingArtifacts.record(result(1), inspection(1)),
    /artifacts must be installed/,
  );

  const malformedProgram = program();
  (malformedProgram as unknown as { protocol: string }).protocol =
    "uhura-inspect/1";
  assert.throws(
    () => createInspectionStore().installArtifacts({
      generation: 1,
      program: malformedProgram as InspectProgram,
    }),
    /must be an uhura-inspect\/0 program/,
  );

  const badSpanEncoding = program();
  (
    badSpanEncoding as unknown as { "span-offset-encoding": string }
  )["span-offset-encoding"] = "utf-16-code-units";
  assert.throws(
    () => createInspectionStore().installArtifacts({
      generation: 1,
      program: badSpanEncoding as InspectProgram,
    }),
    /UTF-8 byte offsets/,
  );

  assert.throws(
    () => installedStore().record(result(1), inspection(1, { irHash: "other" })),
    /IR hash does not match/,
  );
  assert.throws(
    () => installedStore().record(result(1), inspection(1, { uHash: "other" })),
    /U hash does not match/,
  );
  assert.throws(
    () => installedStore().record(result(2, "u-1", "v-1"), inspection(1)),
    /revision does not match the step view revision/,
  );
  assert.throws(
    () => installedStore().record(
      result(1),
      inspection(1, { viewRevision: 2 }),
    ),
    /view metadata revision does not match/,
  );
  assert.throws(
    () => installedStore().record(result(1), inspection(1, { vHash: "other" })),
    /view hash does not match/,
  );
  assert.throws(
    () => installedStore().record(result(1), inspection(1, { uRevision: 2 })),
    /U revision does not match/,
  );
  assert.throws(
    () => installedStore().record(result(1), inspection(1, { nullView: true })),
    /must include view metadata/,
  );

  const monotonic = installedStore();
  monotonic.record(result(2), inspection(2));
  assert.throws(
    () => monotonic.record(result(2), inspection(2)),
    /must increase monotonically/,
  );
  assert.throws(
    () => monotonic.installArtifacts(artifacts()),
    /already installed/,
  );
});

test("dispose publishes a cleared terminal state and ignores late async writes", () => {
  const store = createInspectionStore();
  const published: InspectionState[] = [];
  store.handle.subscribe((state) => published.push(state));
  store.installArtifacts(artifacts());
  store.record(result(1), inspection(1));

  store.dispose();
  store.dispose();
  assert.equal(published.length, 4, "dispose publishes exactly one terminal state");
  assert.deepEqual(store.handle.state, {
    disposed: true,
    historyLimit: DEFAULT_INSPECTION_HISTORY_LIMIT,
    artifacts: null,
    latest: null,
    history: [],
    evictedSteps: 0,
  });
  assert.ok(Object.isFrozen(store.handle.state));
  assert.ok(Object.isFrozen(store.handle.state.history));

  assert.equal(store.installArtifacts(artifacts(8)), false);
  assert.equal(store.record(result(2), inspection(2)), false);
  assert.equal(published.length, 4, "retired listeners receive no late writes");

  let disposedReplay = false;
  const unsubscribe = store.handle.subscribe((state) => {
    disposedReplay = state.disposed;
  });
  unsubscribe();
  unsubscribe();
  assert.equal(disposedReplay, true);
});
