import assert from "node:assert/strict";
import { test } from "vitest";

import type { PreparedAuthoring, PreviewOccurrence } from "../editor-authoring.js";
import {
  AnnotationOverlay,
  composedParent,
  validateAnnotationRealizations,
} from "../annotation-overlay.js";
import { RealizationResources } from "../editor-realization.js";
import type { RenderNodeRef, SourceTarget } from "../editor-state.js";

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
