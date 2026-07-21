import {
  directMechanic,
  physicalAttribute,
  semanticAttributes,
  textAttribute,
  UNIT_EVENT,
} from "./common.js";
import type {
  ElementNode,
  PrimitiveAdapter,
  PrimitiveContext,
} from "./types.js";

interface PagerState {
  readonly track: HTMLElement;
  readonly listener: EventListener;
  page: number;
  pageCount: number;
  binding: string | undefined;
  context: PrimitiveContext | undefined;
}

const pagerStates = new WeakMap<HTMLElement, PagerState>();

const activePage = (track: HTMLElement, pageCount: number): number => {
  if (pageCount < 1) return 0;
  const width = track.clientWidth || 1;
  return Math.min(
    pageCount - 1,
    Math.max(0, Math.round(track.scrollLeft / width)),
  );
};

const updateDots = (
  pager: HTMLElement,
  page: number,
): void => {
  const dots = directMechanic(pager, "dots");
  if (!dots) return;
  Array.from(dots.children).forEach((dot, index) => {
    dot.classList.toggle("on", index === page);
  });
};

const onPagerScroll = (pager: HTMLElement): void => {
  const state = pagerStates.get(pager);
  if (!state) return;
  const nextPage = activePage(state.track, state.pageCount);
  updateDots(pager, nextPage);
  if (nextPage === state.page) return;
  state.page = nextPage;
  if (state.binding && state.context?.mode === "play") {
    state.context.options.dispatch(
      state.binding,
      state.context.projectionRevision,
      UNIT_EVENT,
    );
  }
};

const ensureTrack = (pager: HTMLElement): HTMLElement => {
  let track = directMechanic(pager, "track");
  if (!track) {
    track = pager.ownerDocument.createElement("div");
    track.className = "uh-track";
    track.dataset["uhMechanic"] = "track";
    pager.append(track);
  }
  if (!pagerStates.has(pager)) {
    const listener: EventListener = () => onPagerScroll(pager);
    track.addEventListener("scroll", listener, { passive: true });
    pagerStates.set(pager, {
      track,
      listener,
      page: 0,
      pageCount: 0,
      binding: undefined,
      context: undefined,
    });
  }
  return track;
};

const syncDots = (
  pager: HTMLElement,
  node: ElementNode,
): void => {
  if (textAttribute(node.attributes, "indicator") !== "dots") {
    directMechanic(pager, "dots")?.remove();
    return;
  }
  let dots = directMechanic(pager, "dots");
  if (!dots) {
    dots = pager.ownerDocument.createElement("div");
    dots.className = "uh-dots";
    dots.dataset["uhMechanic"] = "dots";
    dots.setAttribute("aria-hidden", "true");
    pager.append(dots);
  }
  while (dots.children.length > node.children.length) {
    dots.lastElementChild?.remove();
  }
  while (dots.children.length < node.children.length) {
    const dot = pager.ownerDocument.createElement("span");
    dot.className = "uh-dot";
    dots.append(dot);
  }
};

export const pagerAdapter: PrimitiveAdapter = {
  id: "pager",
  tag: "div",
  managedEvents: ["page-change"],
  attributes(node) {
    return semanticAttributes(node, [
      { name: "role", value: "group" },
      physicalAttribute(
        "aria-label",
        textAttribute(node.attributes, "label"),
      ),
    ]);
  },
  hosts(element) {
    return { children: ensureTrack(element), events: element };
  },
  sync(element, node, _hosts, context) {
    syncDots(element, node);
    const state = pagerStates.get(element);
    if (!state) throw new Error("Uhura pager lost its track state");
    state.pageCount = node.children.length;
    state.binding = node.events.find(
      (event) => event.event === "page-change",
    )?.binding;
    state.context = context;
    const page = activePage(state.track, state.pageCount);
    state.page = page;
    updateDots(element, page);
  },
  dispose(element) {
    const state = pagerStates.get(element);
    if (!state) return;
    state.track.removeEventListener("scroll", state.listener);
    pagerStates.delete(element);
  },
};
