import assert from "node:assert/strict";
import { test } from "vitest";

import {
  createDebugResizeController,
  debugResizeGeometry,
  debugResizeMode,
  type DebugResizeViewport,
} from "../debug-resize.js";

type Listener = (event: Record<string, unknown>) => void;

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

  fire(type: string, properties: Record<string, unknown> = {}) {
    let prevented = false;
    const event = {
      type,
      button: 0,
      clientX: 0,
      clientY: 0,
      pointerId: 1,
      key: "",
      shiftKey: false,
      preventDefault: () => {
        prevented = true;
      },
      ...properties,
    };
    for (const listener of [...(this.listeners.get(type) ?? [])]) {
      listener(event);
    }
    return { prevented };
  }

  listenerCount(): number {
    let count = 0;
    for (const listeners of this.listeners.values()) count += listeners.size;
    return count;
  }
}

class FakeStyle {
  readonly values = new Map<string, string>();

  setProperty(name: string, value: string): void {
    this.values.set(name, value);
  }

  get(name: string): string | undefined {
    return this.values.get(name);
  }
}

class FakeElement extends FakeEventTarget {
  readonly attributes = new Map<string, string>();
  readonly captures = new Set<number>();
  readonly style = new FakeStyle();
  height = 848;
  id = "";

  getBoundingClientRect(): { height: number } {
    return { height: this.height };
  }

  hasPointerCapture(pointerId: number): boolean {
    return this.captures.has(pointerId);
  }

  releasePointerCapture(pointerId: number): void {
    this.captures.delete(pointerId);
  }

  removeAttribute(name: string): void {
    this.attributes.delete(name);
  }

  setAttribute(name: string, value: string): void {
    this.attributes.set(name, value);
  }

  setPointerCapture(pointerId: number): void {
    this.captures.add(pointerId);
  }

  attribute(name: string): string | undefined {
    return this.attributes.get(name);
  }
}

class FakeViewport extends FakeEventTarget implements DebugResizeViewport {
  innerWidth: number;
  innerHeight: number;

  constructor(width: number, height: number) {
    super();
    this.innerWidth = width;
    this.innerHeight = height;
  }

  resize(width: number, height = this.innerHeight): void {
    this.innerWidth = width;
    this.innerHeight = height;
    this.fire("resize");
  }
}

function asElement(element: FakeElement): HTMLElement {
  return element as unknown as HTMLElement;
}

function harness(width = 1_400, height = 900, panelHeight = 848) {
  const viewport = new FakeViewport(width, height);
  const container = new FakeElement();
  const panel = new FakeElement();
  panel.id = "uh-debug-panel";
  panel.height = panelHeight;
  const panelSeparator = new FakeElement();
  const details = new FakeElement();
  details.id = "uh-debug-details";
  const detailsSeparator = new FakeElement();
  const controller = createDebugResizeController({
    container: asElement(container),
    panel: asElement(panel),
    panelSeparator: asElement(panelSeparator),
    details: asElement(details),
    detailsSeparator: asElement(detailsSeparator),
    viewport,
  });
  return {
    container,
    controller,
    detailsSeparator,
    panel,
    panelSeparator,
    viewport,
  };
}

test("selects responsive modes at the debugger CSS breakpoints", () => {
  assert.equal(debugResizeMode(1_101), "wide");
  assert.equal(debugResizeMode(1_100), "bottom");
  assert.equal(debugResizeMode(621), "bottom");
  assert.equal(debugResizeMode(620), "compact");

  const short = debugResizeGeometry(1_000, 300);
  assert.deepEqual(short.panelBlock, { min: 160, max: 160 });
  assert.equal(
    short.detailsBlock.min <= short.detailsBlock.max,
    true,
    "short viewports retain valid clamp ranges",
  );
  assert.equal(
    Math.round(debugResizeGeometry(1_400, 900, 848).detailsBlock.max),
    466,
    "controller bounds match the CSS 55% drawer cap",
  );
});

test("syncs separator orientation, bounds, and disabled state across modes", () => {
  const { controller, detailsSeparator, panelSeparator, viewport } = harness();

  assert.equal(controller.mode, "wide");
  assert.equal(panelSeparator.attribute("aria-orientation"), "vertical");
  assert.equal(panelSeparator.attribute("aria-disabled"), "false");
  assert.equal(panelSeparator.attribute("aria-valuemin"), "320");
  assert.equal(panelSeparator.attribute("aria-valuemax"), "920");
  assert.equal(panelSeparator.attribute("aria-valuenow"), "476");
  assert.equal(panelSeparator.attribute("aria-controls"), "uh-debug-panel");
  assert.equal(detailsSeparator.attribute("aria-orientation"), "horizontal");
  assert.equal(detailsSeparator.attribute("aria-disabled"), "false");

  viewport.resize(1_000);
  assert.equal(controller.mode, "bottom");
  assert.equal(panelSeparator.attribute("aria-orientation"), "horizontal");
  assert.equal(panelSeparator.attribute("aria-disabled"), "false");
  assert.equal(detailsSeparator.attribute("aria-orientation"), "vertical");
  assert.equal(detailsSeparator.attribute("aria-disabled"), "true");
  assert.equal(detailsSeparator.attribute("aria-valuemin"), "0");
  assert.equal(detailsSeparator.attribute("tabindex"), "-1");

  viewport.resize(620);
  assert.equal(controller.mode, "compact");
  assert.equal(panelSeparator.attribute("aria-disabled"), "true");
  assert.equal(panelSeparator.attribute("tabindex"), "-1");
  assert.equal(detailsSeparator.attribute("aria-orientation"), "horizontal");
  assert.equal(detailsSeparator.attribute("aria-disabled"), "false");
  assert.equal(detailsSeparator.attribute("tabindex"), "0");
});

test("wide pointer and keyboard resizing clamp the panel and release capture", () => {
  const { container, panelSeparator } = harness();

  const down = panelSeparator.fire("pointerdown", {
    clientX: 924,
    pointerId: 7,
  });
  assert.equal(down.prevented, true);
  assert.deepEqual([...panelSeparator.captures], [7]);
  assert.equal(panelSeparator.attribute("data-resizing"), "true");
  assert.equal(container.attribute("data-debug-resizing"), "true");
  assert.equal(container.attribute("data-debug-resize-axis"), "inline");

  panelSeparator.fire("pointermove", { clientX: -1_000, pointerId: 7 });
  assert.equal(container.style.get("--uh-debug-inline-size"), "920px");
  assert.equal(panelSeparator.attribute("aria-valuenow"), "920");

  panelSeparator.fire("pointermove", { clientX: 2_000, pointerId: 7 });
  assert.equal(container.style.get("--uh-debug-inline-size"), "320px");

  panelSeparator.fire("pointerup", { pointerId: 7 });
  assert.deepEqual([...panelSeparator.captures], []);
  assert.equal(panelSeparator.attribute("data-resizing"), undefined);
  assert.equal(container.attribute("data-debug-resizing"), undefined);
  assert.equal(container.attribute("data-debug-resize-axis"), undefined);

  const left = panelSeparator.fire("keydown", { key: "ArrowLeft" });
  assert.equal(left.prevented, true);
  assert.equal(container.style.get("--uh-debug-inline-size"), "336px");
  panelSeparator.fire("keydown", { key: "End" });
  assert.equal(container.style.get("--uh-debug-inline-size"), "920px");
  panelSeparator.fire("keydown", { key: "ArrowRight", shiftKey: true });
  assert.equal(container.style.get("--uh-debug-inline-size"), "872px");
});

test("bottom dock resizes vertically while its side details splitter is inert", () => {
  const { container, detailsSeparator, panelSeparator, viewport } = harness(
    1_000,
    900,
  );
  assert.equal(container.style.get("--uh-debug-block-size"), "396px");

  panelSeparator.fire("pointerdown", { clientY: 504, pointerId: 2 });
  panelSeparator.fire("pointermove", { clientY: 404, pointerId: 2 });
  panelSeparator.fire("pointerup", { pointerId: 2 });
  assert.equal(container.style.get("--uh-debug-block-size"), "496px");

  panelSeparator.fire("keydown", { key: "ArrowDown" });
  assert.equal(container.style.get("--uh-debug-block-size"), "480px");
  panelSeparator.fire("keydown", { key: "End" });
  assert.equal(container.style.get("--uh-debug-block-size"), "660px");

  const before = container.style.get("--uh-debug-details-size");
  const down = detailsSeparator.fire("pointerdown", {
    clientY: 600,
    pointerId: 9,
  });
  const key = detailsSeparator.fire("keydown", { key: "ArrowUp" });
  assert.equal(down.prevented, false);
  assert.equal(key.prevented, false);
  assert.equal(container.style.get("--uh-debug-details-size"), before);
  assert.deepEqual([...detailsSeparator.captures], []);

  viewport.resize(600, 900);
  assert.equal(
    container.style.get("--uh-debug-details-size"),
    before,
    "the side-column mode preserves the drawer preference for compact mode",
  );
});

test("details drawer resizes in wide and compact layouts and resets on double click", () => {
  const { container, detailsSeparator, viewport } = harness();

  detailsSeparator.fire("pointerdown", { clientY: 668, pointerId: 3 });
  detailsSeparator.fire("pointermove", { clientY: 568, pointerId: 3 });
  detailsSeparator.fire("pointerup", { pointerId: 3 });
  assert.equal(container.style.get("--uh-debug-details-size"), "340px");

  detailsSeparator.fire("keydown", { key: "Home" });
  assert.equal(container.style.get("--uh-debug-details-size"), "96px");
  detailsSeparator.fire("dblclick");
  assert.equal(container.style.get("--uh-debug-details-size"), "240px");

  viewport.resize(620, 700);
  detailsSeparator.fire("keydown", { key: "ArrowUp" });
  assert.equal(container.style.get("--uh-debug-details-size"), "256px");
  assert.equal(detailsSeparator.attribute("aria-valuenow"), "256");

  const before = container.style.get("--uh-debug-block-size");
  const disabled = viewport.resize.bind(viewport);
  assert.doesNotThrow(() => disabled(620, 700));
  assert.equal(container.style.get("--uh-debug-block-size"), before);
});

test("mode changes cancel a drag and disposal removes every listener", () => {
  const {
    container,
    controller,
    detailsSeparator,
    panelSeparator,
    viewport,
  } = harness();
  const initialListenerCount =
    panelSeparator.listenerCount() + detailsSeparator.listenerCount();
  assert.equal(initialListenerCount, 14);
  assert.equal(viewport.listenerCount(), 1);

  panelSeparator.fire("pointerdown", { clientX: 900, pointerId: 12 });
  assert.deepEqual([...panelSeparator.captures], [12]);
  viewport.resize(1_000);
  assert.deepEqual([...panelSeparator.captures], []);
  assert.equal(panelSeparator.attribute("data-resizing"), undefined);

  const sizeAtDispose = container.style.get("--uh-debug-block-size");
  assert.equal(controller.dispose(), true);
  assert.equal(controller.dispose(), false);
  assert.equal(controller.isDisposed, true);
  assert.equal(panelSeparator.listenerCount(), 0);
  assert.equal(detailsSeparator.listenerCount(), 0);
  assert.equal(viewport.listenerCount(), 0);

  panelSeparator.fire("keydown", { key: "ArrowUp" });
  viewport.resize(800, 500);
  controller.syncLayout();
  assert.equal(container.style.get("--uh-debug-block-size"), sizeAtDispose);
});

test("responsive modes preserve sizing preferences that are currently dormant", () => {
  const wide = harness();
  wide.panelSeparator.fire("keydown", { key: "End" });
  assert.equal(wide.container.style.get("--uh-debug-inline-size"), "920px");
  wide.viewport.resize(1_000);
  assert.equal(wide.container.style.get("--uh-debug-inline-size"), "920px");
  wide.viewport.resize(1_400);
  assert.equal(wide.container.style.get("--uh-debug-inline-size"), "920px");

  const compact = harness(600, 900);
  assert.equal(compact.container.style.get("--uh-debug-inline-size"), "360px");
  compact.viewport.resize(1_200);
  assert.equal(compact.container.style.get("--uh-debug-inline-size"), "360px");
});
