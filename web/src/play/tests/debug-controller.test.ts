import assert from "node:assert/strict";
import { test } from "vitest";

import type {
  RuntimeInspectionHandle,
  RuntimeInspectionListener,
  RuntimeInspectionState,
} from "../../protocol/types.js";
import {
  createDebugController,
  type DebugController,
  type DebugControllerUpdate,
} from "../debug-controller.js";

function inspectionState(
  marker: number,
  disposed = false,
): RuntimeInspectionState {
  return Object.freeze({
    protocol: "uhura-runtime-inspection-state/0",
    disposed,
    historyLimit: 128,
    artifacts: null,
    latest: null,
    history: Object.freeze([]),
    evictedSteps: marker,
  });
}

class FakeInspection {
  state: RuntimeInspectionState;
  readonly listeners = new Set<RuntimeInspectionListener>();
  subscribeCalls = 0;
  unsubscribeCalls = 0;
  afterReplay: (() => void) | null = null;

  constructor(state: RuntimeInspectionState) {
    this.state = state;
  }

  readonly handle: RuntimeInspectionHandle = {
    get state(): RuntimeInspectionState {
      throw new Error("the controller must rely on subscribe's atomic replay");
    },
    subscribe: (listener) => {
      this.subscribeCalls += 1;
      this.listeners.add(listener);
      listener(this.state);
      this.afterReplay?.();
      let subscribed = true;
      return () => {
        if (!subscribed) return;
        subscribed = false;
        this.unsubscribeCalls += 1;
        this.listeners.delete(listener);
      };
    },
  };

  publish(state: RuntimeInspectionState): void {
    this.state = state;
    for (const listener of [...this.listeners]) {
      listener(state);
    }
  }
}

function frameScheduler() {
  let nextHandle = 1;
  const callbacks = new Map<number, () => void>();
  const canceled: number[] = [];
  return {
    request(callback: () => void): number {
      const handle = nextHandle++;
      callbacks.set(handle, callback);
      return handle;
    },
    cancel(handle: number): void {
      canceled.push(handle);
      callbacks.delete(handle);
    },
    flush(): void {
      const scheduled = [...callbacks.values()];
      callbacks.clear();
      for (const callback of scheduled) callback();
    },
    get size() {
      return callbacks.size;
    },
    canceled,
  };
}

test("subscribes lazily, coalesces to the latest state, and replays on reopen", () => {
  const first = inspectionState(1);
  const second = inspectionState(2);
  const third = inspectionState(3);
  const inspection = new FakeInspection(first);
  const frames = frameScheduler();
  const rendered: DebugControllerUpdate[] = [];
  let resolutions = 0;
  const controller = createDebugController({
    resolveInspection: () => {
      resolutions += 1;
      return inspection.handle;
    },
    requestFrame: frames.request,
    cancelFrame: frames.cancel,
    render: (update) => rendered.push(update),
  });

  assert.equal(controller.isOpen, false);
  assert.equal(resolutions, 0);
  assert.equal(inspection.subscribeCalls, 0);

  assert.equal(controller.open(), true);
  assert.equal(controller.open(), false);
  assert.equal(resolutions, 1);
  assert.equal(inspection.subscribeCalls, 1);
  assert.equal(inspection.listeners.size, 1);
  assert.equal(frames.size, 1, "the synchronous replay schedules one frame");

  inspection.publish(second);
  assert.equal(frames.size, 1, "a burst retains only one scheduled frame");
  frames.flush();
  assert.deepEqual(rendered, [{
    kind: "inspection",
    publication: second,
  }]);

  assert.equal(controller.close(), true);
  assert.equal(controller.close(), false);
  assert.equal(controller.isOpen, false);
  assert.equal(inspection.unsubscribeCalls, 1);
  assert.equal(inspection.listeners.size, 0);
  inspection.publish(third);
  frames.flush();
  assert.equal(rendered.length, 1, "closed controllers ignore publications");

  assert.equal(controller.open(), true);
  assert.equal(resolutions, 2);
  assert.equal(inspection.subscribeCalls, 2);
  frames.flush();
  assert.deepEqual(rendered.at(-1), {
    kind: "inspection",
    publication: third,
  });
});

test("cancels pending work, unsubscribes, and permanently retires on dispose", () => {
  const inspection = new FakeInspection(inspectionState(1));
  const frames = frameScheduler();
  const rendered: DebugControllerUpdate[] = [];
  const controller = createDebugController({
    resolveInspection: () => inspection.handle,
    requestFrame: frames.request,
    cancelFrame: frames.cancel,
    render: (update) => rendered.push(update),
  });

  controller.open();
  inspection.publish(inspectionState(2));
  assert.equal(frames.size, 1);
  assert.equal(controller.dispose(), true);
  assert.equal(controller.dispose(), false);
  assert.equal(controller.close(), false);
  assert.equal(controller.open(), false);
  assert.equal(controller.isOpen, false);
  assert.equal(controller.isDisposed, true);
  assert.equal(inspection.listeners.size, 0);
  assert.equal(inspection.unsubscribeCalls, 1);
  assert.deepEqual(frames.canceled, [1]);

  inspection.publish(inspectionState(3));
  frames.flush();
  assert.deepEqual(rendered, []);
});

test("does not leak an unsubscribe returned after synchronous replay closes it", () => {
  const inspection = new FakeInspection(inspectionState(1));
  const frames = frameScheduler();
  let controller: DebugController;
  inspection.afterReplay = () => controller.close();
  controller = createDebugController({
    resolveInspection: () => inspection.handle,
    requestFrame: frames.request,
    cancelFrame: frames.cancel,
    render: () => {},
  });

  assert.equal(controller.open(), true);
  assert.equal(controller.isOpen, false);
  assert.equal(inspection.subscribeCalls, 1);
  assert.equal(inspection.unsubscribeCalls, 1);
  assert.equal(inspection.listeners.size, 0);
  assert.equal(frames.size, 0);
  assert.deepEqual(frames.canceled, [1]);
});

test("delivers unavailable and terminal disposed states without retaining a listener", () => {
  const frames = frameScheduler();
  const unavailable: DebugControllerUpdate[] = [];
  const missing = createDebugController({
    resolveInspection: () => null,
    requestFrame: frames.request,
    cancelFrame: frames.cancel,
    render: (update) => unavailable.push(update),
  });

  missing.open();
  frames.flush();
  assert.deepEqual(unavailable, [{ kind: "unavailable" }]);
  missing.close();

  const terminal = inspectionState(4, true);
  const inspection = new FakeInspection(terminal);
  const terminalFrames = frameScheduler();
  const rendered: DebugControllerUpdate[] = [];
  const controller = createDebugController({
    resolveInspection: () => inspection.handle,
    requestFrame: terminalFrames.request,
    cancelFrame: terminalFrames.cancel,
    render: (update) => rendered.push(update),
  });

  controller.open();
  assert.equal(controller.isOpen, true, "the unavailable panel remains visible");
  assert.equal(inspection.listeners.size, 0);
  assert.equal(inspection.unsubscribeCalls, 1);
  terminalFrames.flush();
  assert.deepEqual(rendered, [{
    kind: "inspection",
    publication: terminal,
  }]);
  assert.equal(controller.close(), true);
});

test("accepts a synchronous frame scheduler without retaining a stale frame", () => {
  const inspection = new FakeInspection(inspectionState(1));
  const rendered: DebugControllerUpdate[] = [];
  let cancelCalls = 0;
  const controller = createDebugController({
    resolveInspection: () => inspection.handle,
    requestFrame: (callback) => {
      callback();
      return 99;
    },
    cancelFrame: () => {
      cancelCalls += 1;
    },
    render: (update) => rendered.push(update),
  });

  controller.open();
  assert.equal(rendered.length, 1);
  controller.close();
  assert.equal(cancelCalls, 0);
  assert.equal(inspection.listeners.size, 0);
});

test("a canceled callback cannot consume a reopened controller's update", () => {
  const first = inspectionState(1);
  const second = inspectionState(2);
  const inspection = new FakeInspection(first);
  const callbacks: (() => void)[] = [];
  const canceled: number[] = [];
  const rendered: DebugControllerUpdate[] = [];
  const controller = createDebugController({
    resolveInspection: () => inspection.handle,
    requestFrame: (callback) => {
      callbacks.push(callback);
      return callbacks.length - 1;
    },
    // Deliberately retain the callback to model a scheduler that races or
    // fails cancellation; generation ownership must still reject it.
    cancelFrame: (handle) => canceled.push(handle),
    render: (update) => rendered.push(update),
  });

  controller.open();
  controller.close();
  inspection.publish(second);
  controller.open();
  assert.deepEqual(canceled, [0]);
  assert.equal(callbacks.length, 2);

  callbacks[0]?.();
  assert.deepEqual(rendered, []);
  callbacks[1]?.();
  assert.deepEqual(rendered, [{
    kind: "inspection",
    publication: second,
  }]);
});
