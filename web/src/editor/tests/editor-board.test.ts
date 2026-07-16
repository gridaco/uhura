import assert from "node:assert/strict";
import { test } from "vitest";

import {
  disposePreparedEditorModel,
  prepareEditorModel,
  reconcilePreparedEditorModel,
  setAnnotationConnectorsHidden,
  type PreparedEditorModel,
} from "../editor-board.js";
import { sourceActionsEnabled } from "../editor-authoring.js";
import {
  AnnotationOverlay,
  renderSourcePanel,
  validateAnnotationRealizations,
} from "../annotation-overlay.js";
import type {
  EditorRender,
  PreviewFreshness,
  RenderNodeRef,
  SourceMetadataEntry,
  SourceTarget,
} from "../editor-state.js";

class FakeStyle {
  backgroundImage = "";
  display = "";
  transform = "";
  readonly #properties = new Map<string, string>();

  setProperty(name: string, value: string): void {
    this.#properties.set(name, value);
  }
}

class FakeClassList {
  readonly #owner: FakeElement;

  constructor(owner: FakeElement) {
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

class FakeStyleSheet {
  readonly cssRules: readonly unknown[] = [];
  text = "";

  replaceSync(text: string): void {
    this.text = text;
  }
}

class FakeNode {
  parentNode: FakeContainer | null = null;
  readonly ownerDocument: FakeDocument;
  readonly nodeType: number;

  constructor(ownerDocument: FakeDocument, nodeType: number) {
    this.ownerDocument = ownerDocument;
    this.nodeType = nodeType;
  }

  get nextSibling(): FakeNode | null {
    if (!this.parentNode) return null;
    const index = this.parentNode.childNodes.indexOf(this);
    return this.parentNode.childNodes[index + 1] ?? null;
  }

  remove(): void {
    if (!this.parentNode) return;
    const index = this.parentNode.childNodes.indexOf(this);
    if (index >= 0) this.parentNode.childNodes.splice(index, 1);
    this.parentNode = null;
  }

  replaceWith(node: FakeNode): void {
    const parent = this.parentNode;
    if (!parent) return;
    const next = this.nextSibling;
    this.remove();
    parent.insertBefore(node, next);
  }

  getRootNode(): FakeNode {
    return this.parentNode?.getRootNode() ?? this;
  }
}

class FakeContainer extends FakeNode {
  readonly childNodes: FakeNode[] = [];

  get children(): readonly FakeElement[] {
    return this.childNodes.filter((node): node is FakeElement => node instanceof FakeElement);
  }

  get firstChild(): FakeNode | null {
    return this.childNodes[0] ?? null;
  }

  get firstElementChild(): FakeElement | null {
    return this.children[0] ?? null;
  }

  get lastElementChild(): FakeElement | null {
    return this.children.at(-1) ?? null;
  }

  append(...nodes: FakeNode[]): void {
    for (const node of nodes) {
      node.remove();
      node.parentNode = this;
      this.childNodes.push(node);
    }
  }

  replaceChildren(...nodes: FakeNode[]): void {
    for (const node of this.childNodes) node.parentNode = null;
    this.childNodes.length = 0;
    this.append(...nodes);
  }

  insertBefore(node: FakeNode, reference: FakeNode | null): FakeNode {
    node.remove();
    node.parentNode = this;
    const index = reference ? this.childNodes.indexOf(reference) : -1;
    if (index < 0) this.childNodes.push(node);
    else this.childNodes.splice(index, 0, node);
    return node;
  }

  contains(candidate: FakeNode): boolean {
    return this === candidate || this.childNodes.some((child) =>
      child === candidate
      || (child instanceof FakeContainer && child.contains(candidate))
    );
  }

  descendants(): FakeElement[] {
    return this.childNodes.flatMap((child) => child instanceof FakeElement
      ? [child, ...child.descendants()]
      : child instanceof FakeContainer
        ? child.descendants()
        : []);
  }
}

class FakeShadowRoot extends FakeContainer {
  adoptedStyleSheets: FakeStyleSheet[] = [];
  readonly host: FakeElement;

  constructor(ownerDocument: FakeDocument, host: FakeElement) {
    super(ownerDocument, 11);
    this.host = host;
  }

  override getRootNode(): FakeShadowRoot {
    return this;
  }
}

class FakeElement extends FakeContainer {
  readonly attributes = new Map<string, string>();
  readonly classList: FakeClassList;
  readonly dataset: Record<string, string> = {};
  readonly style = new FakeStyle();
  readonly tagName: string;
  readonly #listeners = new Map<string, EventListenerOrEventListenerObject[]>();
  shadowRoot: FakeShadowRoot | null = null;
  className = "";
  textContent = "";
  id = "";
  type = "";
  title = "";
  tabIndex = -1;
  inert = false;
  disabled = false;
  hidden = false;
  scrollLeft = 0;
  scrollTop = 0;

  constructor(ownerDocument: FakeDocument, tagName: string) {
    super(ownerDocument, 1);
    this.tagName = tagName.toUpperCase();
    this.classList = new FakeClassList(this);
  }

  setAttribute(name: string, value: string): void {
    this.attributes.set(name, String(value));
  }

  getAttribute(name: string): string | null {
    return this.attributes.get(name) ?? null;
  }

  hasAttribute(name: string): boolean {
    return this.attributes.has(name);
  }

  removeAttribute(name: string): void {
    this.attributes.delete(name);
  }

  attachShadow(_options: ShadowRootInit): FakeShadowRoot {
    if (this.shadowRoot) throw new Error("shadow root already attached");
    this.shadowRoot = new FakeShadowRoot(this.ownerDocument, this);
    return this.shadowRoot;
  }

  querySelector(_selector: string): null {
    return null;
  }

  addEventListener(type: string, listener: EventListenerOrEventListenerObject): void {
    const listeners = this.#listeners.get(type) ?? [];
    listeners.push(listener);
    this.#listeners.set(type, listeners);
  }

  removeEventListener(type: string, listener: EventListenerOrEventListenerObject): void {
    const listeners = this.#listeners.get(type);
    if (!listeners) return;
    const index = listeners.indexOf(listener);
    if (index >= 0) listeners.splice(index, 1);
  }

  click(): void {
    const event = { type: "click", stopPropagation: () => {} } as Event;
    for (const listener of this.#listeners.get("click") ?? []) {
      if (typeof listener === "function") listener.call(this, event);
      else listener.handleEvent(event);
    }
  }

  focus(_options?: FocusOptions): void {
    this.ownerDocument.activeElement = this;
    const event = { type: "focus", stopPropagation: () => {} } as Event;
    for (const listener of this.#listeners.get("focus") ?? []) {
      if (typeof listener === "function") listener.call(this, event);
      else listener.handleEvent(event);
    }
  }
}

class FakeDocumentFragment extends FakeContainer {
  constructor(ownerDocument: FakeDocument) {
    super(ownerDocument, 11);
  }
}

class FakeDocument {
  readonly defaultView = {
    CSSStyleSheet: FakeStyleSheet,
    navigator: { clipboard: undefined },
    requestAnimationFrame: (_callback: FrameRequestCallback): number => 1,
    cancelAnimationFrame: (_frame: number): void => {},
  };
  activeElement: FakeElement | null = null;

  createElement(tagName: string): FakeElement {
    return new FakeElement(this, tagName);
  }

  createElementNS(_namespace: string, tagName: string): FakeElement {
    return this.createElement(tagName);
  }

  createDocumentFragment(): FakeDocumentFragment {
    return new FakeDocumentFragment(this);
  }
}

const asDocument = (document: FakeDocument): Document => document as unknown as Document;
const asElement = (element: FakeElement): HTMLElement => element as unknown as HTMLElement;

const anchor: RenderNodeRef = { root: { kind: "fragment" }, path: [] };

const span = {
  offset: 24,
  len: 6,
  start: { line: 2, col: 3 },
  end: { line: 2, col: 9 },
};

const target: SourceTarget = {
  id: "target:button",
  class: "catalog-element",
  file: "components/card.uhura",
  span,
  label: "button",
  owner: { kind: "component", name: "card" },
};

const metadata = (text: string): SourceMetadataEntry => ({
  id: "annotation:button",
  class: "annotation",
  kind: "annotation",
  text,
  span,
  targetId: target.id,
  order: 0,
});

const render = (
  revision: number,
  freshness: PreviewFreshness,
  annotationText: string,
): EditorRender => ({
  revision,
  freshness,
  application: { name: "Lifecycle" },
  authoring: { targets: [target], entries: [metadata(annotationText)] },
  groups: [{
    id: "component/card",
    kind: "component",
    subject: "card",
    previews: ["card/default"],
  }],
  previews: [{
    id: "card/default",
    identity: { kind: "component", subject: "card", example: "default" },
    sourceFile: "components/card.uhura",
    default: true,
    pinned: false,
    derived: false,
    inFlight: 0,
    from: null,
    replaySteps: [],
    replay: [],
    note: null,
    data: [],
    interactions: [],
    documentation: { declarationDocId: null, exampleDocId: null },
    provenance: {
      occurrences: [{
        id: "occurrence:button",
        targetId: target.id,
        anchors: [anchor],
      }],
    },
    content: {
      key: "root",
      element: "text",
      props: { content: { t: "plain", v: "Stable semantic content" } },
    },
  }],
  stylesheet: ":root { --accent: blue; } body { color: black; }",
  assets: {},
  interactionGraph: { protocol: "uhura-interaction-graph/0", nodes: [], edges: [] },
});

const annotationText = (model: PreparedEditorModel): string | undefined =>
  model.authoring.entriesById.get("annotation:button")?.text;

const validatePrepared = (
  previous: PreparedEditorModel | null,
  next: PreparedEditorModel,
): void => {
  const resources = new Map(next.resourcesByPreviewId);
  if (previous) {
    for (const id of next.reusableRealizationIds) {
      const retained = previous.resourcesByPreviewId.get(id);
      if (retained) resources.set(id, retained);
    }
  }
  validateAnnotationRealizations({
    render: next.render,
    authoring: next.authoring,
    resourcesByPreviewId: resources,
  });
};

const install = (
  root: FakeElement,
  overlay: AnnotationOverlay,
  previous: PreparedEditorModel | null,
  next: PreparedEditorModel,
): void => {
  validatePrepared(previous, next);
  if (previous) reconcilePreparedEditorModel(previous, next);
  root.replaceChildren(next.board as unknown as FakeElement);
  if (previous) disposePreparedEditorModel(previous);
  overlay.install({
    render: next.render,
    authoring: next.authoring,
    resourcesByPreviewId: next.resourcesByPreviewId,
  });
};

const classElements = (root: FakeElement, className: string): FakeElement[] =>
  [root, ...root.descendants()].filter((element) => element.classList.contains(className));

const overlayAnnotationText = (root: FakeElement): readonly string[] =>
  classElements(root, "annotation-text").map((element) => element.textContent);

test("metadata, stale, cold-invalid, and recovery preserve only their owned DOM", () => {
  const document = new FakeDocument();
  const root = document.createElement("main");
  const viewport = document.createElement("div");
  const overlayRoot = document.createElement("div");
  const overlay = new AnnotationOverlay({
    viewport: asElement(viewport),
    root: asElement(overlayRoot),
    focusPreview: () => {},
  });

  const current = prepareEditorModel(asDocument(document), render(1, "current", "First"));
  install(root, overlay, null, current);
  const originalFrame = current.frameById.get("card/default");
  const originalHost = current.shadowHostById.get("card/default");
  const originalShadow = originalHost?.shadowRoot;
  const originalResources = current.resourcesByPreviewId.get("card/default");
  const originalAnchor = originalResources?.resolve(anchor);
  assert.ok(originalFrame && originalHost && originalShadow && originalResources && originalAnchor);
  assert.equal(annotationText(current), "First");
  assert.deepEqual(overlayAnnotationText(overlayRoot), ["First"]);
  assert.equal(sourceActionsEnabled(current.render), true);

  const metadataOnly = prepareEditorModel(
    asDocument(document),
    render(2, "current", "Second"),
    current,
  );
  const discardedFrame = metadataOnly.frameById.get("card/default");
  const discardedResources = metadataOnly.resourcesByPreviewId.get("card/default");
  assert.deepEqual([...metadataOnly.reusableRealizationIds], ["card/default"]);
  assert.deepEqual([...metadataOnly.reusableFrameIds], ["card/default"]);
  assert.equal(annotationText(metadataOnly), "Second", "detached authoring is already refreshed");
  install(root, overlay, current, metadataOnly);
  assert.equal(metadataOnly.frameById.get("card/default"), originalFrame);
  assert.equal(metadataOnly.shadowHostById.get("card/default"), originalHost);
  assert.equal(originalHost.shadowRoot, originalShadow);
  assert.equal(metadataOnly.resourcesByPreviewId.get("card/default"), originalResources);
  assert.equal(originalResources.resolve(anchor), originalAnchor);
  assert.equal(discardedResources?.disposed, true);
  assert.equal((discardedFrame as unknown as FakeElement).parentNode, null);
  assert.equal(annotationText(metadataOnly), "Second");
  assert.deepEqual(
    overlayAnnotationText(overlayRoot),
    ["Second"],
    "overlay presentation refreshes while its semantic frame stays exact",
  );

  const stale = prepareEditorModel(
    asDocument(document),
    render(2, "stale", "Second"),
    metadataOnly,
  );
  install(root, overlay, metadataOnly, stale);
  assert.equal(stale.frameById.get("card/default"), originalFrame);
  assert.equal(stale.shadowHostById.get("card/default")?.shadowRoot, originalShadow);
  assert.equal(stale.resourcesByPreviewId.get("card/default"), originalResources);
  assert.equal(annotationText(stale), "Second", "stale DOM keeps stale-render metadata");
  assert.equal(sourceActionsEnabled(stale.render), false);
  assert.equal(overlayRoot.classList.contains("is-stale"), true);
  assert.ok(classElements(overlayRoot, "source-location").every((button) => button.disabled));

  const coldInvalid = prepareEditorModel(asDocument(document), null, stale);
  install(root, overlay, stale, coldInvalid);
  assert.equal(coldInvalid.render, null);
  assert.equal(coldInvalid.resourcesByPreviewId.size, 0);
  assert.equal(coldInvalid.authoring.entriesById.size, 0);
  assert.equal(originalResources.disposed, true);
  assert.notEqual(root.firstChild, originalFrame as unknown as FakeNode);
  assert.deepEqual(overlayAnnotationText(overlayRoot), []);

  const recovered = prepareEditorModel(
    asDocument(document),
    render(4, "current", "Recovered"),
    coldInvalid,
  );
  install(root, overlay, coldInvalid, recovered);
  const recoveredFrame = recovered.frameById.get("card/default");
  const recoveredHost = recovered.shadowHostById.get("card/default");
  const recoveredResources = recovered.resourcesByPreviewId.get("card/default");
  assert.ok(recoveredFrame && recoveredHost && recoveredResources);
  assert.notEqual(recoveredFrame, originalFrame);
  assert.notEqual(recoveredHost.shadowRoot, originalShadow);
  assert.notEqual(recoveredResources, originalResources);
  assert.ok(recoveredResources.resolve(anchor));
  assert.equal(annotationText(recovered), "Recovered");
  assert.deepEqual(overlayAnnotationText(overlayRoot), ["Recovered"]);
  assert.equal(sourceActionsEnabled(recovered.render), true);

  overlay.dispose();
  disposePreparedEditorModel(recovered);
});

test("caption chrome can replace while its semantic ShadowRoot stays exact", () => {
  const document = new FakeDocument();
  const root = document.createElement("main");
  const overlayRoot = document.createElement("div");
  const overlay = new AnnotationOverlay({
    viewport: asElement(document.createElement("div")),
    root: asElement(overlayRoot),
    focusPreview: () => {},
  });
  const current = prepareEditorModel(asDocument(document), render(1, "current", "First"));
  install(root, overlay, null, current);
  const originalFrame = current.frameById.get("card/default");
  const originalHost = current.shadowHostById.get("card/default");
  const originalResources = current.resourcesByPreviewId.get("card/default");
  assert.ok(originalFrame && originalHost && originalResources);

  const nextRender = render(2, "current", "First");
  nextRender.previews[0]!.note = "Updated caption";
  const captionUpdate = prepareEditorModel(asDocument(document), nextRender, current);
  const preparedFrame = captionUpdate.frameById.get("card/default");
  assert.deepEqual([...captionUpdate.reusableRealizationIds], ["card/default"]);
  assert.deepEqual([...captionUpdate.reusableFrameIds], []);
  install(root, overlay, current, captionUpdate);

  assert.equal(captionUpdate.frameById.get("card/default"), preparedFrame);
  assert.notEqual(captionUpdate.frameById.get("card/default"), originalFrame);
  assert.equal(captionUpdate.shadowHostById.get("card/default"), originalHost);
  assert.equal(captionUpdate.resourcesByPreviewId.get("card/default"), originalResources);
  assert.deepEqual(
    classElements(preparedFrame as unknown as FakeElement, "caption-note")
      .map((element) => element.textContent),
    ["Updated caption"],
  );

  overlay.dispose();
  disposePreparedEditorModel(captionUpdate);
});

test("all rendered occurrences keep one badge while preview selection only decorates them", () => {
  const document = new FakeDocument();
  const root = document.createElement("main");
  const overlayRoot = document.createElement("div");
  let focusedPreviewId: string | null = null;
  const overlay = new AnnotationOverlay({
    viewport: asElement(document.createElement("div")),
    root: asElement(overlayRoot),
    focusPreview: (previewId) => { focusedPreviewId = previewId; },
  });
  const multiPreview = render(1, "current", "Selected instance");
  const first = multiPreview.previews[0]!;
  multiPreview.previews.push({
    ...first,
    id: "card/alternate",
    identity: { kind: "component", subject: "card", example: "alternate" },
    default: false,
    provenance: {
      occurrences: [
        {
          id: "occurrence:button",
          targetId: target.id,
          anchors: [anchor, anchor],
        },
        { id: "occurrence:alternate:second", targetId: target.id, anchors: [anchor] },
      ],
    },
  });
  multiPreview.groups[0]!.previews.push("card/alternate");
  const model = prepareEditorModel(asDocument(document), multiPreview);
  install(root, overlay, null, model);

  const markers = classElements(overlayRoot, "annotation-marker");
  const highlights = classElements(overlayRoot, "annotation-highlight");
  const cards = classElements(overlayRoot, "annotation-card");
  assert.equal(markers.length, 3, "each occurrence gets one badge, not each of its anchors");
  assert.equal(highlights.length, 3);
  assert.equal(cards.length, 1, "all realizations share one source-target card");
  assert.ok(markers.every((marker) => marker.hidden === false));
  assert.ok(
    highlights.every((highlight) => highlight.style.display === "none"),
    "unselected occurrences show badges without annotation outlines",
  );
  assert.deepEqual(markers.map((marker) => marker.textContent), ["1", "1", "1"]);
  assert.deepEqual(
    markers.map((marker) => marker.getAttribute("data-preview-id")),
    ["card/default", "card/alternate", "card/alternate"],
  );
  assert.deepEqual(
    markers.map((marker) => marker.getAttribute("data-occurrence-id")),
    ["occurrence:button", "occurrence:button", "occurrence:alternate:second"],
    "preview-local occurrence IDs may repeat without collapsing badges",
  );
  assert.equal(new Set(markers.map((marker) => marker.id)).size, 3);
  assert.equal(cards[0]?.hidden, true, "badges are default-on while cards start dismissed");

  overlay.activatePreviewOccurrences("card/alternate");
  assert.deepEqual(
    markers.map((marker) => marker.classList.contains("is-preview-active")),
    [false, true, true],
  );
  assert.deepEqual(
    highlights.map((highlight) => highlight.classList.contains("is-preview-active")),
    [false, true, true],
  );
  assert.deepEqual(
    highlights.map((highlight) => highlight.style.display),
    ["none", "", ""],
    "only selected-preview occurrences show annotation outlines",
  );
  assert.ok(markers.every((marker) => marker.hidden === false));
  assert.equal(overlay.selectSourceTarget(target.id), true);
  assert.equal(focusedPreviewId, "card/alternate", "Source prefers a selected-preview badge");
  assert.equal(cards[0]?.hidden, false);
  assert.equal(markers[1]?.classList.contains("is-active"), true);

  overlay.activatePreviewOccurrences("card/missing");
  assert.ok(markers.every((marker) => !marker.classList.contains("is-preview-active")));
  assert.ok(markers.every((marker) => marker.hidden === false));
  overlay.activatePreviewOccurrences(null);
  assert.ok(markers.every((marker) => !marker.classList.contains("is-preview-active")));
  assert.ok(highlights.every((highlight) => !highlight.classList.contains("is-preview-active")));
  assert.ok(highlights.every((highlight) => highlight.style.display === "none"));

  overlay.dispose();
  disposePreparedEditorModel(model);
});

test("Source targets and canvas markers expose one direct annotation selection path", () => {
  const document = new FakeDocument();
  const root = document.createElement("main");
  const overlayRoot = document.createElement("div");
  let focusedPreviewId: string | null = null;
  let focusedAnchors: readonly HTMLElement[] | undefined;
  const focusedSourceIds: string[] = [];
  const overlay = new AnnotationOverlay({
    viewport: asElement(document.createElement("div")),
    root: asElement(overlayRoot),
    focusPreview: (previewId, anchors) => {
      focusedPreviewId = previewId;
      focusedAnchors = anchors;
    },
    focusSourceTarget: (targetId) => { focusedSourceIds.push(targetId); },
  });
  const model = prepareEditorModel(asDocument(document), render(1, "current", "Navigate me"));
  install(root, overlay, null, model);

  const marker = classElements(overlayRoot, "annotation-marker")[0];
  const card = classElements(overlayRoot, "annotation-card")[0];
  assert.ok(marker && card);
  assert.equal(classElements(overlayRoot, "annotation-toolbar").length, 0);
  assert.equal(classElements(overlayRoot, "annotation-visibility").length, 0);
  assert.equal(classElements(overlayRoot, "annotation-kind-filter").length, 0);
  assert.equal(marker.hidden, false);
  assert.equal(card.hidden, true, "persistent badges do not open their cards by default");

  overlay.dismissCards();
  assert.equal(card.hidden, true);
  assert.equal(marker.hidden, false, "dismissing a card retains its marker");
  marker.focus();
  assert.equal(card.hidden, false, "marker focus reveals its card again");
  assert.equal(marker.classList.contains("is-active"), true);
  assert.equal(card.classList.contains("is-revealed"), true);
  assert.deepEqual(focusedSourceIds, [target.id]);

  overlay.dismissCards();
  assert.equal(marker.hidden, false);
  assert.equal(marker.classList.contains("is-active"), false);
  assert.equal(card.classList.contains("is-revealed"), false);
  assert.equal(overlay.selectSourceTarget(target.id), true);
  assert.equal(focusedPreviewId, "card/default");
  assert.deepEqual(
    focusedAnchors,
    [model.resourcesByPreviewId.get("card/default")?.resolve(anchor)],
    "Source navigation exposes direct semantic anchors for camera centering",
  );
  assert.equal(card.hidden, false);
  assert.equal(overlay.selectSourceTarget("missing"), false);

  overlay.toggleCanvasVisibility();
  assert.equal(marker.hidden, true);
  assert.equal(card.hidden, true);
  overlay.toggleCanvasVisibility();

  const sourcePanel = document.createElement("div");
  const selectedTargets: string[] = [];
  renderSourcePanel(
    asElement(sourcePanel),
    model.authoring,
    false,
    (targetId) => { selectedTargets.push(targetId); },
  );
  const sourceEntry = classElements(sourcePanel, "source-entry")[0];
  const show = classElements(sourcePanel, "source-target-select")[0];
  const copy = classElements(sourcePanel, "source-location")[0];
  assert.ok(sourceEntry && show && copy);
  assert.equal(sourceEntry.getAttribute("data-source-target-id"), target.id);
  assert.equal(show.getAttribute("data-source-target-id"), target.id);
  assert.equal(show.textContent, "Show");
  assert.equal(show.disabled, false);
  assert.notEqual(show, copy, "canvas selection and source copying remain separate actions");
  show.click();
  assert.deepEqual(selectedTargets, [target.id]);

  renderSourcePanel(asElement(sourcePanel), model.authoring, false);
  assert.equal(
    classElements(sourcePanel, "source-target-select").length,
    0,
    "Source omits canvas annotation actions when the Editor does not mount that feature",
  );

  overlay.dispose();
  disposePreparedEditorModel(model);
});

test("hiding annotations flags the connector layer without touching selection classes", () => {
  const document = new FakeDocument();
  const model = prepareEditorModel(asDocument(document), render(1, "current", "First"));
  const layer = model.connectorLayer;
  assert.equal(layer.getAttribute("class"), "workflow-connectors");
  layer.classList.add("has-selection", "has-structure");

  setAnnotationConnectorsHidden(layer, true);
  assert.equal(layer.classList.contains("annotations-hidden"), true);
  assert.equal(layer.classList.contains("has-selection"), true);
  assert.equal(layer.classList.contains("has-structure"), true);
  assert.equal(
    (layer as unknown as { style: { display: string } }).style.display,
    "",
    "the layer itself stays rendered so structural arrows can draw and measure",
  );

  setAnnotationConnectorsHidden(layer, false);
  assert.equal(layer.classList.contains("annotations-hidden"), false);
  assert.equal(layer.classList.contains("has-structure"), true);

  disposePreparedEditorModel(model);
});
