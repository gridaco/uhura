import assert from "node:assert/strict";
import { test } from "vitest";

import {
  anchoredDebugViewportScroll,
  clampDebugViewportZoom,
  createDebugViewport,
  fitDebugViewportZoom,
  wheelDebugViewportZoom,
  type DebugViewportWindow,
} from "../debug-viewport.js";

type Listener = (event: Record<string, unknown>) => void;

class FakeEventTarget {
  private readonly listeners = new Map<string, Set<Listener>>();

  addEventListener(
    type: string,
    listener: Listener,
    _options?: unknown,
  ): void {
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
      target: this,
      button: 0,
      clientX: 0,
      clientY: 0,
      pointerId: 1,
      pointerType: "mouse",
      isPrimary: true,
      ctrlKey: false,
      metaKey: false,
      deltaY: 0,
      deltaMode: 0,
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
  blockSize = "";
  inlineSize = "";
  transform = "";
  transformOrigin = "";
}

class FakeElement extends FakeEventTarget {
  readonly attributes = new Map<string, string>();
  readonly captures = new Set<number>();
  readonly dataset: Record<string, string> = {};
  readonly style = new FakeStyle();
  readonly ownerDocument: { defaultView: DebugViewportWindow | null };
  clientHeight = 300;
  clientLeft = 0;
  clientTop = 0;
  clientWidth = 400;
  disabled = false;
  scrollLeft = 0;
  scrollTop = 0;
  textContent: string | null = null;

  constructor(view: DebugViewportWindow | null) {
    super();
    this.ownerDocument = { defaultView: view };
  }

  getBoundingClientRect(): { left: number; top: number } {
    return { left: 10, top: 20 };
  }

  hasPointerCapture(pointerId: number): boolean {
    return this.captures.has(pointerId);
  }

  releasePointerCapture(pointerId: number): void {
    this.captures.delete(pointerId);
  }

  removeAttribute(name: string): void {
    this.attributes.delete(name);
    if (name === "data-panning") delete this.dataset["panning"];
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

class FakeWindow extends FakeEventTarget {}

function asElement(element: FakeElement): HTMLElement {
  return element as unknown as HTMLElement;
}

function asButton(element: FakeElement): HTMLButtonElement {
  return element as unknown as HTMLButtonElement;
}

function closeTo(actual: number, expected: number): void {
  assert.ok(
    Math.abs(actual - expected) < 0.000_001,
    `${String(actual)} should be close to ${String(expected)}`,
  );
}

function harness() {
  const view = new FakeWindow();
  const viewport = new FakeElement(view as unknown as DebugViewportWindow);
  const content = new FakeElement(view as unknown as DebugViewportWindow);
  const zoomIn = new FakeElement(view as unknown as DebugViewportWindow);
  const zoomOut = new FakeElement(view as unknown as DebugViewportWindow);
  const zoomReset = new FakeElement(view as unknown as DebugViewportWindow);
  const zoomOutput = new FakeElement(view as unknown as DebugViewportWindow);
  const controller = createDebugViewport({
    viewport: asElement(viewport),
    content: asElement(content),
    zoomIn: asButton(zoomIn),
    zoomOut: asButton(zoomOut),
    zoomReset: asButton(zoomReset),
    zoomOutput: asElement(zoomOutput),
    window: view as unknown as DebugViewportWindow,
  });
  return {
    canvas: new FakeElement(view as unknown as DebugViewportWindow),
    content,
    controller,
    view,
    viewport,
    zoomIn,
    zoomOut,
    zoomOutput,
    zoomReset,
  };
}

test("pure zoom math clamps, fits, and preserves an arbitrary anchor", () => {
  assert.equal(clampDebugViewportZoom(0.1), 0.5);
  assert.equal(clampDebugViewportZoom(1.25), 1.25);
  assert.equal(clampDebugViewportZoom(9), 2);
  assert.throws(() => clampDebugViewportZoom(Number.NaN), /finite/);

  assert.equal(
    fitDebugViewportZoom(
      { width: 400, height: 300 },
      { width: 1_000, height: 600 },
    ),
    0.5,
  );
  assert.equal(
    fitDebugViewportZoom(
      { width: 400, height: 300 },
      { width: 100, height: 100 },
    ),
    2,
  );

  assert.deepEqual(
    anchoredDebugViewportScroll(
      { x: 100, y: 50 },
      { x: 100, y: 75 },
      1,
      2,
    ),
    { x: 300, y: 175 },
  );
  closeTo(wheelDebugViewportZoom(1, -100, 0, 300), Math.exp(0.2));
  assert.equal(wheelDebugViewportZoom(2, -100, 1, 300), 2);
});

test("layout, direct zoom, buttons, fit, and clear own deterministic geometry", () => {
  const {
    canvas,
    content,
    controller,
    viewport,
    zoomIn,
    zoomOut,
    zoomOutput,
    zoomReset,
  } = harness();

  assert.equal(zoomOutput.textContent, "100%");
  assert.equal(zoomOutput.attribute("aria-label"), "Graph zoom 100%");
  assert.equal(zoomIn.disabled, true);
  assert.equal(zoomOut.disabled, true);

  controller.setLayout(asElement(canvas), 1_000, 600);
  assert.equal(controller.hasLayout, true);
  assert.equal(content.style.inlineSize, "1000px");
  assert.equal(content.style.blockSize, "600px");
  assert.equal(canvas.style.transformOrigin, "0 0");
  assert.equal(canvas.style.transform, "scale(1)");
  assert.equal(zoomIn.disabled, false);
  assert.equal(zoomOut.disabled, false);
  assert.equal(zoomReset.disabled, true);

  viewport.scrollLeft = 100;
  viewport.scrollTop = 50;
  assert.equal(controller.setZoom(9, { x: 100, y: 75 }), true);
  assert.equal(controller.zoom, 2);
  assert.equal(viewport.scrollLeft, 300);
  assert.equal(viewport.scrollTop, 175);
  assert.equal(content.style.inlineSize, "2000px");
  assert.equal(canvas.style.transform, "scale(2)");
  assert.equal(zoomOutput.textContent, "200%");
  assert.equal(zoomIn.disabled, true);

  zoomOut.fire("click");
  assert.equal(controller.zoom, 1.9);
  assert.equal(zoomOutput.textContent, "190%");
  zoomReset.fire("click");
  assert.equal(controller.zoom, 1);
  assert.equal(zoomReset.disabled, true);
  assert.equal(controller.fit(), true);
  assert.equal(controller.zoom, 0.5);
  assert.equal(content.style.inlineSize, "500px");
  assert.equal(content.style.blockSize, "300px");

  controller.clearLayout();
  assert.equal(controller.hasLayout, false);
  assert.equal(controller.zoom, 1);
  assert.equal(content.style.inlineSize, "");
  assert.equal(content.style.blockSize, "");
  assert.equal(canvas.style.transform, "");
  assert.equal(canvas.style.transformOrigin, "");
  assert.equal(viewport.scrollLeft, 0);
  assert.equal(viewport.scrollTop, 0);
  assert.equal(zoomOutput.textContent, "100%");
  assert.equal(zoomReset.disabled, true);
});

test("modified wheel anchors at the pointer while ordinary wheel stays native", () => {
  const { canvas, controller, viewport } = harness();
  controller.setLayout(asElement(canvas), 1_000, 600);
  viewport.scrollLeft = 100;
  viewport.scrollTop = 50;

  const ordinary = viewport.fire("wheel", { deltaY: -100 });
  assert.equal(ordinary.prevented, false);
  assert.equal(controller.zoom, 1);
  assert.equal(viewport.scrollLeft, 100);

  const modified = viewport.fire("wheel", {
    clientX: 110,
    clientY: 95,
    ctrlKey: true,
    deltaY: -100,
  });
  assert.equal(modified.prevented, true);
  const nextZoom = Math.exp(0.2);
  closeTo(controller.zoom, nextZoom);
  closeTo(viewport.scrollLeft, 200 * nextZoom - 100);
  closeTo(viewport.scrollTop, 125 * nextZoom - 75);

  controller.clearLayout();
  const emptyPinch = viewport.fire("wheel", {
    metaKey: true,
    deltaY: -10,
  });
  assert.equal(emptyPinch.prevented, true);
  assert.equal(controller.zoom, 1);
});

test("mouse and pen pan backgrounds without intercepting controls or touch", () => {
  const { canvas, controller, viewport } = harness();
  controller.setLayout(asElement(canvas), 1_000, 600);
  viewport.scrollLeft = 300;
  viewport.scrollTop = 200;

  const interactive = { closest: () => ({ tagName: "BUTTON" }) };
  const nodeDown = viewport.fire("pointerdown", {
    target: interactive,
    clientX: 100,
    clientY: 100,
    pointerId: 4,
  });
  assert.equal(nodeDown.prevented, false);
  assert.deepEqual([...viewport.captures], []);

  const touchDown = viewport.fire("pointerdown", {
    clientX: 100,
    clientY: 100,
    pointerId: 5,
    pointerType: "touch",
  });
  assert.equal(touchDown.prevented, false);

  const background = { closest: () => null };
  const penDown = viewport.fire("pointerdown", {
    target: background,
    clientX: 150,
    clientY: 120,
    pointerId: 7,
    pointerType: "pen",
  });
  assert.equal(penDown.prevented, true);
  assert.deepEqual([...viewport.captures], [7]);
  assert.equal(viewport.dataset["panning"], "true");

  const move = viewport.fire("pointermove", {
    clientX: 120,
    clientY: 150,
    pointerId: 7,
  });
  assert.equal(move.prevented, true);
  assert.equal(viewport.scrollLeft, 330);
  assert.equal(viewport.scrollTop, 170);

  const up = viewport.fire("pointerup", { pointerId: 7 });
  assert.equal(up.prevented, true);
  assert.deepEqual([...viewport.captures], []);
  assert.equal(viewport.dataset["panning"], undefined);
});

test("replacing a canvas retains the logical center and disposal removes wiring", () => {
  const {
    canvas,
    content,
    controller,
    view,
    viewport,
    zoomIn,
    zoomOut,
    zoomReset,
  } = harness();
  controller.setLayout(asElement(canvas), 1_000, 600);
  controller.setZoom(1.5);
  viewport.scrollLeft = 300;
  viewport.scrollTop = 150;
  const logicalCenter = {
    x: (viewport.scrollLeft + viewport.clientWidth / 2) / controller.zoom,
    y: (viewport.scrollTop + viewport.clientHeight / 2) / controller.zoom,
  };

  const replacement = new FakeElement(view as unknown as DebugViewportWindow);
  controller.setLayout(asElement(replacement), 1_200, 800);
  closeTo(
    (viewport.scrollLeft + viewport.clientWidth / 2) / controller.zoom,
    logicalCenter.x,
  );
  closeTo(
    (viewport.scrollTop + viewport.clientHeight / 2) / controller.zoom,
    logicalCenter.y,
  );
  assert.equal(canvas.style.transform, "");
  assert.equal(replacement.style.transform, "scale(1.5)");
  assert.equal(content.style.inlineSize, "1800px");
  assert.equal(content.style.blockSize, "1200px");

  viewport.fire("pointerdown", {
    target: { closest: () => null },
    clientX: 100,
    clientY: 100,
    pointerId: 12,
  });
  assert.deepEqual([...viewport.captures], [12]);
  assert.equal(controller.dispose(), true);
  assert.equal(controller.dispose(), false);
  assert.equal(controller.isDisposed, true);
  assert.equal(viewport.listenerCount(), 0);
  assert.equal(view.listenerCount(), 0);
  assert.equal(zoomIn.listenerCount(), 0);
  assert.equal(zoomOut.listenerCount(), 0);
  assert.equal(zoomReset.listenerCount(), 0);
  assert.deepEqual([...viewport.captures], []);
  assert.equal(content.style.inlineSize, "");
  assert.equal(replacement.style.transform, "");

  zoomIn.fire("click");
  viewport.fire("wheel", { ctrlKey: true, deltaY: -100 });
  assert.equal(controller.zoom, 1);
});
