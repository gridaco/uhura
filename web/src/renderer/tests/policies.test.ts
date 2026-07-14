import assert from "node:assert/strict";
import { test } from "vitest";

import type {
  EditorAsset as ContractEditorAsset,
} from "../../editor/editor-state.js";
import type { Descriptor, VNode } from "../../protocol/types.js";
import { createEditorRenderer } from "../editor.js";
import type { IconDefinition } from "../icons.js";
import { createPlayAssets, createPlayRenderer, findScope } from "../play.js";

type Listener = (event: FakeEvent) => void;

class FakeEvent {
  readonly key: string;
  defaultPrevented = false;

  constructor(key = "") {
    this.key = key;
  }

  preventDefault(): void {
    this.defaultPrevented = true;
  }
}

class FakeClassList {
  private readonly owner: FakeElement;

  constructor(owner: FakeElement) {
    this.owner = owner;
  }

  toggle(name: string, force: boolean): void {
    const names = new Set(this.owner.className.split(/\s+/).filter(Boolean));
    if (force) names.add(name);
    else names.delete(name);
    this.owner.className = [...names].join(" ");
  }
}

class FakeElement {
  readonly attributes = new Map<string, string>();
  readonly style = { backgroundImage: "", cssText: "" };
  readonly listeners = new Map<string, Listener[]>();
  readonly classList = new FakeClassList(this);
  readonly nodeType = 1;
  readonly ownerDocument: FakeDocument;
  readonly tagName: string;
  parentElement: FakeElement | null = null;
  children: FakeElement[] = [];
  className = "";
  textContent = "";
  innerHTML = "";
  inert = false;
  disabled = false;
  readOnly = false;
  value = "";
  type = "";
  autoplay = false;
  muted = false;
  loop = false;
  controls = false;
  playsInline = false;
  scrollLeft = 0;
  scrollTop = 0;
  scrollHeight = 0;
  clientWidth = 0;
  loadCalls = 0;

  constructor(ownerDocument: FakeDocument, tagName: string) {
    this.ownerDocument = ownerDocument;
    this.tagName = tagName.toUpperCase();
  }

  get firstChild(): FakeElement | null {
    return this.children[0] ?? null;
  }

  get lastElementChild(): FakeElement | null {
    return this.children.at(-1) ?? null;
  }

  get nextSibling(): FakeElement | null {
    if (!this.parentElement) return null;
    const index = this.parentElement.children.indexOf(this);
    return this.parentElement.children[index + 1] ?? null;
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

  append(...nodes: FakeElement[]): void {
    for (const node of nodes) {
      node.remove();
      node.parentElement = this;
      this.children.push(node);
    }
  }

  replaceChildren(...nodes: FakeElement[]): void {
    for (const child of this.children) child.parentElement = null;
    this.children = [];
    this.append(...nodes);
  }

  insertBefore(node: FakeElement, reference: FakeElement | null): void {
    node.remove();
    node.parentElement = this;
    const index = reference === null ? -1 : this.children.indexOf(reference);
    if (index < 0) this.children.push(node);
    else this.children.splice(index, 0, node);
  }

  remove(): void {
    if (!this.parentElement) return;
    const index = this.parentElement.children.indexOf(this);
    if (index >= 0) this.parentElement.children.splice(index, 1);
    this.parentElement = null;
  }

  contains(candidate: FakeElement): boolean {
    return this === candidate || this.children.some((child) => child.contains(candidate));
  }

  querySelector(selector: string): FakeElement | null {
    const direct = selector.startsWith(":scope > ");
    const needle = direct ? selector.slice(9) : selector;
    const candidates = direct ? this.children : this.descendants();
    if (needle.startsWith(".")) {
      const className = needle.slice(1);
      return (
        candidates.find((item) => item.className.split(/\s+/).includes(className)) ?? null
      );
    }
    return candidates.find((item) => item.tagName.toLowerCase() === needle) ?? null;
  }

  addEventListener(type: string, listener: Listener): void {
    const listeners = this.listeners.get(type) ?? [];
    listeners.push(listener);
    this.listeners.set(type, listeners);
  }

  fire(type: string, event = new FakeEvent()): FakeEvent {
    for (const listener of this.listeners.get(type) ?? []) listener(event);
    return event;
  }

  focus(): void {
    this.ownerDocument.activeElement = this;
  }

  load(): void {
    this.loadCalls += 1;
  }

  descendants(): FakeElement[] {
    return this.children.flatMap((child) => [child, ...child.descendants()]);
  }
}

class FakeDocument {
  activeElement: FakeElement | null = null;

  createElement(tagName: string): FakeElement {
    return new FakeElement(this, tagName);
  }

  createElementNS(_namespace: string, tagName: string): FakeElement {
    return this.createElement(tagName);
  }
}

const asDocument = (value: FakeDocument): Document => value as unknown as Document;
const asElement = (value: FakeElement): HTMLElement => value as unknown as HTMLElement;

function keyed(root: FakeElement, key: string): FakeElement {
  const found = [root, ...root.descendants()].find(
    (candidate) => candidate.getAttribute("data-key") === key,
  );
  if (!found) throw new Error(`missing rendered key ${key}`);
  return found;
}

const press: Descriptor = {
  kind: "input",
  event: "press",
  emit: "liked",
  scope: "page:1",
  payload: {},
};

const fixture: VNode[] = [
  {
    key: "root",
    element: "view",
    props: { role: "list" },
    children: [
      { key: "title", element: "text", props: { content: { t: "plain", v: "Demo" } } },
      {
        key: "button",
        element: "button",
        props: { label: "Like" },
        on: [press],
        children: [{ key: "button-label", element: "text", props: { content: "Like" } }],
      },
      {
        key: "image",
        element: "image",
        props: { src: { t: "image", asset: "photo" }, alt: "A photograph" },
      },
      {
        key: "video",
        element: "video",
        props: {
          src: { t: "image", asset: "clip" },
          poster: { t: "image", asset: "poster" },
          label: "A clip",
          autoplay: true,
          muted: true,
          controls: true,
        },
      },
      { key: "icon", element: "icon", props: { name: "heart" } },
      {
        key: "pager",
        element: "pager",
        props: { label: "Gallery", indicator: "dots" },
        children: [
          { key: "slide-1", element: "text", props: { content: "One" } },
          { key: "slide-2", element: "text", props: { content: "Two" } },
        ],
      },
      {
        key: "field",
        element: "text-field",
        props: { value: "draft", label: "Caption" },
        on: [{ ...press, event: "change" }],
      },
      {
        key: "feed",
        element: "scroll",
        props: {},
        on: [{ ...press, kind: "observe", event: "near-end" }],
      },
    ],
  },
];

test("Editor and Play share semantic structure while Editor stays inert", () => {
  const editorDocument = new FakeDocument();
  const playDocument = new FakeDocument();
  const editorHost = editorDocument.createElement("main");
  const playHost = playDocument.createElement("main");

  const icons: Record<string, IconDefinition> = {
    heart: {
      viewBox: [0, 0, 24, 24],
      commands: [
        {
          kind: "path",
          d: "M12 20 4 12",
          fill: "none",
          stroke: "currentColor",
          strokeWidth: "1.8",
        },
        {
          kind: "circle",
          cx: "12",
          cy: "12",
          r: "3.5",
          opacity: "0.4",
        },
      ],
    },
  };
  const assets: Record<string, ContractEditorAsset> = {
    photo: { dataUri: "data:image/jpeg;base64,photo", alt: "A photograph" },
    poster: { dataUri: "data:image/jpeg;base64,poster", alt: "A clip" },
  };
  const editor = createEditorRenderer({
    document: asDocument(editorDocument),
    icons,
    assets,
  });

  const emitted: Descriptor[] = [];
  let textFieldWires = 0;
  let textFieldApplies = 0;
  let scrollSyncs = 0;
  const play = createPlayRenderer({
    document: asDocument(playDocument),
    icons: {
      heart: {
        viewBox: [0, 0, 24, 24],
        commands: [{ kind: "path", d: "M12 20 4 12" }],
      },
    },
    assets: createPlayAssets(),
    emit: (descriptor) => emitted.push(descriptor),
    textFields: {
      wire: () => {
        textFieldWires += 1;
      },
      applyValue: (input, value) => {
        textFieldApplies += 1;
        input.value = value;
      },
    },
    scrolls: {
      sync: () => {
        scrollSyncs += 1;
      },
      disposeSubtree: () => {},
      savePositions: () => {},
      restorePositions: () => {},
    },
  });

  editor.realize(asElement(editorHost), fixture, { scope: "preview:demo" });
  play.reconcileChildren(asElement(playHost), fixture, "preview:demo", false);

  assert.equal(editorHost.inert, true);
  for (const key of [
    "root",
    "title",
    "button",
    "image",
    "video",
    "icon",
    "pager",
    "field",
    "feed",
  ]) {
    const editorElement = keyed(editorHost, key);
    const playElement = keyed(playHost, key);
    assert.equal(editorElement.tagName, playElement.tagName, `${key} tag`);
    assert.equal(editorElement.className, playElement.className, `${key} class`);
    assert.equal(editorElement.getAttribute("data-path"), playElement.getAttribute("data-path"));
    assert.equal(editorElement.getAttribute("role"), playElement.getAttribute("role"));
  }

  assert.equal(keyed(editorHost, "root").getAttribute("role"), "list");
  assert.equal(keyed(editorHost, "button").getAttribute("role"), "listitem");
  assert.equal(keyed(editorHost, "image").getAttribute("aria-label"), "A photograph");
  assert.equal(keyed(editorHost, "button").getAttribute("aria-label"), "Like");
  assert.equal(findScope(fixture[0] as VNode), "page:1");

  const editorPager = keyed(editorHost, "pager");
  const editorTrack = editorPager.querySelector(":scope > .uh-track");
  const editorDots = editorPager.querySelector(":scope > .uh-dots");
  assert.equal(editorPager.getAttribute("role"), "listitem");
  assert.equal(editorPager.getAttribute("aria-label"), "Gallery");
  assert.equal(editorTrack?.children.length, 2);
  assert.equal(editorDots?.children.length, 2);

  const editorSvg = keyed(editorHost, "icon").querySelector("svg");
  const editorPath = editorSvg?.querySelector("path");
  const editorCircle = editorSvg?.querySelector("circle");
  assert.equal(editorSvg?.getAttribute("viewBox"), "0 0 24 24");
  assert.equal(editorPath?.getAttribute("stroke-width"), "1.8");
  assert.equal(editorCircle?.getAttribute("r"), "3.5");
  assert.equal(editorCircle?.getAttribute("opacity"), "0.4");

  keyed(editorHost, "button").fire("click");
  assert.deepEqual(emitted, []);
  keyed(playHost, "button").fire("click");
  assert.deepEqual(emitted, [press]);

  assert.equal(textFieldWires, 1);
  assert.equal(textFieldApplies, 1);
  assert.equal(scrollSyncs, 1);
  const editorInput = keyed(editorHost, "field").querySelector(":scope > input");
  assert.equal(editorInput?.readOnly, true);
  assert.equal(editorInput?.value, "draft");
  assert.equal(editorInput?.listeners.size, 0);
});

test("Editor realization is fresh, local-only, and video is poster-only", () => {
  const document = new FakeDocument();
  const host = document.createElement("main");
  const renderer = createEditorRenderer({
    document: asDocument(document),
    icons: {},
    assets: {},
  });

  renderer.realize(asElement(host), fixture);
  const firstRoot = host.firstChild;
  const image = keyed(host, "image");
  const video = keyed(host, "video");
  assert.match(
    image.style.backgroundImage,
    /^url\("data:image\/svg\+xml;utf8,<svg /,
  );
  assert.match(
    video.getAttribute("poster") ?? "",
    /^data:image\/svg\+xml;utf8,<svg /,
  );
  assert.notEqual(
    image.style.backgroundImage,
    `url(${JSON.stringify(video.getAttribute("poster"))})`,
    "each missing asset id keeps its own deterministic hue",
  );
  assert.equal(video.hasAttribute("src"), false);
  assert.equal(video.autoplay, false);
  assert.equal(video.controls, false);
  assert.equal(video.getAttribute("data-video-preview"), "poster");

  renderer.realize(asElement(host), fixture);
  assert.notEqual(host.firstChild, firstRoot, "one-shot realization remounts semantic DOM");
});

test("Play disposes replaced and removed subtrees before detaching them", () => {
  const document = new FakeDocument();
  const host = document.createElement("main");
  const disposed: { root: FakeElement; attached: boolean }[] = [];
  const renderer = createPlayRenderer({
    document: asDocument(document),
    icons: {},
    assets: createPlayAssets(),
    emit: () => {},
    textFields: {
      wire: () => {},
      applyValue: () => {},
    },
    scrolls: {
      sync: () => {},
      disposeSubtree: (root) => {
        const fake = root as unknown as FakeElement;
        disposed.push({ root: fake, attached: fake.parentElement !== null });
      },
      savePositions: () => {},
      restorePositions: () => {},
    },
  });

  const initial: VNode[] = [
    {
      key: "root",
      element: "view",
      props: {},
      children: [
        {
          key: "gone",
          element: "view",
          props: {},
          children: [{ key: "nested-scroll", element: "scroll", props: {} }],
        },
        { key: "changed", element: "scroll", props: {} },
      ],
    },
  ];
  renderer.reconcileChildren(asElement(host), initial, "page:1", false);
  const gone = keyed(host, "gone");
  const changed = keyed(host, "changed");

  renderer.reconcileChildren(
    asElement(host),
    [
      {
        key: "root",
        element: "view",
        props: {},
        children: [{ key: "changed", element: "view", props: {} }],
      },
    ],
    "page:1",
    false,
  );

  assert.deepEqual(
    disposed.map(({ root, attached }) => [root, attached]),
    [
      [changed, true],
      [gone, true],
    ],
  );
  assert.equal(changed.parentElement, null);
  assert.equal(gone.parentElement, null);
});
