import { readFileSync } from "node:fs";

import { afterEach, describe, expect, it, vi } from "vitest";

import { natural } from "../protocol/machine.js";
import {
  createProjectionRenderer,
  decodeProjectionSources,
  decodeRenderDocument,
} from "./projection.js";
import type {
  ProjectionRendererOptions,
  RenderDocument,
  RenderNode,
} from "./projection.js";
import {
  PRIMITIVE_ADAPTER_IDS,
  primitiveAdapter,
} from "./primitives/registry.js";

const LIVE_REVISION = natural("1");

afterEach(() => {
  vi.unstubAllGlobals();
});

const documentValue = (text: string): RenderDocument => ({
  protocol: "uhura-view/1",
  presentation: "example@1::Web",
  machine: "example@1::Machine",
  instance: "example/1",
  sequence: natural("0"),
  nodes: [
    {
      kind: "element",
      key: "main",
      element: "main",
      attributes: [{ name: "aria-label", value: "Example" }],
      events: [],
      surface: false,
      children: [
        {
          kind: "element",
          key: "button",
          element: "button",
          attributes: [],
          events: [{ event: "press", binding: "press-1" }],
          surface: false,
          children: [{ kind: "text", key: "label", text }],
        },
      ],
    },
  ],
});

class FakeClassList {
  readonly #element: FakeElement;

  constructor(element: FakeElement) {
    this.#element = element;
  }

  add(...tokens: string[]): void {
    const next = new Set(this.#tokens());
    for (const token of tokens) next.add(token);
    this.#write(next);
  }

  contains(token: string): boolean {
    return this.#tokens().includes(token);
  }

  toggle(token: string, force?: boolean): boolean {
    const next = new Set(this.#tokens());
    const enabled = force ?? !next.has(token);
    if (enabled) next.add(token);
    else next.delete(token);
    this.#write(next);
    return enabled;
  }

  #tokens(): string[] {
    return this.#element.className.split(/\s+/u).filter(Boolean);
  }

  #write(tokens: ReadonlySet<string>): void {
    this.#element.className = [...tokens].join(" ");
  }
}

abstract class FakeNode {
  abstract readonly nodeType: number;
  parentNode: FakeElement | null = null;
  readonly ownerDocument: FakeDocument;

  constructor(document: FakeDocument) {
    this.ownerDocument = document;
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
}

class FakeText extends FakeNode {
  readonly nodeType = 3;
  data: string;

  constructor(document: FakeDocument, data: string) {
    super(document);
    this.data = data;
  }
}

interface FakeEventInit {
  readonly isComposing?: boolean;
}

class FakeElement extends FakeNode {
  readonly nodeType = 1;
  readonly localName: string;
  readonly tagName: string;
  readonly dataset: Record<string, string> = {};
  readonly classList = new FakeClassList(this);
  readonly style = { cssText: "" };
  readonly #attributes = new Map<string, string>();
  readonly #listeners = new Map<string, Set<EventListener>>();
  childNodes: FakeNode[] = [];
  inert = false;
  value = "";
  type = "";
  disabled = false;
  readOnly = false;
  autoplay = false;
  muted = false;
  loop = false;
  controls = false;
  playsInline = false;
  scrollLeft = 0;
  scrollTop = 0;
  scrollWidth = 0;
  scrollHeight = 0;
  clientWidth = 0;
  clientHeight = 0;

  constructor(document: FakeDocument, name: string) {
    super(document);
    this.localName = name.toLowerCase();
    this.tagName = this.localName.toUpperCase();
  }

  get className(): string {
    return this.#attributes.get("class") ?? "";
  }

  set className(value: string) {
    if (value.length === 0) this.#attributes.delete("class");
    else this.#attributes.set("class", value);
  }

  get children(): FakeElement[] {
    return this.childNodes.filter(
      (node): node is FakeElement => node.nodeType === 1,
    );
  }

  get firstChild(): FakeNode | null {
    return this.childNodes[0] ?? null;
  }

  get isConnected(): boolean {
    return this.parentNode !== null;
  }

  get lastElementChild(): FakeElement | null {
    return this.children.at(-1) ?? null;
  }

  get textContent(): string {
    return this.childNodes.map((node) =>
      node instanceof FakeText
        ? node.data
        : node instanceof FakeElement
          ? node.textContent
          : ""
    ).join("");
  }

  set textContent(value: string) {
    this.replaceChildren(
      ...(value.length === 0 ? [] : [this.ownerDocument.createTextNode(value)]),
    );
  }

  setAttribute(name: string, value: string): void {
    this.#attributes.set(name, value);
  }

  getAttribute(name: string): string | null {
    return this.#attributes.get(name) ?? null;
  }

  hasAttribute(name: string): boolean {
    return this.#attributes.has(name);
  }

  removeAttribute(name: string): void {
    this.#attributes.delete(name);
  }

  toggleAttribute(name: string, force?: boolean): boolean {
    const enabled = force ?? !this.hasAttribute(name);
    if (enabled) this.setAttribute(name, "");
    else this.removeAttribute(name);
    return enabled;
  }

  append(...nodes: FakeNode[]): void {
    for (const node of nodes) this.insertBefore(node, null);
  }

  insertBefore(node: FakeNode, reference: FakeNode | null): FakeNode {
    node.remove();
    const index = reference === null
      ? this.childNodes.length
      : this.childNodes.indexOf(reference);
    this.childNodes.splice(index < 0 ? this.childNodes.length : index, 0, node);
    node.parentNode = this;
    return node;
  }

  replaceChildren(...nodes: FakeNode[]): void {
    for (const child of this.childNodes) child.parentNode = null;
    this.childNodes = [];
    this.append(...nodes);
  }

  contains(candidate: FakeElement): boolean {
    return candidate === this
      || this.children.some((child) => child.contains(candidate));
  }

  querySelectorAll<T extends Element>(_selector: string): T[] {
    const matches = (candidate: FakeElement): boolean =>
      candidate.hasAttribute("autofocus")
      || (candidate.localName === "a" && candidate.hasAttribute("href"))
      || ["button", "input", "select", "textarea"].includes(candidate.localName)
      || (
        candidate.hasAttribute("tabindex")
        && candidate.getAttribute("tabindex") !== "-1"
      );
    const output: FakeElement[] = [];
    const visit = (candidate: FakeElement): void => {
      if (matches(candidate)) output.push(candidate);
      for (const child of candidate.children) visit(child);
    };
    for (const child of this.children) visit(child);
    return output as unknown as T[];
  }

  addEventListener(type: string, listener: EventListener): void {
    let eventListeners = this.#listeners.get(type);
    if (!eventListeners) {
      eventListeners = new Set();
      this.#listeners.set(type, eventListeners);
    }
    eventListeners.add(listener);
  }

  removeEventListener(type: string, listener: EventListener): void {
    this.#listeners.get(type)?.delete(listener);
  }

  emit(type: string, init: FakeEventInit = {}): void {
    const event = {
      type,
      isComposing: init.isComposing ?? false,
      preventDefault: vi.fn<() => void>(),
    } as unknown as Event;
    this.emitEvent(type, event);
  }

  emitEvent(type: string, event: Event): void {
    for (const listener of this.#listeners.get(type) ?? []) {
      listener.call(this as unknown as EventTarget, event);
    }
  }

  click(): void {
    this.emit("click");
  }

  focus(): void {
    this.ownerDocument.activeElement = this;
    this.ownerDocument.emitEvent("focusin", {
      target: this,
    } as unknown as FocusEvent);
  }
}

class FakeIntersectionObserver {
  readonly #callback: IntersectionObserverCallback;
  target: FakeElement | undefined;
  disconnected = false;

  constructor(callback: IntersectionObserverCallback) {
    this.#callback = callback;
  }

  observe(target: Element): void {
    this.target = target as unknown as FakeElement;
  }

  disconnect(): void {
    this.disconnected = true;
  }

  emit(isIntersecting: boolean): void {
    if (this.disconnected || !this.target) return;
    this.#callback(
      [{
        isIntersecting,
        target: this.target as unknown as Element,
      } as IntersectionObserverEntry],
      this as unknown as IntersectionObserver,
    );
  }
}

class FakeDocument {
  readonly defaultView: Window | undefined;
  readonly intersections: FakeIntersectionObserver[] = [];
  activeElement: FakeElement | null = null;
  readonly #listeners = new Map<string, Set<EventListener>>();
  readonly #animationFrames = new Map<number, FrameRequestCallback>();
  #nextAnimationFrame = 1;

  constructor(options: {
    readonly animationFrames?: boolean;
    readonly intersections?: boolean;
  } = {}) {
    const { intersections } = this;
    const view: Partial<Window> & {
      IntersectionObserver?: typeof IntersectionObserver;
    } = {};
    if (options.animationFrames) {
      view.requestAnimationFrame = (callback: FrameRequestCallback): number => {
        const handle = this.#nextAnimationFrame;
        this.#nextAnimationFrame += 1;
        this.#animationFrames.set(handle, callback);
        return handle;
      };
      view.cancelAnimationFrame = (handle: number): void => {
        this.#animationFrames.delete(handle);
      };
    }
    if (options.intersections) {
      view.IntersectionObserver = class extends FakeIntersectionObserver {
        constructor(callback: IntersectionObserverCallback) {
          super(callback);
          intersections.push(this);
        }
      } as unknown as typeof IntersectionObserver;
    }
    this.defaultView =
      options.animationFrames || options.intersections
        ? view as Window
        : undefined;
  }

  createElement(name: string): FakeElement {
    return new FakeElement(this, name);
  }

  createTextNode(value: string): FakeText {
    return new FakeText(this, value);
  }

  addEventListener(type: string, listener: EventListener): void {
    let listeners = this.#listeners.get(type);
    if (!listeners) {
      listeners = new Set();
      this.#listeners.set(type, listeners);
    }
    listeners.add(listener);
  }

  removeEventListener(type: string, listener: EventListener): void {
    this.#listeners.get(type)?.delete(listener);
  }

  emitEvent(type: string, event: Event): void {
    for (const listener of this.#listeners.get(type) ?? []) {
      listener.call(this as unknown as EventTarget, event);
    }
  }

  flushAnimationFrames(): void {
    const frames = [...this.#animationFrames.values()];
    this.#animationFrames.clear();
    for (const callback of frames) callback(0);
  }
}

const fakeRoot = (options: {
  readonly animationFrames?: boolean;
  readonly intersections?: boolean;
} = {}): {
  readonly root: HTMLElement;
  readonly element: FakeElement;
  readonly document: FakeDocument;
} => {
  const document = new FakeDocument(options);
  const element = document.createElement("div");
  return {
    root: element as unknown as HTMLElement,
    element,
    document,
  };
};

type ElementRenderNode = Extract<
  RenderDocument["nodes"][number],
  { readonly kind: "element" }
>;

const elementNode = (
  key: string,
  element: string,
  attributes: readonly {
    readonly name: string;
    readonly value: boolean | string;
  }[] = [],
  children: readonly RenderDocument["nodes"][number][] = [],
  events: readonly { readonly event: string; readonly binding: string }[] = [],
): ElementRenderNode => ({
  kind: "element",
  key,
  element,
  attributes,
  events,
  children,
  surface: false,
});

const renderNodes = (
  nodes: readonly RenderDocument["nodes"][number][],
): RenderDocument => ({
  ...documentValue("fixture"),
  nodes,
});

describe("Uhura render protocol", () => {
  it("requires exact-text sequence values", () => {
    expect(decodeRenderDocument({
      ...documentValue("valid"),
      sequence: "9007199254740993",
    }).sequence).toBe("9007199254740993");
    expect(() =>
      decodeRenderDocument({
        ...documentValue("invalid"),
        sequence: 1,
      })
    ).toThrow(/sequence must be text/);
  });

  it("keeps live projection identity outside the pure view protocol", () => {
    expect(decodeRenderDocument(documentValue("static")))
      .toEqual(documentValue("static"));
    expect(() =>
      decodeRenderDocument({
        ...documentValue("live envelope field"),
        projectionRevision: "1",
      })
    ).toThrow(/Uhura render document.*wrong fields/u);
  });

  it("rejects unknown fields at every document and render-node boundary", () => {
    expect(() =>
      decodeRenderDocument({
        ...documentValue("document"),
        ambient: true,
      })
    ).toThrow(/Uhura render document.*wrong fields/u);
    expect(() =>
      decodeRenderDocument({
        ...documentValue("text"),
        nodes: [{
          kind: "text",
          key: "label",
          text: "Text",
          ambient: true,
        }],
      })
    ).toThrow(/nodes\[0\].*wrong fields/u);
    expect(() =>
      decodeRenderDocument({
        ...documentValue("element"),
        nodes: [{
          kind: "element",
          key: "main",
          element: "main",
          attributes: [],
          events: [],
          children: [],
          surface: false,
          ambient: true,
        }],
      })
    ).toThrow(/nodes\[0\].*wrong fields/u);
    expect(() =>
      decodeRenderDocument({
        ...documentValue("attribute"),
        nodes: [{
          kind: "element",
          key: "main",
          element: "main",
          attributes: [{
            name: "aria-label",
            value: "Example",
            ambient: true,
          }],
          events: [],
          children: [],
          surface: false,
        }],
      })
    ).toThrow(/attributes\[0\].*wrong fields/u);
    expect(() =>
      decodeRenderDocument({
        ...documentValue("event"),
        nodes: [{
          kind: "element",
          key: "main",
          element: "main",
          attributes: [],
          events: [{
            event: "press",
            binding: "press-1",
            ambient: true,
          }],
          children: [],
          surface: false,
        }],
      })
    ).toThrow(/events\[0\].*wrong fields/u);
  });

  it("requires projection sources to cover the exact rendered key set", () => {
    const document = documentValue("ready");
    const source = {
      id: "ui/main",
      path: "app.uhura",
      start: 0,
      end: 4,
    };
    const decoded = decodeProjectionSources({
      protocol: "uhura-projection-sources/0",
      presentation: document.presentation,
      nodes: {
        main: source,
        button: { ...source, id: "ui/button", start: 5, end: 11 },
        label: { ...source, id: "ui/label", start: 12, end: 17 },
      },
    }, document);
    expect(Object.keys(decoded.nodes)).toEqual(["main", "button", "label"]);

    expect(() => decodeProjectionSources({
      ...decoded,
      nodes: { main: source },
    }, document)).toThrow(/every rendered key exactly/u);
    expect(() => decodeProjectionSources({
      ...decoded,
      presentation: "other@1::Web",
    }, document)).toThrow(/must match its render document/u);
  });
});

describe("Uhura semantic primitive projection", () => {
  it("publishes exactly the adapters required by the checked 0.4 catalogue", () => {
    const contract = JSON.parse(readFileSync(
      new URL("../../../resources/ui-catalog/0.4.json", import.meta.url),
      "utf8",
    )) as {
      readonly protocol: string;
      readonly language: string;
      readonly primitiveAdapters: readonly string[];
    };
    expect(contract.protocol).toBe("uhura-ui-catalog/0");
    expect(contract.language).toBe("0.4");
    expect(PRIMITIVE_ADAPTER_IDS).toEqual([
      "view",
      "scroll",
      "pager",
      "text",
      "img",
      "video",
      "icon",
      "button",
      "textfield",
      "region",
    ]);
    expect([...PRIMITIVE_ADAPTER_IDS].sort())
      .toEqual(contract.primitiveAdapters);
    expect(new Set(PRIMITIVE_ADAPTER_IDS).size)
      .toBe(PRIMITIVE_ADAPTER_IDS.length);
    for (const id of PRIMITIVE_ADAPTER_IDS) {
      expect(primitiveAdapter(id)?.id).toBe(id);
    }
    expect(primitiveAdapter("main")).toBeUndefined();
  });

  it("retains listeners across binding tokens and rewires changed event shapes", () => {
    const { root, element } = fakeRoot();
    const dispatch = vi.fn<ProjectionRendererOptions["dispatch"]>();
    const renderer = createProjectionRenderer({
      root,
      dispatch,
      mode: "play",
    });
    const projection = (
      event: string,
      binding: string,
      className: string,
      label: string,
    ): RenderDocument =>
      renderNodes([
        elementNode(
          "action",
          "button",
          [{ name: "class", value: className }],
          [{ kind: "text", key: "label", text: label }],
          [{ event, binding }],
        ),
      ]);
    const bindingA = "press-1";
    const bindingB = "press-2";
    const doubleBinding = "double-1";
    const revisionA = natural("11");
    const revisionB = natural("12");
    const revisionDouble = natural("13");

    renderer.render(
      projection("press", bindingA, "primary", "First"),
      revisionA,
    );
    const button = element.children[0]!;
    const addEventListener = vi.spyOn(button, "addEventListener");
    const removeEventListener = vi.spyOn(button, "removeEventListener");
    const setAttribute = vi.spyOn(button, "setAttribute");
    const removeAttribute = vi.spyOn(button, "removeAttribute");
    const toggleAttribute = vi.spyOn(button, "toggleAttribute");

    renderer.render(
      projection("press", bindingA, "primary", "Second"),
      revisionA,
    );
    expect(button.textContent).toBe("Second");
    expect(addEventListener).not.toHaveBeenCalled();
    expect(removeEventListener).not.toHaveBeenCalled();
    expect(setAttribute).not.toHaveBeenCalled();
    expect(removeAttribute).not.toHaveBeenCalled();
    expect(toggleAttribute).not.toHaveBeenCalled();

    dispatch.mockClear();
    button.click();
    expect(dispatch).toHaveBeenCalledTimes(1);
    expect(dispatch).toHaveBeenCalledWith(bindingA, revisionA, {
      $: "record",
      fields: [],
    });

    renderer.render(
      projection("press", bindingB, "primary", "Third"),
      revisionB,
    );
    expect(removeEventListener).not.toHaveBeenCalled();
    expect(addEventListener).not.toHaveBeenCalled();
    dispatch.mockClear();
    button.click();
    expect(dispatch).toHaveBeenCalledTimes(1);
    expect(dispatch).toHaveBeenCalledWith(bindingB, revisionB, {
      $: "record",
      fields: [],
    });

    renderer.render(
      projection(
        "activate-double",
        doubleBinding,
        "primary",
        "Fourth",
      ),
      revisionDouble,
    );
    expect(removeEventListener).toHaveBeenCalledTimes(1);
    expect(addEventListener).toHaveBeenCalledTimes(1);
    dispatch.mockClear();
    button.emit("dblclick");
    expect(dispatch).toHaveBeenCalledTimes(1);
    expect(dispatch).toHaveBeenCalledWith(
      doubleBinding,
      revisionDouble,
      {
        $: "record",
        fields: [],
      },
    );

    renderer.render(
      projection(
        "activate-double",
        doubleBinding,
        "secondary",
        "Fifth",
      ),
      revisionDouble,
    );
    expect(removeEventListener).toHaveBeenCalledTimes(1);
    expect(addEventListener).toHaveBeenCalledTimes(1);
    expect(setAttribute).toHaveBeenCalledWith(
      "class",
      "uh-button secondary",
    );
  });

  it("restores a controlled input property without rewriting its attribute", () => {
    const { root, element } = fakeRoot();
    const renderer = createProjectionRenderer({
      root,
      dispatch: vi.fn<ProjectionRendererOptions["dispatch"]>(),
      mode: "play",
    });
    const projection = renderNodes([
      elementNode("query", "input", [
        { name: "type", value: "text" },
        { name: "value", value: "model" },
      ]),
    ]);

    renderer.render(projection);
    const input = element.children[0]!;
    input.value = "browser edit";
    const setAttribute = vi.spyOn(input, "setAttribute");

    renderer.render(projection);
    expect(input.value).toBe("model");
    expect(setAttribute).not.toHaveBeenCalled();
  });

  it("maps button state and classes without changing raw HTML passthrough", () => {
    const { root, element } = fakeRoot();
    const dispatch = vi.fn<ProjectionRendererOptions["dispatch"]>();
    const renderer = createProjectionRenderer({
      root,
      dispatch,
      mode: "play",
    });
    renderer.render(renderNodes([
      elementNode("main", "main", [
        { name: "class", value: "document-main" },
        { name: "aria-label", value: "Raw main" },
        { name: "data-trace", value: "kept" },
      ], [
        elementNode("action", "button", [
          { name: "class", value: "primary" },
          { name: "label", value: "Publish" },
          { name: "busy", value: true },
          { name: "pressed", value: false },
          { name: "current", value: true },
          { name: "disabled", value: true },
        ]),
        elementNode(
          "native-input",
          "input",
          [
            { name: "class", value: "native-search" },
            { name: "type", value: "text" },
            { name: "value", value: "initial" },
            { name: "aria-label", value: "Native search" },
          ],
          [],
          [{ event: "input", binding: "native-change" }],
        ),
      ]),
    ]), LIVE_REVISION);

    const main = element.children[0]!;
    const button = main.children[0]!;
    const input = main.children[1]!;
    expect(main.className).toBe("document-main");
    expect(main.getAttribute("aria-label")).toBe("Raw main");
    expect(main.getAttribute("data-trace")).toBe("kept");
    expect(button.className).toBe("uh-button primary");
    expect(button.getAttribute("type")).toBe("button");
    expect(button.getAttribute("aria-label")).toBe("Publish");
    expect(button.getAttribute("aria-busy")).toBe("true");
    expect(button.getAttribute("aria-pressed")).toBe("false");
    expect(button.getAttribute("aria-current")).toBe("true");
    expect(button.hasAttribute("disabled")).toBe(true);
    expect(button.disabled).toBe(true);
    expect(button.hasAttribute("label")).toBe(false);
    expect(button.hasAttribute("busy")).toBe(false);
    expect(button.hasAttribute("pressed")).toBe(false);
    expect(button.hasAttribute("current")).toBe(false);
    expect(input.className).toBe("native-search");
    expect(input.getAttribute("type")).toBe("text");
    expect(input.getAttribute("aria-label")).toBe("Native search");
    expect(input.value).toBe("initial");
    input.value = "next";
    input.emit("input");
    expect(dispatch).toHaveBeenCalledWith("native-change", LIVE_REVISION, {
      $: "record",
      fields: [{ name: "value", value: { $: "Text", value: "next" } }],
    });

    renderer.render(renderNodes([
      elementNode("main", "main", [], [
        elementNode("action", "button", [
          { name: "class", value: "secondary" },
          { name: "label", value: "Publish" },
          { name: "busy", value: false },
          { name: "current", value: false },
          { name: "disabled", value: false },
        ]),
      ]),
    ]));
    const updated = element.children[0]!.children[0]!;
    expect(updated).toBe(button);
    expect(updated.className).toBe("uh-button secondary");
    expect(updated.hasAttribute("aria-busy")).toBe(false);
    expect(updated.hasAttribute("aria-pressed")).toBe(false);
    expect(updated.hasAttribute("aria-current")).toBe(false);
    expect(updated.hasAttribute("disabled")).toBe(false);
    expect(updated.disabled).toBe(false);

    renderer.render(renderNodes([{
      ...elementNode("surface", "dialog", [
        { name: "class", value: "comments-sheet" },
      ]),
      surface: true,
    }]));
    const surface = element.children[0]!;
    expect(surface.className).toBe("uhura-surface comments-sheet");
    renderer.render(renderNodes([{
      ...elementNode("surface", "dialog", [
        { name: "class", value: "policy-sheet" },
      ]),
      surface: true,
    }]));
    expect(element.children[0]).toBe(surface);
    expect(surface.className).toBe("uhura-surface policy-sheet");
  });

  it("keeps machine surfaces in a dedicated host and inerts only the page", () => {
    const { root, element, document } = fakeRoot();
    const surfaceElement = document.createElement("div");
    const host = document.createElement("div");
    const hostControl = document.createElement("button");
    host.append(element, surfaceElement, hostControl);
    const dispatch = vi.fn<ProjectionRendererOptions["dispatch"]>();
    const renderer = createProjectionRenderer({
      root,
      surfaceRoot: surfaceElement as unknown as HTMLElement,
      dispatch,
      mode: "play",
    });
    const nested = {
      ...elementNode("nested-surface", "dialog", [], [
        elementNode(
          "nested-action",
          "button",
          [],
          [{ kind: "text", key: "nested-label", text: "Nested" }],
          [{ event: "press", binding: "nested-press" }],
        ),
        elementNode(
          "nested-secondary",
          "button",
          [],
          [{ kind: "text", key: "secondary-label", text: "Secondary" }],
        ),
      ]),
      surface: true,
    };
    const surface = (children: readonly RenderNode[] = []) => ({
      ...elementNode("surface", "dialog", [], [
        { kind: "text" as const, key: "surface-label", text: "Policy" },
        ...children,
      ]),
      surface: true,
    });
    const page = (children: readonly RenderNode[] = []): RenderDocument =>
      renderNodes([
        elementNode("page", "main", [], [
          elementNode(
            "opener",
            "button",
            [],
            [{ kind: "text", key: "opener-label", text: "Open" }],
          ),
          ...children,
        ]),
      ]);

    renderer.render(page());
    const opener = element.children[0]!.children[0]!;
    opener.focus();
    renderer.render(page([surface()]));
    expect(document.activeElement).toBe(surfaceElement.children[0]);

    renderer.render(page([surface([nested])]));

    expect(element.children.map((child) => child.localName)).toEqual(["main"]);
    expect(surfaceElement.children.map((child) => child.localName))
      .toEqual(["dialog", "dialog"]);
    expect(surfaceElement.children[0]?.textContent).toBe("Policy");
    expect(surfaceElement.children[0]?.hasAttribute("open")).toBe(true);
    expect(surfaceElement.children[0]?.inert).toBe(true);
    expect(surfaceElement.children[0]?.getAttribute("aria-hidden")).toBe("true");
    expect(surfaceElement.children[1]?.inert).toBe(false);
    expect(surfaceElement.children[1]?.getAttribute("aria-hidden")).toBeNull();
    expect(surfaceElement.children[1]?.getAttribute("aria-modal")).toBeNull();
    expect(surfaceElement.children[1]?.getAttribute("tabindex")).toBe("-1");
    expect(element.inert).toBe(true);
    const nestedDialog = surfaceElement.children[1]!;
    const nestedAction = nestedDialog.children[0]!;
    const nestedSecondary = nestedDialog.children[1]!;
    expect(document.activeElement).toBe(nestedAction);
    hostControl.focus();
    expect(document.activeElement).toBe(hostControl);
    renderer.render(page([surface([nested])]));
    expect(document.activeElement).toBe(hostControl);
    opener.focus();
    expect(document.activeElement).toBe(nestedAction);
    nestedSecondary.focus();
    const tab = {
      key: "Tab",
      shiftKey: false,
      preventDefault: vi.fn<() => void>(),
      stopPropagation: vi.fn<() => void>(),
    } as unknown as KeyboardEvent;
    nestedDialog.emitEvent("keydown", tab);
    expect(tab.preventDefault).toHaveBeenCalledOnce();
    expect(document.activeElement).toBe(nestedAction);
    const reverseTab = {
      key: "Tab",
      shiftKey: true,
      preventDefault: vi.fn<() => void>(),
      stopPropagation: vi.fn<() => void>(),
    } as unknown as KeyboardEvent;
    nestedDialog.emitEvent("keydown", reverseTab);
    expect(reverseTab.preventDefault).toHaveBeenCalledOnce();
    expect(document.activeElement).toBe(nestedSecondary);
    renderer.render(page([surface([{
      ...nested,
      children: nested.children.slice(0, 1),
    }])]));
    expect(surfaceElement.children[1]).toBe(nestedDialog);
    expect(document.activeElement).toBe(nestedAction);
    const escape = {
      key: "Escape",
      preventDefault: vi.fn<() => void>(),
      stopPropagation: vi.fn<() => void>(),
    } as unknown as KeyboardEvent;
    nestedDialog.emitEvent("keydown", escape);
    expect(escape.preventDefault).toHaveBeenCalledOnce();
    expect(escape.stopPropagation).toHaveBeenCalledOnce();
    expect(dispatch).not.toHaveBeenCalled();
    surfaceElement.click();
    expect(dispatch).not.toHaveBeenCalled();
    nestedAction.click();
    expect(dispatch).toHaveBeenCalledWith(
      "nested-press",
      undefined,
      { $: "record", fields: [] },
    );

    renderer.render(page([surface()]));
    expect(document.activeElement).toBe(surfaceElement.children[0]);
    renderer.render(page());
    expect(surfaceElement.children).toHaveLength(0);
    expect(element.inert).toBe(false);
    expect(document.activeElement).toBe(opener);

    renderer.render(page([surface()]));
    const firstLifetime = surfaceElement.children[0]!;
    hostControl.focus();
    renderer.render(page([{
      ...surface(),
      key: "replacement-surface",
    }]));
    expect(surfaceElement.children[0]).not.toBe(firstLifetime);
    expect(document.activeElement).toBe(hostControl);
    renderer.render(page());
    expect(surfaceElement.children).toHaveLength(0);
    expect(element.inert).toBe(false);
    expect(document.activeElement).toBe(hostControl);
  });

  it("maps checked Link hrefs and enforces disabled without an event handler", () => {
    const { root, element } = fakeRoot();
    const renderer = createProjectionRenderer({
      root,
      dispatch: vi.fn<ProjectionRendererOptions["dispatch"]>(),
      mode: "play",
      resolveLinkHref: (href) => href === "/" ? "/play" : href,
    });
    const link = (disabled: boolean): RenderDocument => renderNodes([
      elementNode("home", "a", [
        { name: "href", value: "/" },
        { name: "disabled", value: disabled },
      ], [{ kind: "text", key: "home-label", text: "Home" }]),
    ]);

    renderer.render(link(true));
    const anchor = element.children[0]!;
    expect(anchor.getAttribute("href")).toBe("/play");
    expect(anchor.getAttribute("aria-disabled")).toBe("true");
    expect(anchor.hasAttribute("disabled")).toBe(false);
    const disabledClick = {
      preventDefault: vi.fn<() => void>(),
    } as unknown as Event;
    anchor.emitEvent("click", disabledClick);
    expect(disabledClick.preventDefault).toHaveBeenCalledOnce();

    renderer.render(link(false));
    const enabledClick = {
      preventDefault: vi.fn<() => void>(),
    } as unknown as Event;
    anchor.emitEvent("click", enabledClick);
    expect(enabledClick.preventDefault).not.toHaveBeenCalled();
  });

  it("owns list-item roles at a neutral view boundary", () => {
    const { root, element } = fakeRoot();
    const renderer = createProjectionRenderer({
      root,
      dispatch: vi.fn<ProjectionRendererOptions["dispatch"]>(),
      mode: "play",
    });
    renderer.render(renderNodes([
      elementNode("list", "view", [{ name: "role", value: "list" }], [
        elementNode("item", "view", [], [
          elementNode(
            "action",
            "region",
            [{ name: "label", value: "Open profile" }],
            [{ kind: "text", key: "label", text: "Profile" }],
            [{ event: "activate", binding: "open" }],
          ),
        ]),
      ]),
      elementNode("unsupported-role", "view", [
        { name: "role", value: "tablist" },
      ]),
    ]));

    const list = element.children[0]!;
    const item = list.children[0]!;
    const action = item.children[0]!;
    expect(list.getAttribute("role")).toBe("list");
    expect(item.getAttribute("role")).toBe("listitem");
    expect(action.getAttribute("role")).toBe("button");
    expect(element.children[1]!.getAttribute("role")).toBeNull();

    expect(() => renderer.render(renderNodes([
      elementNode("invalid-list", "view", [{ name: "role", value: "list" }], [
        elementNode(
          "invalid-item",
          "region",
          [{ name: "label", value: "Open profile" }],
          [],
          [{ event: "activate", binding: "open" }],
        ),
      ]),
    ]))).toThrow(/neutral direct child/u);
  });

  it("realizes scroll and region semantics and keeps Editor effects inert", () => {
    const dispatch = vi.fn<ProjectionRendererOptions["dispatch"]>();
    const play = fakeRoot();
    createProjectionRenderer({
      root: play.root,
      dispatch,
      mode: "play",
    }).render(renderNodes([
      elementNode("scroll", "scroll", [
        { name: "class", value: "rail" },
        { name: "direction", value: "horizontal" },
      ]),
      elementNode(
        "region",
        "region",
        [
          { name: "class", value: "card" },
          { name: "label", value: "Open profile" },
          { name: "supplementary", value: true },
        ],
        [],
        [{ event: "activate", binding: "open" }],
      ),
    ]), LIVE_REVISION);
    const scroll = play.element.children[0]!;
    const region = play.element.children[1]!;
    expect(scroll.className).toBe("uh-scroll rail");
    expect(scroll.getAttribute("data-direction")).toBe("horizontal");
    expect(scroll.hasAttribute("direction")).toBe(false);
    expect(region.className).toBe("uh-region card");
    expect(region.getAttribute("role")).toBe("button");
    expect(region.getAttribute("tabindex")).toBe("0");
    expect(region.getAttribute("aria-label")).toBe("Open profile");
    expect(region.hasAttribute("label")).toBe(false);
    expect(region.hasAttribute("supplementary")).toBe(false);
    region.click();
    expect(dispatch).toHaveBeenCalledWith("open", LIVE_REVISION, {
      $: "record",
      fields: [],
    });
    class KeyboardEventStub {
      readonly isComposing = false;
      readonly keyCode = 0;
      readonly key: string;
      readonly preventDefault = vi.fn<() => void>();

      constructor(key: string) {
        this.key = key;
      }
    }
    vi.stubGlobal("KeyboardEvent", KeyboardEventStub);
    const enter = new KeyboardEventStub("Enter");
    region.emitEvent("keydown", enter as unknown as Event);
    expect(enter.preventDefault).toHaveBeenCalledTimes(1);
    expect(dispatch).toHaveBeenLastCalledWith("open", LIVE_REVISION, {
      $: "record",
      fields: [],
    });

    const editorDispatch =
      vi.fn<ProjectionRendererOptions["dispatch"]>();
    const editor = fakeRoot();
    createProjectionRenderer({
      root: editor.root,
      dispatch: editorDispatch,
      mode: "editor",
    }).render(renderNodes([
      elementNode(
        "region",
        "region",
        [{ name: "label", value: "Open profile" }],
        [],
        [{ event: "activate", binding: "open" }],
      ),
    ]));
    const editorRegion = editor.element.children[0]!;
    expect(editor.element.inert).toBe(true);
    expect(editorRegion.getAttribute("role")).toBe("button");
    expect(editorRegion.hasAttribute("tabindex")).toBe(false);
    editorRegion.click();
    expect(editorDispatch).not.toHaveBeenCalled();
  });

  it("realizes authored static scroll positions only in Editor mode", () => {
    const editor = fakeRoot({ animationFrames: true });
    const editorRenderer = createProjectionRenderer({
      root: editor.root,
      dispatch: vi.fn<ProjectionRendererOptions["dispatch"]>(),
      mode: "editor",
    });
    const vertical = renderNodes([
      elementNode("scroll", "scroll", [
        { name: "direction", value: "vertical" },
        { name: "position", value: "0.25" },
      ]),
    ]);
    editorRenderer.render(vertical);
    const editorScroll = editor.element.children[0]!;
    editorScroll.scrollHeight = 1_000;
    editorScroll.clientHeight = 200;
    editor.document.flushAnimationFrames();
    expect(editorScroll.scrollTop).toBe(200);

    const horizontal = renderNodes([
      elementNode("scroll", "scroll", [
        { name: "direction", value: "horizontal" },
        { name: "position", value: "0.75" },
      ]),
    ]);
    editorRenderer.render(horizontal);
    editorScroll.scrollWidth = 1_000;
    editorScroll.clientWidth = 200;
    editor.document.flushAnimationFrames();
    expect(editorScroll.scrollLeft).toBe(600);
    expect(editorScroll.scrollTop).toBe(0);

    editorRenderer.render(renderNodes([
      elementNode("scroll", "scroll", [
        { name: "direction", value: "horizontal" },
      ]),
    ]));
    editor.document.flushAnimationFrames();
    expect(editorScroll.scrollLeft).toBe(0);

    const play = fakeRoot();
    const playRenderer = createProjectionRenderer({
      root: play.root,
      dispatch: vi.fn<ProjectionRendererOptions["dispatch"]>(),
      mode: "play",
    });
    playRenderer.render(vertical);
    const playScroll = play.element.children[0]!;
    playScroll.scrollHeight = 1_000;
    playScroll.clientHeight = 200;
    playRenderer.render(vertical);
    expect(playScroll.scrollTop).toBe(0);
  });

  it("arms, updates, and disposes the scroll near-end capability", () => {
    const environment = fakeRoot({ intersections: true });
    const dispatch = vi.fn<ProjectionRendererOptions["dispatch"]>();
    const renderer = createProjectionRenderer({
      root: environment.root,
      dispatch,
      mode: "play",
    });
    const projection = (
      binding: string,
      child = "First",
    ): RenderDocument => renderNodes([
      elementNode(
        "scroll",
        "scroll",
        [],
        [{ kind: "text", key: "content", text: child }],
        [{ event: "near-end", binding }],
      ),
    ]);
    renderer.render(projection("load-1"), natural("3"));
    const firstObserver = environment.document.intersections[0]!;
    expect(firstObserver.target?.dataset["uhMechanic"]).toBe("near-end");

    firstObserver.emit(true);
    firstObserver.emit(true);
    expect(dispatch).toHaveBeenCalledTimes(1);
    expect(dispatch).toHaveBeenLastCalledWith("load-1", natural("3"), {
      $: "record",
      fields: [],
    });

    renderer.render(projection("load-2", "Updated"), natural("4"));
    expect(environment.document.intersections).toHaveLength(1);
    firstObserver.emit(false);
    firstObserver.emit(true);
    expect(dispatch).toHaveBeenLastCalledWith("load-2", natural("4"), {
      $: "record",
      fields: [],
    });

    renderer.render(renderNodes([
      elementNode("scroll", "scroll"),
    ]));
    expect(firstObserver.disconnected).toBe(true);
    dispatch.mockClear();
    firstObserver.emit(false);
    firstObserver.emit(true);
    expect(dispatch).not.toHaveBeenCalled();

    renderer.render(projection("load-3"), natural("5"));
    const replacementObserver = environment.document.intersections[1]!;
    expect(replacementObserver.disconnected).toBe(false);
    renderer.dispose();
    expect(replacementObserver.disconnected).toBe(true);
  });

  it("owns pager children, dots, and checked page-change dispatch", () => {
    const { root, element } = fakeRoot();
    const dispatch = vi.fn<ProjectionRendererOptions["dispatch"]>();
    const renderer = createProjectionRenderer({
      root,
      dispatch,
      mode: "play",
    });
    const pages = [
      elementNode("first", "view", [], [{
        kind: "text",
        key: "first-text",
        text: "First",
      }]),
      elementNode("second", "view", [], [{
        kind: "text",
        key: "second-text",
        text: "Second",
      }]),
    ] as const;
    renderer.render(renderNodes([
      elementNode("pager", "pager", [
        { name: "class", value: "gallery" },
        { name: "label", value: "Post media" },
        { name: "indicator", value: "dots" },
      ], pages, [{ event: "page-change", binding: "page" }]),
    ]), LIVE_REVISION);

    const pager = element.children[0]!;
    const track = pager.children.find(
      (child) => child.dataset["uhMechanic"] === "track",
    )!;
    const dots = pager.children.find(
      (child) => child.dataset["uhMechanic"] === "dots",
    )!;
    expect(pager.className).toBe("uh-pager gallery");
    expect(pager.getAttribute("role")).toBe("group");
    expect(pager.getAttribute("aria-label")).toBe("Post media");
    expect(pager.hasAttribute("label")).toBe(false);
    expect(pager.hasAttribute("indicator")).toBe(false);
    expect(track.className).toBe("uh-track");
    expect(track.children.map((child) => child.className)).toEqual([
      "uh-view",
      "uh-view",
    ]);
    expect(dots.className).toBe("uh-dots");
    expect(dots.children).toHaveLength(2);
    expect(dots.children[0]!.classList.contains("on")).toBe(true);

    track.clientWidth = 100;
    track.scrollLeft = 100;
    track.emit("scroll");
    expect(dots.children[0]!.classList.contains("on")).toBe(false);
    expect(dots.children[1]!.classList.contains("on")).toBe(true);
    expect(dispatch).toHaveBeenCalledTimes(1);
    expect(dispatch).toHaveBeenCalledWith("page", LIVE_REVISION, {
      $: "record",
      fields: [],
    });

    renderer.render(renderNodes([
      elementNode("pager", "pager", [
        { name: "label", value: "Post media" },
        { name: "indicator", value: "none" },
      ], pages, [{ event: "page-change", binding: "page-next" }]),
    ]), natural("2"));
    const updatedPager = element.children[0]!;
    expect(
      updatedPager.children.find(
        (child) => child.dataset["uhMechanic"] === "track",
      ),
    ).toBe(track);
    expect(
      updatedPager.children.some(
        (child) => child.dataset["uhMechanic"] === "dots",
      ),
    ).toBe(false);
    track.scrollLeft = 0;
    track.emit("scroll");
    expect(dispatch).toHaveBeenLastCalledWith("page-next", natural("2"), {
      $: "record",
      fields: [],
    });

    dispatch.mockClear();
    renderer.render(renderNodes([]));
    track.scrollLeft = 100;
    track.emit("scroll");
    expect(dispatch).not.toHaveBeenCalled();
  });

  it("realizes textfield as a wrapper and dispatches from its input mechanic", () => {
    const dispatch = vi.fn<ProjectionRendererOptions["dispatch"]>();
    const play = fakeRoot();
    createProjectionRenderer({
      root: play.root,
      dispatch,
      mode: "play",
    }).render(renderNodes([
      elementNode(
        "field",
        "textfield",
        [
          { name: "class", value: "query" },
          { name: "label", value: "Search" },
          { name: "placeholder", value: "Find people" },
          { name: "value", value: "mi" },
          { name: "disabled", value: false },
        ],
        [],
        [{ event: "change", binding: "search" }],
      ),
    ]), LIVE_REVISION);
    const wrapper = play.element.children[0]!;
    const input = wrapper.children[0]!;
    expect(wrapper.localName).toBe("div");
    expect(wrapper.className).toBe("uh-textfield query");
    expect(wrapper.hasAttribute("label")).toBe(false);
    expect(wrapper.hasAttribute("value")).toBe(false);
    expect(input.localName).toBe("input");
    expect(input.dataset["uhMechanic"]).toBe("input");
    expect(input.getAttribute("aria-label")).toBe("Search");
    expect(input.getAttribute("placeholder")).toBe("Find people");
    expect(input.value).toBe("mi");
    expect(input.readOnly).toBe(false);

    wrapper.emit("input");
    expect(dispatch).not.toHaveBeenCalled();
    input.value = "mira";
    input.emit("input");
    expect(dispatch).toHaveBeenCalledWith("search", LIVE_REVISION, {
      $: "record",
      fields: [{ name: "text", value: { $: "Text", value: "mira" } }],
    });

    const editorDispatch =
      vi.fn<ProjectionRendererOptions["dispatch"]>();
    const editor = fakeRoot();
    createProjectionRenderer({
      root: editor.root,
      dispatch: editorDispatch,
      mode: "editor",
    }).render(renderNodes([
      elementNode(
        "field",
        "textfield",
        [
          { name: "label", value: "Search" },
          { name: "value", value: "static" },
        ],
        [],
        [{ event: "change", binding: "search" }],
      ),
    ]));
    const editorInput = editor.element.children[0]!.children[0]!;
    expect(editorInput.readOnly).toBe(true);
    editorInput.value = "changed";
    editorInput.emit("input");
    expect(editorDispatch).not.toHaveBeenCalled();
  });

  it("keeps controlled textfield input coherent across synchronous projections", () => {
    const environment = fakeRoot();
    const projection = (value: string): RenderDocument => renderNodes([
      elementNode(
        "field",
        "textfield",
        [
          { name: "label", value: "Search" },
          { name: "value", value },
        ],
        [],
        [{ event: "change", binding: "search" }],
      ),
    ]);
    let renderer: ReturnType<typeof createProjectionRenderer>;
    const dispatch = vi.fn<ProjectionRendererOptions["dispatch"]>(() => {
      renderer.render(projection("canonical"), natural("8"));
    });
    renderer = createProjectionRenderer({
      root: environment.root,
      dispatch,
      mode: "play",
    });
    renderer.render(projection("model"), natural("7"));
    const input = environment.element.children[0]!.children[0]!;

    input.value = "browser";
    input.emit("input");
    expect(dispatch).toHaveBeenCalledTimes(1);
    expect(input.value).toBe("canonical");

    dispatch.mockClear();
    input.emit("compositionstart");
    input.value = "composing";
    input.emit("input", { isComposing: true });
    expect(dispatch).not.toHaveBeenCalled();
    expect(input.value).toBe("composing");
    input.emit("compositionend");
    expect(dispatch).toHaveBeenCalledTimes(1);
    expect(input.value).toBe("canonical");
  });

  it("filters media and icon semantics through renderer capabilities", () => {
    const applyImage = vi.fn<
      (image: HTMLImageElement, asset: string | undefined) => void
    >((image, asset) => {
      if (asset) image.setAttribute("src", `resolved:${asset}`);
    });
    const applyVideoSource = vi.fn<
      (video: HTMLVideoElement, asset: string | undefined) => void
    >((video, asset) => {
      if (asset) video.setAttribute("src", `resolved:${asset}`);
      else video.removeAttribute("src");
    });
    const applyVideoPoster = vi.fn<
      (video: HTMLVideoElement, asset: string | undefined) => void
    >((video, asset) => {
      if (asset) video.setAttribute("poster", `resolved:${asset}`);
    });
    const applyIcon = vi.fn<
      (
        host: HTMLElement,
        family: string | undefined,
        name: string,
      ) => void
    >((host, _family, name) => {
      host.textContent = `glyph:${name}`;
    });
    const capabilities = {
      assets: { applyImage, applyVideoSource, applyVideoPoster },
      icons: {
        defaultFamily: "lucide",
        fingerprint: "font-1",
        apply: applyIcon,
      },
    };

    const editor = fakeRoot();
    createProjectionRenderer({
      root: editor.root,
      dispatch: vi.fn<ProjectionRendererOptions["dispatch"]>(),
      mode: "editor",
      ...capabilities,
    }).render(renderNodes([
      elementNode("image", "img", [
        { name: "class", value: "avatar" },
        { name: "src", value: "avatar-mira" },
        { name: "decorative", value: true },
      ]),
      elementNode("video", "video", [
        { name: "src", value: "clip" },
        { name: "poster", value: "cover" },
        { name: "label", value: "Reel" },
        { name: "autoplay", value: true },
        { name: "muted", value: true },
        { name: "controls", value: true },
      ]),
      elementNode("icon", "icon", [
        { name: "name", value: "heart" },
        { name: "family", value: "lucide" },
      ]),
    ]));
    const image = editor.element.children[0]!;
    const video = editor.element.children[1]!;
    const icon = editor.element.children[2]!;
    expect(image.className).toBe("uh-img avatar");
    expect(image.getAttribute("alt")).toBe("");
    expect(image.getAttribute("src")).toBe("resolved:avatar-mira");
    expect(image.hasAttribute("decorative")).toBe(false);
    expect(applyImage).toHaveBeenCalledWith(image, "avatar-mira");
    expect(video.className).toBe("uh-video");
    expect(video.getAttribute("aria-label")).toBe("Reel");
    expect(video.getAttribute("data-video-preview")).toBe("poster");
    expect(video.hasAttribute("autoplay")).toBe(false);
    expect(video.hasAttribute("muted")).toBe(false);
    expect(video.hasAttribute("controls")).toBe(false);
    expect(video.autoplay).toBe(false);
    expect(video.muted).toBe(false);
    expect(applyVideoSource).toHaveBeenCalledWith(video, undefined);
    expect(applyVideoPoster).toHaveBeenCalledWith(video, "cover");
    expect(icon.className).toBe("uh-icon");
    expect(icon.getAttribute("aria-hidden")).toBe("true");
    expect(icon.hasAttribute("name")).toBe(false);
    expect(icon.hasAttribute("family")).toBe(false);
    expect(icon.dataset["icon"]).toBe("heart");
    expect(icon.textContent).toBe("glyph:heart");

    const play = fakeRoot();
    createProjectionRenderer({
      root: play.root,
      dispatch: vi.fn<ProjectionRendererOptions["dispatch"]>(),
      mode: "play",
      ...capabilities,
    }).render(renderNodes([
      elementNode("video", "video", [
        { name: "src", value: "clip" },
        { name: "poster", value: "cover" },
        { name: "label", value: "Reel" },
        { name: "autoplay", value: true },
        { name: "muted", value: true },
        { name: "loop", value: true },
        { name: "controls", value: true },
        { name: "playsinline", value: true },
      ]),
    ]));
    const playVideo = play.element.children[0]!;
    expect(playVideo.getAttribute("src")).toBe("resolved:clip");
    expect(playVideo.hasAttribute("data-video-preview")).toBe(false);
    expect(playVideo.hasAttribute("autoplay")).toBe(true);
    expect(playVideo.hasAttribute("muted")).toBe(true);
    expect(playVideo.hasAttribute("loop")).toBe(true);
    expect(playVideo.hasAttribute("controls")).toBe(true);
    expect(playVideo.hasAttribute("playsinline")).toBe(true);
    expect(playVideo.autoplay).toBe(true);
    expect(playVideo.muted).toBe(true);
    expect(playVideo.loop).toBe(true);
    expect(playVideo.controls).toBe(true);
    expect(playVideo.playsInline).toBe(true);
    expect(applyVideoSource).toHaveBeenLastCalledWith(playVideo, "clip");
  });
});

// The repository intentionally has no synthetic DOM package. These checks run
// when Vitest is given a browser environment; Play's browser smoke test is the
// required renderer gate in the default toolchain.
const describeDom = typeof window === "undefined" ? describe.skip : describe;

describeDom("Uhura view renderer", () => {
  it("reconciles stable keyed DOM and dispatches semantic events", () => {
    const root = window.document.createElement("div");
    const dispatch = vi.fn<ProjectionRendererOptions["dispatch"]>();
    const observeElement = vi.fn<(key: string, element: HTMLElement) => void>();
    const renderer = createProjectionRenderer({ root, dispatch, observeElement });
    renderer.render(documentValue("First"));
    const button = root.querySelector("button");
    expect(button?.textContent).toBe("First");
    expect(observeElement).toHaveBeenCalledWith("button", button);
    observeElement.mockClear();
    renderer.render(documentValue("Second"));
    expect(root.querySelector("button")).toBe(button);
    expect(observeElement).toHaveBeenCalledWith("button", button);
    button?.click();
    expect(dispatch).toHaveBeenCalledWith("press-1", undefined, {
      $: "record",
      fields: [],
    });
  });

  it("keeps a keyed Surface physical dialog across projections", () => {
    const root = window.document.createElement("div");
    const renderer = createProjectionRenderer({
      root,
      dispatch: () => undefined,
    });
    const surface: RenderDocument = {
      ...documentValue("x"),
      nodes: [
        {
          kind: "element",
          key: "surface-7",
          element: "dialog",
          attributes: [],
          events: [],
          surface: true,
          children: [{ kind: "text", key: "surface-text", text: "Policy" }],
        },
      ],
    };
    renderer.render(surface);
    const dialog = root.querySelector("dialog");
    const surfaceRoot = surface.nodes[0];
    if (surfaceRoot?.kind !== "element") {
      throw new Error("expected the surface fixture root");
    }
    renderer.render({
      ...surface,
      nodes: [
        {
          ...surfaceRoot,
          children: [{ kind: "text", key: "surface-text", text: "Updated" }],
        },
      ],
    });
    expect(root.querySelector("dialog")).toBe(dialog);
    expect(dialog?.textContent).toBe("Updated");
  });
});
