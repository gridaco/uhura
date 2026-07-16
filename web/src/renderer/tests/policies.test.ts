import assert from "node:assert/strict";
import { test } from "vitest";

import type {
  EditorAsset as ContractEditorAsset,
} from "../../editor/editor-state.js";
import type { Descriptor, VNode } from "../../protocol/types.js";
import { createEditorRenderer } from "../editor.js";
import type {
  EditorNodeRealization,
  EditorRenderRoot,
} from "../editor.js";
import { PROVISIONAL_BROWSER_ICON_TABLE } from "../icons.js";
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
        key: "img",
        element: "img",
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
        element: "textfield",
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

  const assets: Record<string, ContractEditorAsset> = {
    photo: { dataUri: "data:image/jpeg;base64,photo", alt: "A photograph" },
    poster: { dataUri: "data:image/jpeg;base64,poster", alt: "A clip" },
  };
  const editor = createEditorRenderer({
    document: asDocument(editorDocument),
    assets,
  });

  const emitted: Descriptor[] = [];
  let textFieldWires = 0;
  let textFieldApplies = 0;
  let scrollSyncs = 0;
  const play = createPlayRenderer({
    document: asDocument(playDocument),
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
    "img",
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
  const editorImg = keyed(editorHost, "img");
  assert.equal(editorImg.tagName, "IMG");
  assert.equal(editorImg.getAttribute("alt"), "A photograph");
  assert.equal(editorImg.getAttribute("aria-label"), null);
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
  const playPath = keyed(playHost, "icon").querySelector("svg")?.querySelector("path");
  assert.equal(editorSvg?.getAttribute("viewBox"), "0 0 24 24");
  assert.equal(editorPath?.getAttribute("stroke-width"), "1.8");
  assert.equal(editorPath?.getAttribute("d"), playPath?.getAttribute("d"));
  assert.equal(keyed(editorHost, "icon").getAttribute("aria-hidden"), "true");

  keyed(editorHost, "button").fire("click");
  assert.deepEqual(emitted, []);
  keyed(playHost, "button").fire("click");
  assert.deepEqual(emitted, [press]);

  assert.equal(textFieldWires, 1);
  assert.equal(textFieldApplies, 1);
  assert.equal(scrollSyncs, 1);
  assert.equal(keyed(editorHost, "field").className, "uh-textfield");
  assert.equal(keyed(playHost, "field").className, "uh-textfield");
  const editorInput = keyed(editorHost, "field").querySelector(":scope > input");
  assert.equal(editorInput?.readOnly, true);
  assert.equal(editorInput?.value, "draft");
  assert.equal(editorInput?.listeners.size, 0);
});

test("img uses native alternative-text and decorative semantics", () => {
  const document = new FakeDocument();
  const host = document.createElement("main");
  const renderer = createEditorRenderer({
    document: asDocument(document),
    assets: {
      photo: { dataUri: "data:image/jpeg;base64,photo", alt: "Manifest fallback" },
      texture: { dataUri: "data:image/jpeg;base64,texture", alt: "Manifest fallback" },
    },
  });

  renderer.realize(asElement(host), [
    {
      key: "informative",
      element: "img",
      props: { src: { t: "image", asset: "photo" }, alt: "A photograph" },
    },
    {
      key: "decorative",
      element: "img",
      props: { src: { t: "image", asset: "texture" }, decorative: true },
    },
  ]);

  const informative = keyed(host, "informative");
  assert.equal(informative.tagName, "IMG");
  assert.equal(informative.className, "uh-img");
  assert.equal(informative.getAttribute("alt"), "A photograph");
  assert.equal(informative.getAttribute("role"), null);
  assert.equal(informative.getAttribute("aria-label"), null);

  const decorative = keyed(host, "decorative");
  assert.equal(decorative.tagName, "IMG");
  assert.equal(decorative.getAttribute("alt"), "");
  assert.equal(decorative.getAttribute("role"), null);
  assert.equal(decorative.getAttribute("aria-hidden"), null);
});

test("provisional browser glyphs cover the current base-catalog vocabulary", () => {
  assert.deepEqual(Object.keys(PROVISIONAL_BROWSER_ICON_TABLE).sort(), [
    "back",
    "bookmark",
    "bookmark-filled",
    "chevron-left",
    "chevron-right",
    "close",
    "comment",
    "grid",
    "heart",
    "heart-filled",
    "home",
    "layers",
    "plus",
    "profile",
    "progress",
    "reels",
    "search",
    "video-off",
  ]);
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
  const img = keyed(host, "img");
  const video = keyed(host, "video");
  const missingGlyph = keyed(host, "icon").querySelector("svg")?.querySelector("circle");
  assert.match(
    img.getAttribute("src") ?? "",
    /^data:image\/svg\+xml;utf8,<svg /,
  );
  assert.match(
    video.getAttribute("poster") ?? "",
    /^data:image\/svg\+xml;utf8,<svg /,
  );
  assert.notEqual(
    img.getAttribute("src"),
    video.getAttribute("poster"),
    "each missing asset id keeps its own deterministic hue",
  );
  assert.equal(video.hasAttribute("src"), false);
  assert.equal(video.autoplay, false);
  assert.equal(video.controls, false);
  assert.equal(video.getAttribute("data-video-preview"), "poster");
  assert.equal(missingGlyph?.getAttribute("r"), "8");

  renderer.realize(asElement(host), fixture);
  assert.notEqual(host.firstChild, firstRoot, "one-shot realization remounts semantic DOM");
});

test("Editor root realization reports semantic paths and direct element handles", () => {
  const document = new FakeDocument();
  const host = document.createElement("main");
  const renderer = createEditorRenderer({
    document: asDocument(document),
    icons: {},
    assets: {},
  });
  const observed: EditorNodeRealization[] = [];

  const realized = renderer.realizeRoot(asElement(host), fixture[0]!, {
    root: { kind: "page" },
    scope: "preview:page",
    observe(realization) {
      assert.equal(
        host.contains(realization.element as unknown as FakeElement),
        true,
        "observers run only after the complete root is mounted",
      );
      observed.push(realization);
    },
  });

  const expected = [
    ["root", []],
    ["title", [0]],
    ["button", [1]],
    ["button-label", [1, 0]],
    ["img", [2]],
    ["video", [3]],
    ["icon", [4]],
    ["pager", [5]],
    ["slide-1", [5, 0]],
    ["slide-2", [5, 1]],
    ["field", [6]],
    ["feed", [7]],
  ] as const;

  assert.equal(host.inert, true);
  assert.deepEqual(observed, realized);
  assert.deepEqual(
    realized.map(({ path, element }) => [element.getAttribute("data-key"), path]),
    expected,
  );
  for (const [key] of expected) {
    assert.equal(
      realized.find(({ element }) => element.getAttribute("data-key") === key)?.element,
      asElement(keyed(host, key)),
      `${key} reports the renderer-created element itself`,
    );
  }

  const pager = keyed(host, "pager");
  const track = pager.querySelector(":scope > .uh-track");
  const input = keyed(host, "field").querySelector(":scope > input");
  assert.equal(track?.contains(keyed(host, "slide-1")), true);
  assert.equal(
    realized.some(({ element }) => element === asElement(track as FakeElement)),
    false,
    "pager track is mechanic DOM, not a semantic path segment",
  );
  assert.equal(
    realized.some(({ element }) => element === asElement(input as FakeElement)),
    false,
    "textfield input is mechanic DOM, not a semantic realization",
  );
  assert.equal(keyed(host, "button").listeners.size, 0);
});

test("Editor root identities are explicit and one-shot realizations retain nothing", () => {
  const document = new FakeDocument();
  const host = document.createElement("main");
  const renderer = createEditorRenderer({
    document: asDocument(document),
    icons: {},
    assets: {},
  });
  const roots: EditorRenderRoot[] = [
    { kind: "page" },
    { kind: "fragment" },
    { kind: "surface", key: "sheet" },
  ];
  let prior: readonly EditorNodeRealization[] | undefined;
  let firstObserverCalls = 0;

  for (const [index, root] of roots.entries()) {
    const current = renderer.realizeRoot(asElement(host), fixture[0]!, {
      root,
      observe:
        index === 0
          ? () => {
              firstObserverCalls += 1;
            }
          : undefined,
    });
    assert.deepEqual(current[0]?.root, root);
    assert.deepEqual(current[0]?.path, []);
    if (prior) {
      assert.equal(
        firstObserverCalls,
        prior.length,
        "a later realization never reuses an earlier observer",
      );
      for (const { element } of prior) {
        assert.equal(
          host.contains(element as unknown as FakeElement),
          false,
          "a replacement does not retain prior realization elements",
        );
      }
    }
    prior = current;
  }
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
