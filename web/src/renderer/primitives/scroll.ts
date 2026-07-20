import {
  semanticAttributes,
  textAttribute,
  UNIT_EVENT,
} from "./common.js";
import type { PrimitiveAdapter, PrimitiveContext } from "./types.js";

interface NearEndObservation {
  readonly observer: IntersectionObserver;
  readonly sentinel: HTMLElement;
  binding: string;
  context: PrimitiveContext;
  armed: boolean;
}

const nearEndObservations = new WeakMap<HTMLElement, NearEndObservation>();

interface StaticPositionFrame {
  readonly view: Window;
  readonly handle: number;
}

const staticPositionFrames = new WeakMap<HTMLElement, StaticPositionFrame>();

const cancelStaticPosition = (element: HTMLElement): void => {
  const pending = staticPositionFrames.get(element);
  if (!pending) return;
  pending.view.cancelAnimationFrame(pending.handle);
  staticPositionFrames.delete(element);
};

const disposeNearEnd = (element: HTMLElement): void => {
  const observation = nearEndObservations.get(element);
  if (!observation) return;
  observation.observer.disconnect();
  observation.sentinel.remove();
  nearEndObservations.delete(element);
};

const syncNearEnd = (
  element: HTMLElement,
  binding: string | undefined,
  context: PrimitiveContext,
): void => {
  const current = nearEndObservations.get(element);
  if (!binding || context.mode !== "play") {
    disposeNearEnd(element);
    return;
  }
  if (current) {
    current.binding = binding;
    current.context = context;
    if (current.sentinel !== element.lastElementChild) {
      element.append(current.sentinel);
    }
    return;
  }

  const Observer = element.ownerDocument.defaultView?.IntersectionObserver;
  if (!Observer) return;
  const sentinel = element.ownerDocument.createElement("div");
  sentinel.dataset["uhMechanic"] = "near-end";
  sentinel.style.cssText = "block-size:1px;flex:none;";
  const observation: NearEndObservation = {
    sentinel,
    binding,
    context,
    armed: true,
    observer: new Observer((entries) => {
      for (const entry of entries) {
        if (entry.isIntersecting && observation.armed) {
          observation.armed = false;
          observation.context.options.dispatch(
            observation.binding,
            observation.context.projectionRevision,
            UNIT_EVENT,
          );
        } else if (!entry.isIntersecting) {
          observation.armed = true;
        }
      }
    }, { root: element, rootMargin: "100%" }),
  };
  nearEndObservations.set(element, observation);
  element.append(sentinel);
  observation.observer.observe(sentinel);
};

const applyStaticPosition = (
  element: HTMLElement,
  positionText: string | undefined,
  direction: string,
  context: PrimitiveContext,
): void => {
  cancelStaticPosition(element);
  if (context.mode !== "editor") return;
  const position = positionText === undefined ? 0 : Number(positionText);
  if (!Number.isFinite(position) || position < 0 || position > 1) return;
  const apply = (): void => {
    if (direction === "horizontal") {
      element.scrollTop = 0;
      element.scrollLeft = Math.round(
        Math.max(0, element.scrollWidth - element.clientWidth) * position,
      );
    } else {
      element.scrollLeft = 0;
      element.scrollTop = Math.round(
        Math.max(0, element.scrollHeight - element.clientHeight) * position,
      );
    }
  };
  apply();

  // Reconciliation runs before the browser has necessarily laid out changed
  // children. Re-apply once at the next frame so a static Editor projection is
  // correct on its first render instead of requiring an incidental rerender.
  const view = element.ownerDocument.defaultView;
  if (
    !view
    || typeof view.requestAnimationFrame !== "function"
    || typeof view.cancelAnimationFrame !== "function"
  ) {
    return;
  }
  const handle = view.requestAnimationFrame(() => {
    const pending = staticPositionFrames.get(element);
    if (pending?.handle !== handle) return;
    staticPositionFrames.delete(element);
    apply();
  });
  staticPositionFrames.set(element, { view, handle });
};

export const scrollAdapter: PrimitiveAdapter = {
  id: "scroll",
  tag: "div",
  managedEvents: ["near-end"],
  attributes(node) {
    return semanticAttributes(node, [{
      name: "data-direction",
      value: textAttribute(node.attributes, "direction") ?? "vertical",
    }]);
  },
  sync(element, node, _hosts, context) {
    const direction =
      textAttribute(node.attributes, "direction") ?? "vertical";
    applyStaticPosition(
      element,
      textAttribute(node.attributes, "position"),
      direction,
      context,
    );
    syncNearEnd(
      element,
      node.events.find((event) => event.event === "near-end")?.binding,
      context,
    );
  },
  dispose(element) {
    cancelStaticPosition(element);
    disposeNearEnd(element);
  },
};
