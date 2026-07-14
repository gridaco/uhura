// Play owns a full-screen, app-like route. Keep browser page zoom and history
// navigation from stealing trackpad gestures from the prototype and debugger,
// then restore the shared document contract when routing back to Editor.

export const PLAY_VIEWPORT_CONTENT =
  "width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no";

const BLOCKED_SCALE_KEYS = new Set(["+", "=", "-", "_", "0"]);
const BLOCKED_SCALE_CODES = new Set(["NumpadAdd", "NumpadSubtract", "Numpad0"]);
const SCROLLABLE_OVERFLOW = new Set(["auto", "overlay", "scroll"]);
const SCROLL_EDGE_EPSILON = 0.5;
const OVERSCROLL_X_PROPERTY = "overscroll-behavior-x";

export interface HorizontalScrollState {
  readonly clientWidth: number;
  readonly scrollLeft: number;
  readonly scrollWidth: number;
}

export function isPageScaleShortcut(
  event: Pick<KeyboardEvent, "ctrlKey" | "metaKey" | "key" | "code">,
): boolean {
  if (!event.ctrlKey && !event.metaKey) return false;
  return BLOCKED_SCALE_KEYS.has(event.key) || BLOCKED_SCALE_CODES.has(event.code);
}

export function isHorizontalSwipeWheel(
  event: Pick<WheelEvent, "ctrlKey" | "metaKey" | "deltaX" | "deltaY">,
): boolean {
  if (event.ctrlKey || event.metaKey) return false;
  if (!Number.isFinite(event.deltaX) || !Number.isFinite(event.deltaY)) {
    return false;
  }
  return Math.abs(event.deltaX) > Math.abs(event.deltaY);
}

export function canConsumeHorizontalWheel(
  state: HorizontalScrollState,
  deltaX: number,
): boolean {
  if (!Number.isFinite(deltaX) || deltaX === 0) return false;
  const extent = state.scrollWidth - state.clientWidth;
  if (!Number.isFinite(extent) || extent <= SCROLL_EDGE_EPSILON) return false;
  const position = Math.min(extent, Math.max(0, state.scrollLeft));
  return deltaX < 0
    ? position > SCROLL_EDGE_EPSILON
    : position < extent - SCROLL_EDGE_EPSILON;
}

function horizontalScrollState(value: unknown): HorizontalScrollState | null {
  if (typeof value !== "object" || value === null) return null;
  const candidate = value as Partial<HorizontalScrollState>;
  if (
    typeof candidate.clientWidth !== "number"
    || typeof candidate.scrollLeft !== "number"
    || typeof candidate.scrollWidth !== "number"
  ) {
    return null;
  }
  return candidate as HorizontalScrollState;
}

function pathCanConsumeHorizontalWheel(
  document: Document,
  event: WheelEvent,
): boolean {
  for (const candidate of event.composedPath()) {
    const state = horizontalScrollState(candidate);
    if (state === null) continue;
    const view = document.defaultView;
    if (view !== null) {
      try {
        const overflow = view.getComputedStyle(candidate as Element).overflowX;
        if (!SCROLLABLE_OVERFLOW.has(overflow)) continue;
      } catch {
        continue;
      }
    }
    if (canConsumeHorizontalWheel(state, event.deltaX)) return true;
  }
  return false;
}

/** Installs route-scoped page gesture locks and returns idempotent cleanup. */
export function lockPlayPageScale(document: Document): () => void {
  let viewport = document.querySelector<HTMLMetaElement>('meta[name="viewport"]');
  const created = viewport === null;
  if (viewport === null) {
    viewport = document.createElement("meta");
    viewport.name = "viewport";
    document.head.append(viewport);
  }
  const priorContent = viewport.getAttribute("content");
  viewport.setAttribute("content", PLAY_VIEWPORT_CONTENT);
  const rootStyle = document.documentElement.style;
  const priorOverscrollBehaviorX = rootStyle.getPropertyValue(
    OVERSCROLL_X_PROPERTY,
  );
  const priorOverscrollPriority = rootStyle.getPropertyPriority(
    OVERSCROLL_X_PROPERTY,
  );
  rootStyle.setProperty(OVERSCROLL_X_PROPERTY, "none");

  const preventGesture = (event: Event): void => event.preventDefault();
  const preventWheelScaleOrHistory = (event: WheelEvent): void => {
    if (event.ctrlKey || event.metaKey) {
      event.preventDefault();
      return;
    }
    if (
      isHorizontalSwipeWheel(event)
      && !pathCanConsumeHorizontalWheel(document, event)
    ) {
      event.preventDefault();
    }
  };
  const preventKeyboardScale = (event: KeyboardEvent): void => {
    if (isPageScaleShortcut(event)) event.preventDefault();
  };
  const activeOptions: AddEventListenerOptions = {
    capture: true,
    passive: false,
  };
  document.addEventListener("wheel", preventWheelScaleOrHistory, activeOptions);
  document.addEventListener("keydown", preventKeyboardScale, true);
  document.addEventListener("gesturestart", preventGesture, activeOptions);
  document.addEventListener("gesturechange", preventGesture, activeOptions);
  document.addEventListener("gestureend", preventGesture, activeOptions);

  let disposed = false;
  return (): void => {
    if (disposed) return;
    disposed = true;
    document.removeEventListener(
      "wheel",
      preventWheelScaleOrHistory,
      activeOptions,
    );
    document.removeEventListener("keydown", preventKeyboardScale, true);
    document.removeEventListener("gesturestart", preventGesture, activeOptions);
    document.removeEventListener("gesturechange", preventGesture, activeOptions);
    document.removeEventListener("gestureend", preventGesture, activeOptions);
    if (priorOverscrollBehaviorX === "") {
      rootStyle.removeProperty(OVERSCROLL_X_PROPERTY);
    } else {
      rootStyle.setProperty(
        OVERSCROLL_X_PROPERTY,
        priorOverscrollBehaviorX,
        priorOverscrollPriority,
      );
    }
    if (created) viewport.remove();
    else if (priorContent === null) viewport.removeAttribute("content");
    else viewport.setAttribute("content", priorContent);
  };
}
