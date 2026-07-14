import assert from "node:assert/strict";
import { test } from "vitest";

import {
  PLAY_VIEWPORT_CONTENT,
  canConsumeHorizontalWheel,
  isHorizontalSwipeWheel,
  isPageScaleShortcut,
  lockPlayPageScale,
} from "../viewport-lock.js";

type Listener = EventListenerOrEventListenerObject;

class FakeMeta {
  readonly attributes = new Map<string, string>();
  name = "viewport";
  removed = false;

  getAttribute(name: string): string | null {
    return this.attributes.get(name) ?? null;
  }

  setAttribute(name: string, value: string): void {
    this.attributes.set(name, value);
  }

  removeAttribute(name: string): void {
    this.attributes.delete(name);
  }

  remove(): void {
    this.removed = true;
  }
}

class FakeRootStyle {
  private value = "";
  private priority = "";

  get overscrollBehaviorX(): string {
    return this.value;
  }

  getPropertyPriority(): string {
    return this.priority;
  }

  getPropertyValue(): string {
    return this.value;
  }

  removeProperty(): string {
    const prior = this.value;
    this.value = "";
    this.priority = "";
    return prior;
  }

  setProperty(_name: string, value: string, priority = ""): void {
    this.value = value;
    this.priority = priority;
  }
}

function fakeDocument(initial: FakeMeta | null): {
  document: Document;
  meta: FakeMeta;
  listeners: Map<string, Set<Listener>>;
  rootStyle: FakeRootStyle;
} {
  const listeners = new Map<string, Set<Listener>>();
  const meta = initial ?? new FakeMeta();
  let current: FakeMeta | null = initial;
  const rootStyle = new FakeRootStyle();
  const document = {
    defaultView: {
      getComputedStyle(candidate: { overflowX?: string }) {
        return { overflowX: candidate.overflowX ?? "visible" };
      },
    },
    documentElement: { style: rootStyle },
    head: {
      append(candidate: FakeMeta) {
        current = candidate;
      },
    },
    querySelector() {
      return current;
    },
    createElement() {
      return meta;
    },
    addEventListener(type: string, listener: Listener) {
      const registered = listeners.get(type) ?? new Set<Listener>();
      registered.add(listener);
      listeners.set(type, registered);
    },
    removeEventListener(type: string, listener: Listener) {
      listeners.get(type)?.delete(listener);
    },
  } as unknown as Document;
  return { document, meta, listeners, rootStyle };
}

function dispatch(
  listeners: Map<string, Set<Listener>>,
  type: string,
  fields: Record<string, unknown> = {},
): number {
  let prevented = 0;
  const event = {
    type,
    preventDefault() {
      prevented += 1;
    },
    ...fields,
  } as unknown as Event;
  for (const listener of listeners.get(type) ?? []) {
    if (typeof listener === "function") listener(event);
    else listener.handleEvent(event);
  }
  return prevented;
}

test("Play locks and restores the shared viewport contract", () => {
  const prior = new FakeMeta();
  prior.setAttribute("content", "width=device-width, initial-scale=1");
  const host = fakeDocument(prior);
  host.rootStyle.setProperty("overscroll-behavior-x", "contain", "important");

  const dispose = lockPlayPageScale(host.document);

  assert.equal(prior.getAttribute("content"), PLAY_VIEWPORT_CONTENT);
  assert.equal(host.rootStyle.overscrollBehaviorX, "none");
  assert.equal(host.rootStyle.getPropertyPriority(), "");
  assert.equal(dispatch(host.listeners, "wheel", { ctrlKey: true, metaKey: false }), 1);
  assert.equal(dispatch(host.listeners, "wheel", { ctrlKey: false, metaKey: false }), 0);
  assert.equal(dispatch(host.listeners, "wheel", {
    ctrlKey: false,
    metaKey: false,
    deltaX: 80,
    deltaY: 2,
    composedPath: () => [],
  }), 1);
  assert.equal(dispatch(host.listeners, "gesturestart"), 1);
  assert.equal(dispatch(host.listeners, "gesturechange"), 1);
  assert.equal(dispatch(host.listeners, "gestureend"), 1);
  assert.equal(dispatch(host.listeners, "keydown", {
    ctrlKey: true,
    metaKey: false,
    key: "+",
    code: "Equal",
  }), 1);

  dispose();
  dispose();

  assert.equal(prior.getAttribute("content"), "width=device-width, initial-scale=1");
  assert.equal(host.rootStyle.overscrollBehaviorX, "contain");
  assert.equal(host.rootStyle.getPropertyPriority(), "important");
  assert.equal(prior.removed, false);
  assert.equal([...host.listeners.values()].every((group) => group.size === 0), true);
});

test("a route-created viewport meta element is removed on cleanup", () => {
  const host = fakeDocument(null);
  const dispose = lockPlayPageScale(host.document);

  assert.equal(host.meta.getAttribute("content"), PLAY_VIEWPORT_CONTENT);
  dispose();
  assert.equal(host.meta.removed, true);
});

test("only browser scale shortcuts are blocked", () => {
  assert.equal(isPageScaleShortcut({
    ctrlKey: true,
    metaKey: false,
    key: "-",
    code: "Minus",
  }), true);
  assert.equal(isPageScaleShortcut({
    ctrlKey: false,
    metaKey: true,
    key: "Add",
    code: "NumpadAdd",
  }), true);
  assert.equal(isPageScaleShortcut({
    ctrlKey: true,
    metaKey: false,
    key: "r",
    code: "KeyR",
  }), false);
  assert.equal(isPageScaleShortcut({
    ctrlKey: false,
    metaKey: false,
    key: "+",
    code: "Equal",
  }), false);
});

test("horizontal swipe detection ignores vertical scrolling and pinch zoom", () => {
  assert.equal(isHorizontalSwipeWheel({
    ctrlKey: false,
    metaKey: false,
    deltaX: 40,
    deltaY: 4,
  }), true);
  assert.equal(isHorizontalSwipeWheel({
    ctrlKey: false,
    metaKey: false,
    deltaX: 4,
    deltaY: 40,
  }), false);
  assert.equal(isHorizontalSwipeWheel({
    ctrlKey: true,
    metaKey: false,
    deltaX: 40,
    deltaY: 0,
  }), false);
});

test("nested horizontal scrolling is preserved until it reaches an edge", () => {
  const state = { clientWidth: 400, scrollLeft: 100, scrollWidth: 1_000 };
  assert.equal(canConsumeHorizontalWheel(state, -50), true);
  assert.equal(canConsumeHorizontalWheel(state, 50), true);
  assert.equal(canConsumeHorizontalWheel({ ...state, scrollLeft: 0 }, -50), false);
  assert.equal(canConsumeHorizontalWheel({ ...state, scrollLeft: 0 }, 50), true);
  assert.equal(canConsumeHorizontalWheel({ ...state, scrollLeft: 600 }, 50), false);
  assert.equal(canConsumeHorizontalWheel({ ...state, scrollLeft: 600 }, -50), true);
});

test("wheel fallback lets a nested scroller consume horizontal movement", () => {
  const host = fakeDocument(new FakeMeta());
  const dispose = lockPlayPageScale(host.document);
  const scroller = {
    clientWidth: 400,
    overflowX: "auto",
    scrollLeft: 100,
    scrollWidth: 1_000,
  };

  assert.equal(dispatch(host.listeners, "wheel", {
    ctrlKey: false,
    metaKey: false,
    deltaX: 80,
    deltaY: 0,
    composedPath: () => [scroller],
  }), 0);
  assert.equal(dispatch(host.listeners, "wheel", {
    ctrlKey: false,
    metaKey: false,
    deltaX: 20,
    deltaY: 80,
    composedPath: () => [],
  }), 0);
  const childAtEdge = {
    clientWidth: 200,
    overflowX: "auto",
    scrollLeft: 0,
    scrollWidth: 200,
  };
  assert.equal(dispatch(host.listeners, "wheel", {
    ctrlKey: false,
    metaKey: false,
    deltaX: 80,
    deltaY: 0,
    composedPath: () => [childAtEdge, scroller],
  }), 0);
  scroller.scrollLeft = 600;
  assert.equal(dispatch(host.listeners, "wheel", {
    ctrlKey: false,
    metaKey: false,
    deltaX: 80,
    deltaY: 0,
    composedPath: () => [scroller],
  }), 1);
  scroller.scrollLeft = 0;
  assert.equal(dispatch(host.listeners, "wheel", {
    ctrlKey: false,
    metaKey: false,
    deltaX: -80,
    deltaY: 0,
    composedPath: () => [scroller],
  }), 1);
  const hiddenScroller = { ...scroller, overflowX: "hidden", scrollLeft: 100 };
  assert.equal(dispatch(host.listeners, "wheel", {
    ctrlKey: false,
    metaKey: false,
    deltaX: 80,
    deltaY: 0,
    composedPath: () => [hiddenScroller],
  }), 1);

  dispose();
});
