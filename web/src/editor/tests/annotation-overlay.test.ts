import assert from "node:assert/strict";
import { test } from "vitest";

import type { PreparedAuthoring, PreviewOccurrence } from "../editor-authoring.js";
import {
  AnnotationOverlay,
  composedParent,
  validateAnnotationRealizations,
} from "../annotation-overlay.js";
import { RealizationResources } from "../editor-realization.js";
import type {
  RenderNodeRef,
  SourceMetadataEntry,
  SourceTarget,
} from "../editor-state.js";

const sourceTarget: SourceTarget = {
  id: "target",
  class: "catalog-element",
  file: "example.uhura",
  span: {
    offset: 0,
    len: 4,
    start: { line: 1, col: 1 },
    end: { line: 1, col: 5 },
  },
  label: "button",
  owner: { kind: "component", name: "card" },
};

const anchor: RenderNodeRef = { root: { kind: "fragment" }, path: [0, 2] };
const occurrence: PreviewOccurrence = {
  previewId: "preview",
  occurrence: { id: "occurrence", targetId: sourceTarget.id, anchors: [anchor] },
};
const authoring: PreparedAuthoring = {
  targetsById: new Map([[sourceTarget.id, sourceTarget]]),
  entriesById: new Map(),
  entriesByTarget: new Map(),
  occurrencesByTarget: new Map([[sourceTarget.id, [occurrence]]]),
  annotationTargets: [],
  documentedTargets: [],
};

class OverlayTestClassList {
  readonly #owner: OverlayTestElement;

  constructor(owner: OverlayTestElement) {
    this.#owner = owner;
  }

  add(...names: string[]): void {
    const current = new Set(this.#owner.className.split(/\s+/).filter(Boolean));
    for (const name of names) current.add(name);
    this.#owner.className = [...current].join(" ");
  }

  toggle(name: string, force?: boolean): boolean {
    const current = new Set(this.#owner.className.split(/\s+/).filter(Boolean));
    const enabled = force ?? !current.has(name);
    if (enabled) current.add(name);
    else current.delete(name);
    this.#owner.className = [...current].join(" ");
    return enabled;
  }

  contains(name: string): boolean {
    return this.#owner.className.split(/\s+/).includes(name);
  }
}

class OverlayTestElement {
  readonly nodeType = 1;
  readonly ownerDocument: OverlayTestDocument;
  readonly tagName: string;
  readonly classList: OverlayTestClassList;
  readonly style = { display: "", transform: "" };
  readonly children: OverlayTestElement[] = [];
  readonly attributes = new Map<string, string>();
  readonly #listeners = new Map<string, EventListenerOrEventListenerObject[]>();
  parentNode: OverlayTestElement | null = null;
  className = "";
  textContent = "";
  id = "";
  type = "";
  title = "";
  tabIndex = -1;
  disabled = false;
  hidden = false;
  scrollLeft = 0;
  scrollTop = 0;
  offsetWidth = 0;
  offsetHeight = 0;
  rect = { left: 0, top: 0, width: 0, height: 0 };

  constructor(ownerDocument: OverlayTestDocument, tagName: string) {
    this.ownerDocument = ownerDocument;
    this.tagName = tagName.toUpperCase();
    this.classList = new OverlayTestClassList(this);
  }

  get isConnected(): boolean {
    return true;
  }

  append(...nodes: OverlayTestElement[]): void {
    for (const node of nodes) {
      if (node.parentNode) {
        const index = node.parentNode.children.indexOf(node);
        if (index >= 0) node.parentNode.children.splice(index, 1);
      }
      node.parentNode = this;
      this.children.push(node);
    }
  }

  replaceChildren(...nodes: OverlayTestElement[]): void {
    for (const child of this.children) child.parentNode = null;
    this.children.length = 0;
    this.append(...nodes);
  }

  descendants(): OverlayTestElement[] {
    return this.children.flatMap((child) => [child, ...child.descendants()]);
  }

  contains(candidate: OverlayTestElement): boolean {
    return this === candidate || this.children.some((child) => child.contains(candidate));
  }

  setAttribute(name: string, value: string): void {
    this.attributes.set(name, String(value));
  }

  getAttribute(name: string): string | null {
    return this.attributes.get(name) ?? null;
  }

  addEventListener(type: string, listener: EventListenerOrEventListenerObject): void {
    const current = this.#listeners.get(type) ?? [];
    current.push(listener);
    this.#listeners.set(type, current);
  }

  removeEventListener(type: string, listener: EventListenerOrEventListenerObject): void {
    const current = this.#listeners.get(type);
    if (!current) return;
    const index = current.indexOf(listener);
    if (index >= 0) current.splice(index, 1);
  }

  focus(_options?: FocusOptions): void {
    this.ownerDocument.activeElement = this;
    const event = { type: "focus", stopPropagation: () => {} } as Event;
    for (const listener of this.#listeners.get("focus") ?? []) {
      if (typeof listener === "function") listener.call(this, event);
      else listener.handleEvent(event);
    }
  }

  getBoundingClientRect(): DOMRect {
    return this.rect as DOMRect;
  }
}

class OverlayTestDocument {
  activeElement: OverlayTestElement | null = null;
  readonly #frames = new Map<number, FrameRequestCallback>();
  #nextFrame = 1;
  readonly defaultView = {
    navigator: { clipboard: undefined },
    requestAnimationFrame: (callback: FrameRequestCallback): number => {
      const frame = this.#nextFrame++;
      this.#frames.set(frame, callback);
      return frame;
    },
    cancelAnimationFrame: (frame: number): void => {
      this.#frames.delete(frame);
    },
    getComputedStyle: (_element: Element): CSSStyleDeclaration => ({
      overflow: "visible",
      overflowX: "visible",
      overflowY: "visible",
    }) as CSSStyleDeclaration,
  };

  createElement(tagName: string): OverlayTestElement {
    return new OverlayTestElement(this, tagName);
  }

  createElementNS(_namespace: string, tagName: string): OverlayTestElement {
    return this.createElement(tagName);
  }

  flushAnimationFrames(): void {
    let remaining = 10;
    while (this.#frames.size > 0 && remaining > 0) {
      const pending = [...this.#frames.entries()];
      this.#frames.clear();
      for (const [, callback] of pending) callback(0);
      remaining -= 1;
    }
    assert.ok(remaining > 0, "annotation layout animation frames settled");
  }
}

const overlayElements = (
  root: OverlayTestElement,
  className: string,
): OverlayTestElement[] => [root, ...root.descendants()].filter(
  (candidate) => candidate.classList.contains(className),
);

test("validates every protocol anchor against its direct realization registry", () => {
  const resources = new RealizationResources();
  resources.claim({});
  resources.register({ root: anchor.root, path: anchor.path, element: {} as HTMLElement });
  assert.doesNotThrow(() => validateAnnotationRealizations({
    render: null,
    authoring,
    resourcesByPreviewId: new Map([[occurrence.previewId, resources]]),
  }));

  const incomplete = new RealizationResources();
  incomplete.claim({});
  assert.throws(
    () => validateAnnotationRealizations({
      render: null,
      authoring,
      resourcesByPreviewId: new Map([[occurrence.previewId, incomplete]]),
    }),
    /internal error.*target.*occurrence.*preview.*fragment\|0\.2.*did not register/s,
  );
  assert.throws(
    () => validateAnnotationRealizations({
      render: null,
      authoring,
      resourcesByPreviewId: new Map(),
    }),
    /internal error.*target.*occurrence.*preview.*no direct realization resources/s,
  );
});

test("composed parent traversal reaches a ShadowRoot host", () => {
  const host = { nodeType: 1, parentNode: null } as unknown as Node;
  const shadow = { nodeType: 11, parentNode: null, host } as unknown as Node;
  const child = { nodeType: 1, parentNode: shadow } as unknown as Node;
  assert.equal(composedParent(shadow), host);
  assert.equal(composedParent(child), host);
});

test("pins the overlay plane across programmatic viewport scrolling", () => {
  let scrollListener: EventListenerOrEventListenerObject | null = null;
  let removedListener: EventListenerOrEventListenerObject | null = null;
  let requestedFrames = 0;
  let cancelledFrame = 0;
  const viewport = {
    scrollLeft: 17,
    scrollTop: 311,
    addEventListener: (
      type: string,
      listener: EventListenerOrEventListenerObject,
      options?: AddEventListenerOptions | boolean,
    ) => {
      assert.equal(type, "scroll");
      assert.deepEqual(options, { passive: true });
      scrollListener = listener;
    },
    removeEventListener: (type: string, listener: EventListenerOrEventListenerObject) => {
      assert.equal(type, "scroll");
      removedListener = listener;
      if (scrollListener === listener) scrollListener = null;
    },
  };
  const style = { transform: "initial" };
  const root = {
    ownerDocument: {
      defaultView: {
        requestAnimationFrame: (_callback: FrameRequestCallback): number => {
          requestedFrames += 1;
          return requestedFrames;
        },
        cancelAnimationFrame: (frame: number): void => {
          cancelledFrame = frame;
        },
      },
    },
    style,
    replaceChildren: () => {},
  };
  const overlay = new AnnotationOverlay({
    viewport: viewport as unknown as HTMLElement,
    root: root as unknown as HTMLElement,
    focusPreview: () => {},
  });

  assert.equal(style.transform, "translate(17px, 311px)");
  const registeredListener = scrollListener as unknown as EventListenerOrEventListenerObject;
  assert.ok(registeredListener);
  viewport.scrollLeft = 29;
  viewport.scrollTop = 407;
  if (typeof registeredListener === "function") {
    registeredListener.call(viewport, { type: "scroll" } as Event);
  } else {
    registeredListener.handleEvent({ type: "scroll" } as Event);
  }
  assert.equal(style.transform, "translate(29px, 407px)");
  assert.equal(requestedFrames, 1, "scrolling also invalidates anchor layout");

  overlay.dispose();
  assert.equal(removedListener, registeredListener);
  assert.equal(scrollListener, null);
  assert.equal(cancelledFrame, 1);
  assert.equal(style.transform, "");
});

test("focused preview filters presentation without becoming annotation selection", () => {
  const document = new OverlayTestDocument();
  const viewport = document.createElement("div");
  viewport.rect = { left: 0, top: 0, width: 800, height: 600 };
  const root = document.createElement("div");
  const firstTarget = document.createElement("button");
  firstTarget.rect = { left: 80, top: 100, width: 120, height: 36 };
  const secondTarget = document.createElement("button");
  secondTarget.rect = { left: 420, top: 220, width: 120, height: 36 };
  viewport.append(firstTarget, secondTarget);

  const entry: SourceMetadataEntry = {
    id: "annotation",
    class: "annotation",
    kind: "annotation",
    text: "Important action",
    span: sourceTarget.span,
    targetId: sourceTarget.id,
    order: 0,
  };
  const firstOccurrence: PreviewOccurrence = {
    previewId: "preview/first",
    occurrence: {
      id: "occurrence/first",
      targetId: sourceTarget.id,
      anchors: [anchor],
    },
  };
  const secondOccurrence: PreviewOccurrence = {
    previewId: "preview/second",
    occurrence: {
      id: "occurrence/second",
      targetId: sourceTarget.id,
      anchors: [anchor],
    },
  };
  const focusedAuthoring: PreparedAuthoring = {
    targetsById: new Map([[sourceTarget.id, sourceTarget]]),
    entriesById: new Map([[entry.id, entry]]),
    entriesByTarget: new Map([[sourceTarget.id, [entry]]]),
    occurrencesByTarget: new Map([
      [sourceTarget.id, [firstOccurrence, secondOccurrence]],
    ]),
    annotationTargets: [{
      target: sourceTarget,
      entries: [entry],
      occurrences: [firstOccurrence, secondOccurrence],
      sourceOrder: 0,
    }],
    documentedTargets: [],
  };
  const firstResources = new RealizationResources();
  firstResources.claim({});
  firstResources.register({
    root: anchor.root,
    path: anchor.path,
    element: firstTarget as unknown as HTMLElement,
  });
  const secondResources = new RealizationResources();
  secondResources.claim({});
  secondResources.register({
    root: anchor.root,
    path: anchor.path,
    element: secondTarget as unknown as HTMLElement,
  });
  const overlay = new AnnotationOverlay({
    viewport: viewport as unknown as HTMLElement,
    root: root as unknown as HTMLElement,
    focusPreview: () => {},
  });
  overlay.install({
    render: null,
    authoring: focusedAuthoring,
    resourcesByPreviewId: new Map([
      [firstOccurrence.previewId, firstResources],
      [secondOccurrence.previewId, secondResources],
    ]),
  });
  document.flushAnimationFrames();

  const markers = overlayElements(root, "annotation-marker");
  const highlights = overlayElements(root, "annotation-highlight");
  const lines = root.descendants().filter((candidate) => candidate.tagName === "LINE");
  const cards = overlayElements(root, "annotation-card");
  assert.equal(markers.length, 2);
  assert.equal(highlights.length, 2);
  assert.equal(cards.length, 1);
  assert.deepEqual(markers.map((marker) => marker.hidden), [false, false]);
  assert.deepEqual(highlights.map((highlight) => highlight.style.display), ["none", "none"]);

  overlay.activatePreviewOccurrences(firstOccurrence.previewId);
  document.flushAnimationFrames();
  assert.deepEqual(highlights.map((highlight) => highlight.style.display), ["", "none"]);

  overlay.setFocusedPreview(secondOccurrence.previewId);
  document.flushAnimationFrames();
  assert.deepEqual(markers.map((marker) => marker.hidden), [true, false]);
  assert.deepEqual(
    markers.map((marker) => marker.classList.contains("is-preview-active")),
    [true, false],
    "focus filtering does not replace preview selection",
  );
  assert.deepEqual(
    highlights.map((highlight) => highlight.style.display),
    ["none", "none"],
    "focus alone introduces no annotation outline",
  );
  assert.equal(cards[0]?.hidden, false, "focus expands the annotation card");
  assert.deepEqual(lines.map((line) => line.style.display), ["none", ""]);
  assert.deepEqual(markers.map((marker) => marker.getAttribute("aria-expanded")), [
    "false",
    "true",
  ]);

  overlay.activatePreviewOccurrences(secondOccurrence.previewId);
  document.flushAnimationFrames();
  assert.deepEqual(highlights.map((highlight) => highlight.style.display), ["none", ""]);
  markers[1]?.focus();
  document.flushAnimationFrames();
  assert.equal(cards[0]?.hidden, false);

  overlay.setFocusedPreview(firstOccurrence.previewId);
  document.flushAnimationFrames();
  assert.deepEqual(markers.map((marker) => marker.hidden), [false, true]);
  assert.ok(highlights.every((highlight) => highlight.style.display === "none"));
  assert.deepEqual(lines.map((line) => line.style.display), ["", "none"]);
  assert.equal(cards[0]?.hidden, false, "focus reanchors the card to its visible occurrence");
  assert.deepEqual(markers.map((marker) => marker.getAttribute("aria-expanded")), [
    "true",
    "false",
  ]);

  overlay.setFocusedPreview(null);
  document.flushAnimationFrames();
  assert.equal(cards[0]?.hidden, false, "leaving focus restores the manual occurrence");
  assert.deepEqual(lines.map((line) => line.style.display), ["none", ""]);
  overlay.setFocusedPreview(firstOccurrence.previewId);
  document.flushAnimationFrames();

  overlay.dismissCards();
  document.flushAnimationFrames();
  assert.equal(cards[0]?.hidden, false, "focused annotations remain automatically expanded");

  overlay.toggleCanvasVisibility();
  document.flushAnimationFrames();
  assert.equal(cards[0]?.hidden, true);
  assert.ok(markers.every((marker) => marker.hidden));
  overlay.toggleCanvasVisibility();
  document.flushAnimationFrames();
  assert.equal(cards[0]?.hidden, false, "explicitly showing comments restores focus cards");

  overlay.setFocusedPreview(null);
  document.flushAnimationFrames();
  assert.deepEqual(markers.map((marker) => marker.hidden), [false, false]);
  assert.deepEqual(highlights.map((highlight) => highlight.style.display), ["none", ""]);
  assert.equal(cards[0]?.hidden, true, "leaving focus restores dismissed normal-canvas state");

  overlay.dispose();
});

test("focused preview expands one collision-aware card per annotated target", () => {
  const document = new OverlayTestDocument();
  const viewport = document.createElement("div");
  viewport.rect = { left: 0, top: 0, width: 800, height: 600 };
  const root = document.createElement("div");
  const firstElement = document.createElement("button");
  firstElement.rect = { left: 100, top: 120, width: 120, height: 36 };
  const secondElement = document.createElement("button");
  secondElement.rect = { left: 430, top: 300, width: 120, height: 36 };
  viewport.append(firstElement, secondElement);

  const secondTarget: SourceTarget = {
    ...sourceTarget,
    id: "target/secondary",
    label: "secondary button",
  };
  const secondAnchor: RenderNodeRef = { root: { kind: "fragment" }, path: [0, 4] };
  const entries: SourceMetadataEntry[] = [
    {
      id: "annotation/first",
      class: "annotation",
      kind: "doc",
      text: "Primary action",
      span: sourceTarget.span,
      targetId: sourceTarget.id,
      order: 0,
    },
    {
      id: "annotation/second",
      class: "annotation",
      kind: "annotation",
      text: "Secondary action",
      span: secondTarget.span,
      targetId: secondTarget.id,
      order: 1,
    },
  ];
  const occurrences: PreviewOccurrence[] = [
    occurrence,
    {
      previewId: occurrence.previewId,
      occurrence: {
        id: "occurrence/secondary",
        targetId: secondTarget.id,
        anchors: [secondAnchor],
      },
    },
  ];
  const focusedAuthoring: PreparedAuthoring = {
    targetsById: new Map([
      [sourceTarget.id, sourceTarget],
      [secondTarget.id, secondTarget],
    ]),
    entriesById: new Map(entries.map((entry) => [entry.id, entry])),
    entriesByTarget: new Map([
      [sourceTarget.id, [entries[0]!]],
      [secondTarget.id, [entries[1]!]],
    ]),
    occurrencesByTarget: new Map([
      [sourceTarget.id, [occurrences[0]!]],
      [secondTarget.id, [occurrences[1]!]],
    ]),
    annotationTargets: [
      {
        target: sourceTarget,
        entries: [entries[0]!],
        occurrences: [occurrences[0]!],
        sourceOrder: 0,
      },
      {
        target: secondTarget,
        entries: [entries[1]!],
        occurrences: [occurrences[1]!],
        sourceOrder: 1,
      },
    ],
    documentedTargets: [],
  };
  const resources = new RealizationResources();
  resources.claim({});
  resources.register({
    root: anchor.root,
    path: anchor.path,
    element: firstElement as unknown as HTMLElement,
  });
  resources.register({
    root: secondAnchor.root,
    path: secondAnchor.path,
    element: secondElement as unknown as HTMLElement,
  });
  const overlay = new AnnotationOverlay({
    viewport: viewport as unknown as HTMLElement,
    root: root as unknown as HTMLElement,
    focusPreview: () => {},
  });
  overlay.install({
    render: null,
    authoring: focusedAuthoring,
    resourcesByPreviewId: new Map([[occurrence.previewId, resources]]),
  });
  document.flushAnimationFrames();

  const markers = overlayElements(root, "annotation-marker");
  const cards = overlayElements(root, "annotation-card");
  const lines = root.descendants().filter((candidate) => candidate.tagName === "LINE");
  assert.deepEqual(cards.map((card) => card.hidden), [true, true]);

  overlay.setFocusedPreview(occurrence.previewId);
  document.flushAnimationFrames();
  assert.deepEqual(cards.map((card) => card.hidden), [false, false]);
  assert.deepEqual(lines.map((line) => line.style.display), ["", ""]);
  assert.deepEqual(markers.map((marker) => marker.getAttribute("aria-expanded")), [
    "true",
    "true",
  ]);
  assert.notEqual(cards[0]?.style.transform, cards[1]?.style.transform);

  overlay.dismissCards();
  document.flushAnimationFrames();
  assert.deepEqual(cards.map((card) => card.hidden), [false, false]);
  overlay.setFocusedPreview(null);
  document.flushAnimationFrames();
  assert.deepEqual(cards.map((card) => card.hidden), [true, true]);

  overlay.dispose();
});
