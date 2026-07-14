import assert from "node:assert/strict";
import { test } from "vitest";

import { mountPlayChrome } from "../chrome.js";
import type { PlayShell } from "../shell.js";

type Listener = (event: Event) => void;

class FakeEventTarget {
  private readonly listeners = new Map<string, Set<Listener>>();

  addEventListener(type: string, listener: Listener): void {
    const listeners = this.listeners.get(type) ?? new Set<Listener>();
    listeners.add(listener);
    this.listeners.set(type, listeners);
  }

  removeEventListener(type: string, listener: Listener): void {
    this.listeners.get(type)?.delete(listener);
  }

  fire(type: string, properties: Record<string, unknown> = {}): void {
    const event = { type, target: this, ...properties } as unknown as Event;
    for (const listener of this.listeners.get(type) ?? []) listener(event);
  }

  listenerCount(type: string): number {
    return this.listeners.get(type)?.size ?? 0;
  }
}

class FakeElement extends FakeEventTarget {
  readonly attributes = new Map<string, string>();
  readonly children: FakeElement[] = [];
  readonly dataset: Record<string, string> = {};
  readonly style: Record<string, string> = {};
  clientHeight = 900;
  clientWidth = 1_400;
  disabled = false;
  hidden = false;
  parent: FakeElement | undefined;
  selected = false;
  textContent = "";
  title = "";
  value = "";
  private readonly queries = new Map<string, FakeElement>();

  get firstChild(): FakeElement | null {
    return this.children[0] ?? null;
  }

  append(...children: FakeElement[]): void {
    for (const child of children) {
      child.parent = this;
      this.children.push(child);
    }
  }

  contains(target: unknown): boolean {
    let candidate = target instanceof FakeElement ? target : undefined;
    while (candidate) {
      if (candidate === this) return true;
      candidate = candidate.parent;
    }
    return false;
  }

  getAttribute(name: string): string | null {
    return this.attributes.get(name) ?? null;
  }

  querySelector(selector: string): FakeElement | null {
    return this.queries.get(selector) ?? null;
  }

  remove(): void {
    if (!this.parent) return;
    const index = this.parent.children.indexOf(this);
    if (index >= 0) this.parent.children.splice(index, 1);
    this.parent = undefined;
  }

  replaceChildren(...children: FakeElement[]): void {
    for (const child of this.children) child.parent = undefined;
    this.children.length = 0;
    this.append(...children);
  }

  setAttribute(name: string, value: string): void {
    this.attributes.set(name, value);
  }

  setQuery(selector: string, element: FakeElement): void {
    this.queries.set(selector, element);
  }
}

class FakeWindow extends FakeEventTarget {
  private nextTimer = 1;
  private readonly timers = new Map<number, () => void>();

  clearTimeout(timer: number): void {
    this.timers.delete(timer);
  }

  pendingTimerCount(): number {
    return this.timers.size;
  }

  runTimers(): void {
    const timers = [...this.timers.values()];
    this.timers.clear();
    for (const timer of timers) timer();
  }

  setTimeout(callback: () => void): number {
    const timer = this.nextTimer;
    this.nextTimer += 1;
    this.timers.set(timer, callback);
    return timer;
  }

  timerCallback(): (() => void) | undefined {
    return this.timers.values().next().value;
  }
}

class FakeDocument extends FakeEventTarget {
  activeElement: FakeElement | null = null;
  readonly defaultView: FakeWindow;
  readonly documentElement = new FakeElement();
  fullscreenElement: FakeElement | null = null;

  constructor(defaultView: FakeWindow) {
    super();
    this.defaultView = defaultView;
  }

  createElement(): FakeElement {
    return new FakeElement();
  }

  createTextNode(text: string): FakeElement {
    const node = new FakeElement();
    node.textContent = text;
    return node;
  }

  exitFullscreen(): Promise<void> {
    return Promise.resolve();
  }
}

function harness() {
  const view = new FakeWindow();
  const document = new FakeDocument(view);
  const container = new FakeElement();
  const toolbar = new FakeElement();
  const toolbarControl = new FakeElement();
  const toolbarControlTwo = new FakeElement();
  toolbar.append(toolbarControl, toolbarControlTwo);
  container.append(toolbar);
  container.setQuery("#uh-shell-toolbar", toolbar);

  const restart = new FakeElement();
  const stage = new FakeElement();
  const frame = new FakeElement();
  const frameSizer = new FakeElement();
  const frameLabel = new FakeElement();
  container.append(restart, stage);
  stage.append(frameSizer);
  frameSizer.append(frame);

  document.documentElement.setQuery("requestFullscreen", new FakeElement());
  Object.assign(document.documentElement, {
    requestFullscreen: (): Promise<void> => Promise.resolve(),
  });

  const element = (): FakeElement => new FakeElement();
  const shell = {
    document,
    container,
    stage,
    frame,
    frameSizer,
    frameLabel,
    frameButtons: [],
    runtimeStatus: element(),
    providerControl: element(),
    providerSelect: element(),
    actorSelect: element(),
    debugToggle: element(),
    debugPanel: element(),
    debugTitle: element(),
    debugClose: element(),
    debugDefinition: element(),
    debugFollowLive: element(),
    debugSummary: element(),
    debugGraph: element(),
    debugDetails: element(),
    fullscreen: element(),
    restart,
    pageHost: element(),
    surfaceHost: element(),
    overlayHost: element(),
  } as unknown as PlayShell;

  const chrome = mountPlayChrome(shell, {
    window: view as unknown as Window,
    storage: {
      getItem: () => null,
      setItem: () => undefined,
    },
    createResizeObserver: () => ({
      observe: () => undefined,
      disconnect: () => undefined,
    }),
  });

  return {
    chrome,
    container,
    document,
    restart,
    toolbar,
    toolbarControl,
    toolbarControlTwo,
    view,
  };
}

test("keeps Play chrome visible while focus is inside it and resumes hiding on focusout", () => {
  const {
    chrome,
    container,
    document,
    restart,
    toolbar,
    toolbarControl,
    toolbarControlTwo,
    view,
  } = harness();
  const pendingHide = view.timerCallback();
  assert.ok(pendingHide);

  document.activeElement = toolbarControl;
  toolbar.fire("focusin");
  assert.equal(view.pendingTimerCount(), 0);
  assert.equal(container.dataset["uiHidden"], undefined);

  pendingHide();
  assert.equal(container.dataset["uiHidden"], undefined);

  document.activeElement = toolbarControlTwo;
  toolbar.fire("focusout", { relatedTarget: toolbarControlTwo });
  assert.equal(view.pendingTimerCount(), 0);

  document.activeElement = restart;
  toolbar.fire("focusout", { relatedTarget: restart });
  restart.fire("focusin");
  assert.equal(view.pendingTimerCount(), 0);

  const appControl = new FakeElement();
  restart.fire("focusout", { relatedTarget: appControl });
  assert.equal(view.pendingTimerCount(), 1);

  document.activeElement = appControl;
  view.runTimers();
  assert.equal(container.dataset["uiHidden"], "true");

  document.activeElement = toolbarControl;
  toolbar.fire("focusin");
  assert.equal(container.dataset["uiHidden"], undefined);
  assert.equal(view.pendingTimerCount(), 0);

  chrome.dispose();
});

test("disposal removes chrome focus listeners and cancels the hide timer", () => {
  const { chrome, container, restart, toolbar, toolbarControl, view } = harness();
  assert.equal(toolbar.listenerCount("focusin"), 1);
  assert.equal(toolbar.listenerCount("focusout"), 1);
  assert.equal(restart.listenerCount("focusin"), 1);
  assert.equal(restart.listenerCount("focusout"), 1);
  assert.equal(view.pendingTimerCount(), 1);

  chrome.dispose();

  assert.equal(toolbar.listenerCount("focusin"), 0);
  assert.equal(toolbar.listenerCount("focusout"), 0);
  assert.equal(restart.listenerCount("focusin"), 0);
  assert.equal(restart.listenerCount("focusout"), 0);
  assert.equal(view.pendingTimerCount(), 0);

  container.dataset["uiHidden"] = "true";
  toolbar.fire("focusin", { target: toolbarControl });
  assert.equal(container.dataset["uiHidden"], "true");
});
