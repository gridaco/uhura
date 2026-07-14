import assert from "node:assert/strict";
import { test } from "vitest";

import type {
  InspectProgram,
  InspectSnapshot,
  InspectionState,
  StepTrace,
} from "../../protocol/types.js";
import {
  deriveDebugGraph,
  formatDebugValue,
  runtimeDefinitionIdForTrace,
} from "../debug-model.js";

const IR_HASH = "debug-ir";

function program(): InspectProgram {
  return {
    protocol: "uhura-inspect/0",
    kind: "program",
    "span-offset-encoding": "utf-8-bytes",
    ir: {
      protocol: "uhura-ir/0",
      hash: IR_HASH,
      app: "debug-app",
      entry: "feed",
    },
    nodes: [
      {
        id: "pages.feed",
        kind: "definition",
        "definition-kind": "page",
        name: "feed",
        entry: true,
      },
      {
        id: "pages.detail",
        kind: "definition",
        "definition-kind": "page",
        name: "detail",
      },
      {
        id: "surfaces.comments",
        kind: "definition",
        "definition-kind": "surface",
        name: "comments",
      },
      {
        id: "components.card",
        kind: "definition",
        "definition-kind": "component",
        name: "card",
      },
      {
        id: "pages.feed/event/like",
        kind: "event",
        definition: "pages.feed",
        name: "like",
        "event-kind": "semantic",
      },
      {
        id: "pages.feed/event/like-post.ok",
        kind: "event",
        definition: "pages.feed",
        name: "like-post.ok",
        "event-kind": "outcome",
        command: "like-post",
        outcome: "ok",
      },
      {
        id: "pages.feed/handler/0",
        kind: "handler",
        definition: "pages.feed",
        index: 0,
        on: "like",
        guarded: true,
        effects: [],
      },
      {
        id: "pages.feed/handler/1",
        kind: "handler",
        definition: "pages.feed",
        index: 1,
        on: "like-post.ok",
        guarded: false,
        effects: [],
      },
      {
        id: "pages.feed/state/liked",
        kind: "state",
        definition: "pages.feed",
        name: "liked",
        initial: false,
      },
      {
        id: "pages.feed/state/pending",
        kind: "state",
        definition: "pages.feed",
        name: "pending",
        initial: false,
      },
      {
        id: "projections.feed-page",
        kind: "projection",
        name: "feed-page",
        port: "feed",
        boot: true,
        keyed: false,
      },
      {
        id: "commands.like-post",
        kind: "command",
        name: "like-post",
        port: "feed",
      },
      {
        id: "commands.submit-comment",
        kind: "command",
        name: "submit-comment",
        port: "comments",
      },
      {
        id: "surfaces.comments/event/close",
        kind: "event",
        definition: "surfaces.comments",
        name: "close",
        "event-kind": "semantic",
      },
      {
        id: "surfaces.comments/handler/0",
        kind: "handler",
        definition: "surfaces.comments",
        index: 0,
        on: "close",
        guarded: false,
        effects: ["dismiss"],
      },
      {
        id: "surfaces.comments/state/draft",
        kind: "state",
        definition: "surfaces.comments",
        name: "draft",
        initial: "",
      },
    ],
    edges: [
      {
        kind: "handles",
        from: "pages.feed/event/like",
        to: "pages.feed/handler/0",
      },
      {
        kind: "handles",
        from: "pages.feed/event/like-post.ok",
        to: "pages.feed/handler/1",
      },
      {
        kind: "guard-reads",
        from: "pages.feed/state/pending",
        to: "pages.feed/handler/0",
      },
      {
        kind: "body-reads",
        from: "projections.feed-page",
        to: "pages.feed/handler/0",
      },
      {
        kind: "writes",
        from: "pages.feed/handler/0",
        to: "pages.feed/state/liked",
        order: 0,
      },
      {
        kind: "writes",
        from: "pages.feed/handler/1",
        to: "pages.feed/state/pending",
        order: 0,
      },
      {
        kind: "sends",
        from: "pages.feed/handler/0",
        to: "commands.like-post",
        order: 1,
      },
      {
        kind: "opens",
        from: "pages.feed/handler/0",
        to: "surfaces.comments",
        order: 2,
      },
      {
        kind: "navigates",
        from: "pages.feed/handler/0",
        to: "pages.detail",
        order: 3,
        mode: "push",
      },
      {
        kind: "settles",
        from: "commands.like-post",
        to: "pages.feed/event/like-post.ok",
      },
      {
        kind: "handles",
        from: "surfaces.comments/event/close",
        to: "surfaces.comments/handler/0",
      },
      {
        kind: "writes",
        from: "surfaces.comments/handler/0",
        to: "surfaces.comments/state/draft",
        order: 0,
      },
      {
        kind: "sends",
        from: "surfaces.comments/handler/0",
        to: "commands.submit-comment",
        order: 1,
      },
    ],
    spans: {
      "pages.feed": { file: "app/feed/page.uhura", start: 0, end: 200 },
      "pages.feed/handler/0": {
        file: "app/feed/page.uhura",
        start: 32,
        end: 120,
      },
    },
  };
}

function snapshot(revision = 4): InspectSnapshot {
  return {
    protocol: "uhura-inspect/0",
    kind: "snapshot",
    "ir-hash": IR_HASH,
    revision,
    "configuration-hash": `configuration-${revision}`,
    "u-hash": `u-${revision}`,
    "x-hash": `x-${revision}`,
    u: {
      rev: revision,
      nav: [
        { serial: 1, route: "feed", params: {}, state: { liked: true, pending: true } },
        { serial: 2, route: "detail", params: {}, state: {} },
      ],
      surfaces: [
        {
          serial: 2,
          definition: "comments",
          props: {},
          state: {},
          opener: "page:1",
        },
      ],
      pending: {
        "c-1": {
          port: "feed",
          command: "like-post",
          payload: { post: "post-1" },
          origin: "page:1",
        },
      },
      counters: { tag: 1, "page-serial": 2, "surface-serial": 2 },
    },
    x: {
      snapshots: [
        { projection: "feed-page", key: null, revision: 3, value: { posts: 2 } },
      ],
      failed: [
        { projection: "feed-page", key: "page-2", reason: "offline" },
      ],
    },
    view: {
      revision,
      route: "detail",
      "surface-count": 1,
      "v-hash": `v-${revision}`,
    },
    "pending-applies": [],
  };
}

function feedTrace(): StepTrace {
  return {
    event: { kind: "ui" },
    dispatch: {
      scope: "page:1",
      definition: "feed",
      on: "like",
      guards: [{ handler: 0, guard: "satisfied" }],
      selected: 0,
      writes: [{ field: "liked", value: true }],
    },
    applies: [
      { apply: "applied", projection: "feed-page", revision: 3 },
    ],
    structural: [
      { op: "navigate", route: "detail", serial: 2 },
      { op: "open-surface", surface: "comments:2", opener: "page:1" },
    ],
    c: [
      {
        kind: "command",
        port: "feed",
        command: "like-post",
        correlation: "c-1",
        payload: { post: "post-1" },
      },
    ],
    "u-hash": "u-4",
    "v-hash": "v-4",
  };
}

function state(trace: StepTrace | null = feedTrace(), inspect = snapshot()): InspectionState {
  return {
    disposed: false,
    historyLimit: 128,
    artifacts: { generation: 7, program: program() },
    latest: trace ? { trace, inspection: inspect } : null,
    history: [],
    evictedSteps: 0,
  };
}

function byId<T extends { id: string }>(items: readonly T[], id: string): T {
  const found = items.find((item) => item.id === id);
  assert.ok(found, `missing ${id}`);
  return found;
}

test("derives a stable focused neighborhood with exact runtime correlation", () => {
  const model = deriveDebugGraph(state());

  assert.equal(model.emptyReason, null);
  assert.equal(model.generation, 7);
  assert.equal(model.programHash, IR_HASH);
  assert.equal(model.revision, 4);
  assert.equal(model.focusDefinitionId, "pages.feed");
  assert.equal(model.runtimeDefinitionId, "pages.feed");
  assert.deepEqual(
    model.definitions.map((definition) => definition.id),
    ["pages.detail", "pages.feed", "surfaces.comments", "components.card"],
  );
  assert.deepEqual(
    model.definitions.map((definition) => ({
      id: definition.id,
      active: definition.active,
      top: definition.top,
      runtime: definition.runtime,
      transition: definition.transitionTarget,
    })),
    [
      { id: "pages.detail", active: true, top: false, runtime: false, transition: true },
      { id: "pages.feed", active: true, top: false, runtime: true, transition: false },
      { id: "surfaces.comments", active: true, top: true, runtime: false, transition: true },
      { id: "components.card", active: false, top: false, runtime: false, transition: false },
    ],
  );

  const handler = byId(model.nodes, "pages.feed/handler/0");
  assert.equal(handler.label, "Handler 1");
  assert.equal(handler.detail, "on like · guarded");
  assert.equal(handler.lane, "handler");
  assert.equal(handler.runtime.consulted, "satisfied");
  assert.equal(handler.runtime.selected, true);
  assert.equal(handler.runtime.current, true);
  assert.deepEqual(handler.span, {
    file: "app/feed/page.uhura",
    start: 32,
    end: 120,
  });

  const event = byId(model.nodes, "pages.feed/event/like");
  assert.equal(event.lane, "input");
  assert.equal(event.runtime.current, true);
  assert.equal(event.span, null, "source spans are intentionally optional");

  const stateNode = byId(model.nodes, "pages.feed/state/liked");
  assert.equal(stateNode.lane, "effect");
  assert.equal(stateNode.detail, "true");
  assert.equal(stateNode.runtime.active, true);
  assert.equal(stateNode.runtime.written, true);

  const projection = byId(model.nodes, "projections.feed-page");
  assert.equal(projection.lane, "input");
  assert.equal(projection.detail, "1 ready · 1 failed");
  assert.equal(projection.runtime.projectionApply, "applied");
  assert.equal(projection.runtime.projectionReady, 1);
  assert.equal(projection.runtime.projectionFailures, 1);

  const command = byId(model.nodes, "commands.like-post");
  assert.equal(command.detail, "1 pending");
  assert.equal(command.runtime.sent, true);
  assert.equal(command.runtime.pending, 1);

  assert.equal(byId(model.nodes, "pages.detail").runtime.structural, true);
  assert.equal(byId(model.nodes, "surfaces.comments").runtime.structural, true);

  const edge = (kind: string, from?: string) => {
    const found = model.edges.find((candidate) =>
      candidate.kind === kind && (from === undefined || candidate.from === from));
    assert.ok(found, `missing ${kind} edge from ${from ?? "anywhere"}`);
    return found;
  };
  assert.equal(edge("handles", "pages.feed/event/like").activity, "taken");
  assert.equal(edge("guard-reads").activity, "context");
  assert.equal(edge("body-reads").activity, "context");
  assert.equal(edge("writes").activity, "taken");
  assert.equal(edge("sends").activity, "taken");
  assert.equal(edge("opens").activity, "taken");
  assert.equal(edge("navigates").activity, "taken");
  assert.equal(edge("settles").activity, "idle");
});

test("maps surface dispatch, guard outcomes, and instance structural IDs", () => {
  const trace: StepTrace = {
    event: { kind: "ui" },
    dispatch: {
      scope: "surface:2",
      definition: "comments",
      on: "close",
      guards: [{ handler: 0, guard: "not-ready" }],
      selected: null,
      aborted: "projection-not-ready",
    },
    structural: [{ op: "dismiss", surface: "comments:2", top: true }],
    drop: "projection-not-ready",
    "u-hash": "u-4",
    "v-hash": "v-4",
  };
  const model = deriveDebugGraph(state(trace));

  assert.equal(runtimeDefinitionIdForTrace(trace), "surfaces.comments");
  assert.equal(model.focusDefinitionId, "surfaces.comments");
  assert.equal(model.runtimeDefinitionId, "surfaces.comments");
  assert.equal(
    byId(model.definitions, "surfaces.comments").transitionTarget,
    true,
    "runtime instance suffix is removed before static ID correlation",
  );
  const handler = byId(model.nodes, "surfaces.comments/handler/0");
  assert.equal(handler.runtime.consulted, "not-ready");
  assert.equal(handler.runtime.selected, false);
  assert.equal(byId(model.nodes, "surfaces.comments/event/close").runtime.current, true);
  const handles = model.edges.find((edge) => edge.kind === "handles");
  assert.equal(handles?.activity, "context");
});

test("resolves duplicate page instances by dispatch serial and scopes pending commands", () => {
  const inspect = snapshot();
  inspect.u.nav = [
    {
      serial: 1,
      route: "feed",
      params: {},
      state: { liked: true, pending: true },
    },
    { serial: 2, route: "detail", params: {}, state: {} },
    {
      serial: 3,
      route: "feed",
      params: {},
      state: { liked: false, pending: false },
    },
  ];
  inspect.u.pending = {
    "c-1": {
      port: "feed",
      command: "like-post",
      payload: { post: "first" },
      origin: "page:1",
    },
    "c-2": {
      port: "feed",
      command: "like-post",
      payload: { post: "second" },
      origin: "page:3",
    },
  };

  const dispatched = deriveDebugGraph(state(feedTrace(), inspect));
  assert.equal(
    byId(dispatched.nodes, "pages.feed/state/liked").detail,
    "true",
    "dispatch scope page:1 selects serial 1 instead of the top duplicate",
  );
  assert.equal(byId(dispatched.nodes, "commands.like-post").detail, "1 pending");
  assert.equal(byId(dispatched.nodes, "commands.like-post").runtime.pending, 1);

  const delivery: StepTrace = {
    event: { kind: "projection" },
    "u-hash": "u-4",
    "v-hash": "v-4",
  };
  const pinned = deriveDebugGraph(state(delivery, inspect), {
    focusDefinitionId: "pages.feed",
  });
  assert.equal(
    byId(pinned.nodes, "pages.feed/state/liked").detail,
    "false",
    "without a dispatch, a pinned definition observes its topmost instance",
  );
  assert.equal(byId(pinned.nodes, "commands.like-post").detail, "1 pending");

  const removed = snapshot();
  removed.u.nav = [
    {
      serial: 3,
      route: "feed",
      params: {},
      state: { liked: true, pending: false },
    },
  ];
  removed.u.pending = inspect.u.pending;
  const afterRemoval = deriveDebugGraph(state(feedTrace(), removed));
  assert.equal(
    byId(afterRemoval.nodes, "pages.feed/state/liked").detail,
    "Initial · false",
    "a removed dispatch origin never falls through to another duplicate",
  );
  assert.equal(
    byId(afterRemoval.nodes, "commands.like-post").detail,
    "1 pending",
    "pending work remains attributable to the removed origin scope",
  );
});

test("resolves duplicate surface instances and their pending origins independently", () => {
  const inspect = snapshot();
  inspect.u.surfaces = [
    {
      serial: 2,
      definition: "comments",
      props: {},
      state: { draft: "first" },
      opener: "page:1",
    },
    {
      serial: 3,
      definition: "comments",
      props: {},
      state: { draft: "second" },
      opener: "page:1",
    },
  ];
  inspect.u.pending = {
    "c-2": {
      port: "comments",
      command: "submit-comment",
      payload: { text: "first" },
      origin: "surface:2",
    },
    "c-3": {
      port: "comments",
      command: "submit-comment",
      payload: { text: "second" },
      origin: "surface:3",
    },
  };
  const trace: StepTrace = {
    event: { kind: "ui" },
    dispatch: {
      scope: "surface:2",
      definition: "comments",
      on: "close",
      guards: [{ handler: 0, guard: "satisfied" }],
      selected: 0,
      writes: [{ field: "draft", value: "first" }],
    },
    c: [
      {
        kind: "command",
        port: "comments",
        command: "submit-comment",
        correlation: "c-2",
        payload: { text: "first" },
      },
    ],
    "u-hash": "u-4",
    "v-hash": "v-4",
  };

  const dispatched = deriveDebugGraph(state(trace, inspect));
  assert.equal(dispatched.focusDefinitionId, "surfaces.comments");
  assert.equal(
    byId(dispatched.nodes, "surfaces.comments/state/draft").detail,
    '"first"',
  );
  assert.equal(
    byId(dispatched.nodes, "commands.submit-comment").detail,
    "1 pending",
  );
  assert.equal(
    byId(dispatched.nodes, "commands.submit-comment").runtime.sent,
    true,
  );

  const delivery: StepTrace = {
    event: { kind: "projection" },
    "u-hash": "u-4",
    "v-hash": "v-4",
  };
  const pinned = deriveDebugGraph(state(delivery, inspect), {
    focusDefinitionId: "surfaces.comments",
  });
  assert.equal(
    byId(pinned.nodes, "surfaces.comments/state/draft").detail,
    '"second"',
  );
  assert.equal(
    byId(pinned.nodes, "commands.submit-comment").detail,
    "1 pending",
  );
});

test("explicit valid focus pins topology while labels and runtime marks advance", () => {
  const first = deriveDebugGraph(state(), { focusDefinitionId: "pages.feed" });
  const nextSnapshot = snapshot(5);
  nextSnapshot.u.nav[0]!.state.liked = false;
  nextSnapshot.u.pending = {};
  nextSnapshot.x.failed = [];
  const delivery: StepTrace = {
    event: { kind: "projection" },
    applies: [
      { apply: "dropped-stale", projection: "feed-page", revision: 2, current: 3 },
    ],
    "u-hash": "u-5",
    "v-hash": "v-5",
  };
  const second = deriveDebugGraph(state(delivery, nextSnapshot), {
    focusDefinitionId: "pages.feed",
  });

  assert.deepEqual(
    second.nodes.map((node) => ({ id: node.id, lane: node.lane, order: node.order })),
    first.nodes.map((node) => ({ id: node.id, lane: node.lane, order: node.order })),
  );
  assert.deepEqual(
    second.edges.map((edge) => ({ id: edge.id, from: edge.from, to: edge.to })),
    first.edges.map((edge) => ({ id: edge.id, from: edge.from, to: edge.to })),
  );
  assert.equal(byId(second.nodes, "pages.feed/state/liked").detail, "false");
  assert.equal(byId(second.nodes, "commands.like-post").detail, "Command · feed");
  assert.equal(
    byId(second.nodes, "projections.feed-page").runtime.projectionApply,
    "dropped-stale",
  );
  assert.equal(byId(second.nodes, "pages.feed/handler/0").runtime.current, false);
});

test("reports loading/disposed/no-program states and falls back from invalid focus", () => {
  const loading: InspectionState = {
    disposed: false,
    historyLimit: 128,
    artifacts: null,
    latest: null,
    history: [],
    evictedSteps: 0,
  };
  assert.equal(deriveDebugGraph(loading).emptyReason, "loading");
  assert.equal(
    deriveDebugGraph({ ...loading, disposed: true }).emptyReason,
    "disposed",
  );

  const boot = state(null);
  const model = deriveDebugGraph(boot, { focusDefinitionId: "pages.missing" });
  assert.equal(model.focusDefinitionId, "pages.feed");
  assert.equal(model.runtimeDefinitionId, null);
  assert.equal(model.revision, null);
});

test("debug values are canonical and bounded without affecting geometry", () => {
  assert.equal(formatDebugValue({ z: 1, a: [true, null] }), '{"a":[true,null],"z":1}');
  assert.equal(formatDebugValue("abcdefgh", 6), '"abcd…');
  assert.equal(formatDebugValue(undefined), "unset");
});
