import assert from "node:assert/strict";
import { test } from "vitest";

import { RealizationResources } from "../editor-realization.js";

test("registry resolves semantic keys and transfers one live owner", () => {
  const resources = new RealizationResources();
  const firstOwner = {};
  const nextOwner = {};
  const element = {} as HTMLElement;
  resources.claim(firstOwner);
  resources.registerKey("sheet:2/action", element);

  assert.equal(resources.resolve("sheet:2/action"), element);
  assert.equal(resources.resolve("sheet:2/missing"), null);
  resources.transfer(firstOwner, nextOwner);
  resources.release(firstOwner);
  assert.equal(resources.disposed, false, "the old model cannot dispose a transplanted registry");
  resources.release(nextOwner);
  assert.equal(resources.disposed, true);
  assert.equal(resources.resolve("sheet:2/action"), null);
});

test("registry resolves  semantic keys without ShadowRoot queries", () => {
  const resources = new RealizationResources();
  const owner = {};
  const element = {} as HTMLElement;
  resources.claim(owner);
  resources.registerKey("main/action", element);

  assert.equal(resources.resolve("main/action"), element);
  assert.equal(resources.resolve("main/missing"), null);
});

test("unused candidate resources dispose independently", () => {
  const retained = new RealizationResources();
  const candidate = new RealizationResources();
  const previousOwner = {};
  const nextOwner = {};
  retained.claim(previousOwner);
  candidate.claim(nextOwner);

  retained.transfer(previousOwner, nextOwner);
  candidate.release(nextOwner);
  assert.equal(retained.disposed, false);
  assert.equal(candidate.disposed, true);
});

test("watchers move with ownership and release scroll/resize resources", () => {
  const root = new EventTarget();
  const frame = new EventTarget() as HTMLElement;
  const realized = Object.assign(new EventTarget(), {
    getRootNode: () => root,
  }) as HTMLElement;
  let observed = 0;
  let disconnected = 0;
  class FakeResizeObserver {
    constructor(_callback: ResizeObserverCallback) {}
    observe(_target: Element): void {
      observed += 1;
    }
    disconnect(): void {
      disconnected += 1;
    }
  }
  const window = {
    ResizeObserver: FakeResizeObserver,
  } as unknown as Window;
  const firstOwner = {};
  const nextOwner = {};
  const resources = new RealizationResources();
  resources.claim(firstOwner);
  resources.registerKey("root", realized);
  let firstInvalidations = 0;
  resources.watch(firstOwner, frame, window, () => { firstInvalidations += 1; });
  root.dispatchEvent(new Event("scroll"));
  assert.equal(firstInvalidations, 1);
  assert.equal(observed, 2, "frame and realized element are resize-observed");

  resources.transfer(firstOwner, nextOwner);
  let nextInvalidations = 0;
  resources.watch(nextOwner, frame, window, () => { nextInvalidations += 1; });
  assert.equal(disconnected, 1, "rebinding disconnects the old model observer");
  root.dispatchEvent(new Event("scroll"));
  assert.equal(firstInvalidations, 1);
  assert.equal(nextInvalidations, 1);

  resources.release(nextOwner);
  assert.equal(disconnected, 2);
  root.dispatchEvent(new Event("scroll"));
  assert.equal(nextInvalidations, 1, "disposed resources remove captured listeners");
});
