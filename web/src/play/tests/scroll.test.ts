import assert from "node:assert/strict";
import { afterEach, beforeEach, test, vi } from "vitest";

import type { Descriptor } from "../../protocol/types.js";
import type { ScrollHolder } from "../../renderer/contracts.js";
import {
  createScrolls,
  SCROLL_POSITION_CACHE_LIMIT,
} from "../scroll.js";

class FakeElement {
  readonly attributes = new Map<string, string>();
  readonly style = { cssText: "" };
  children: FakeElement[] = [];
  parentElement: FakeElement | null = null;
  className = "";
  scrollHeight = 0;
  scrollLeft = 0;
  scrollTop = 0;

  get lastElementChild(): FakeElement | null {
    return this.children.at(-1) ?? null;
  }

  setAttribute(name: string, value: string): void {
    this.attributes.set(name, value);
  }

  getAttribute(name: string): string | null {
    return this.attributes.get(name) ?? null;
  }

  append(...nodes: FakeElement[]): void {
    for (const node of nodes) {
      node.remove();
      node.parentElement = this;
      this.children.push(node);
    }
  }

  remove(): void {
    if (!this.parentElement) return;
    const index = this.parentElement.children.indexOf(this);
    if (index >= 0) this.parentElement.children.splice(index, 1);
    this.parentElement = null;
  }

  contains(candidate: FakeElement): boolean {
    return (
      this === candidate || this.children.some((child) => child.contains(candidate))
    );
  }

  querySelectorAll(selector: string): FakeElement[] {
    const descendants = this.children.flatMap((child) => [
      child,
      ...child.querySelectorAll("*"),
    ]);
    if (selector === "*") return descendants;
    if (selector.startsWith(".")) {
      const className = selector.slice(1);
      return descendants.filter((candidate) =>
        candidate.className.split(/\s+/).includes(className),
      );
    }
    return [];
  }
}

class FakeIntersectionObserver {
  static instances: FakeIntersectionObserver[] = [];

  readonly callback: IntersectionObserverCallback;
  disconnectCalls = 0;

  constructor(callback: IntersectionObserverCallback) {
    this.callback = callback;
    FakeIntersectionObserver.instances.push(this);
  }

  observe(): void {}

  unobserve(): void {}

  disconnect(): void {
    this.disconnectCalls += 1;
  }

  deliver(isIntersecting: boolean): void {
    this.callback(
      [{ isIntersecting } as IntersectionObserverEntry],
      this as unknown as IntersectionObserver,
    );
  }
}

const asElement = (element: FakeElement): HTMLElement =>
  element as unknown as HTMLElement;

beforeEach(() => {
  FakeIntersectionObserver.instances = [];
  vi.stubGlobal("HTMLElement", FakeElement);
  vi.stubGlobal("document", {
    createElement: () => new FakeElement(),
  });
  vi.stubGlobal("IntersectionObserver", FakeIntersectionObserver);
});

afterEach(() => {
  vi.unstubAllGlobals();
});

const nearEnd: Descriptor = {
  kind: "observe",
  event: "near-end",
  emit: "load-more",
  scope: "page:1",
  payload: {},
};

test("disposing a renderer subtree disconnects nested near-end observation", () => {
  const emitted: Descriptor[] = [];
  const scrolls = createScrolls({ emit: (descriptor) => emitted.push(descriptor) });
  const outer = new FakeElement();
  const scroll = new FakeElement();
  outer.append(scroll);
  const holder: ScrollHolder = {
    path: "page:1/root/feed",
    on: { "near-end": nearEnd },
  };

  scrolls.sync(asElement(scroll), holder);
  const observer = FakeIntersectionObserver.instances[0];
  assert.ok(observer);
  const sentinel = scroll.lastElementChild;
  assert.ok(sentinel);

  scrolls.disposeSubtree(asElement(outer));

  assert.equal(observer.disconnectCalls, 1);
  assert.equal(holder.nearEnd, undefined);
  assert.equal(sentinel.parentElement, null);
  observer.deliver(true);
  assert.deepEqual(emitted, [], "a queued stale delivery is ignored");

  scrolls.disposeSubtree(asElement(outer));
  assert.equal(observer.disconnectCalls, 1, "cleanup is idempotent");
});

test("the navigation scroll-position cache evicts its least-recent route", () => {
  const scrolls = createScrolls({ emit: () => {} });

  for (let index = 0; index <= SCROLL_POSITION_CACHE_LIMIT; index += 1) {
    const page = new FakeElement();
    const scroll = new FakeElement();
    scroll.className = "uh-scroll";
    scroll.setAttribute("data-key", "feed");
    scroll.scrollTop = index + 1;
    page.append(scroll);
    scrolls.savePositions(`route-${index}`, asElement(page));
  }

  const evictedPage = new FakeElement();
  const evictedScroll = new FakeElement();
  evictedScroll.className = "uh-scroll";
  evictedScroll.setAttribute("data-key", "feed");
  evictedScroll.scrollTop = 777;
  evictedPage.append(evictedScroll);
  scrolls.restorePositions("route-0", asElement(evictedPage));
  assert.equal(evictedScroll.scrollTop, 777);

  const retainedPage = new FakeElement();
  const retainedScroll = new FakeElement();
  retainedScroll.className = "uh-scroll";
  retainedScroll.setAttribute("data-key", "feed");
  retainedPage.append(retainedScroll);
  scrolls.restorePositions(
    `route-${SCROLL_POSITION_CACHE_LIMIT}`,
    asElement(retainedPage),
  );
  assert.equal(retainedScroll.scrollTop, SCROLL_POSITION_CACHE_LIMIT + 1);
});
