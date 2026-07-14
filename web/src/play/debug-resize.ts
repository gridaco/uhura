// DOM adapter for the two Play debugger splitters. The sizing math is kept
// independent of layout reads so the breakpoints and viewport guarantees stay
// deterministic in tests and in hosts that provide a Window-like viewport.

export type DebugResizeMode = "wide" | "bottom" | "compact";

export interface DebugResizeViewport {
  readonly innerWidth: number;
  readonly innerHeight: number;
  addEventListener(type: "resize", listener: () => void): void;
  removeEventListener(type: "resize", listener: () => void): void;
}

export interface DebugResizeControllerOptions {
  /** Owns the three debugger sizing custom properties. */
  container: HTMLElement;
  panel: HTMLElement;
  panelSeparator: HTMLElement;
  details: HTMLElement;
  detailsSeparator: HTMLElement;
  viewport: DebugResizeViewport;
}

export interface DebugResizeController {
  readonly mode: DebugResizeMode;
  readonly isDisposed: boolean;
  /** Re-evaluates the responsive mode, clamps sizes, and refreshes ARIA. */
  syncLayout(): void;
  /** Idempotently removes listeners and releases any active pointer capture. */
  dispose(): boolean;
}

export interface DebugResizeBounds {
  readonly min: number;
  readonly max: number;
}

export interface DebugResizeGeometry {
  readonly mode: DebugResizeMode;
  readonly panelInline: DebugResizeBounds;
  readonly panelBlock: DebugResizeBounds;
  readonly detailsBlock: DebugResizeBounds;
}

const WIDE_BREAKPOINT = 1_100;
const COMPACT_BREAKPOINT = 620;
const PANEL_INLINE_MIN = 320;
const MAIN_INLINE_MIN = 480;
const PANEL_BLOCK_MIN = 240;
const MAIN_BLOCK_MIN = 240;
const SHORT_VIEWPORT_PANEL_FLOOR = 160;
const DETAILS_BLOCK_MIN = 96;
const DETAILS_MAX_PANEL_RATIO = 0.55;
// Header, picker/footer, and a useful graph region should remain visible above
// the details drawer. Very short viewports degrade to the minimum drawer size.
const DETAILS_NON_DRAWER_RESERVE = 300;
const WIDE_PANEL_TOP_INSET = 52;
const COMPACT_PANEL_VERTICAL_INSET = 114;
const KEYBOARD_STEP = 16;
const LARGE_KEYBOARD_STEP = 48;
const DEFAULT_DETAILS_BLOCK_SIZE = 240;

const PANEL_INLINE_PROPERTY = "--uh-debug-inline-size";
const PANEL_BLOCK_PROPERTY = "--uh-debug-block-size";
const DETAILS_BLOCK_PROPERTY = "--uh-debug-details-size";

function finiteViewport(value: number): number {
  return Number.isFinite(value) ? Math.max(1, value) : 1;
}

function clamp(value: number, bounds: DebugResizeBounds): number {
  return Math.min(bounds.max, Math.max(bounds.min, value));
}

function rounded(value: number): number {
  return Math.round(value);
}

/** Pure breakpoint selection shared by the controller and focused tests. */
export function debugResizeMode(viewportWidth: number): DebugResizeMode {
  const width = finiteViewport(viewportWidth);
  if (width > WIDE_BREAKPOINT) return "wide";
  if (width > COMPACT_BREAKPOINT) return "bottom";
  return "compact";
}

/**
 * Pure resize limits. `panelHeight` may be zero while a hidden panel has no
 * layout box; the viewport fallback mirrors the panel's responsive insets.
 */
export function debugResizeGeometry(
  viewportWidth: number,
  viewportHeight: number,
  panelHeight = 0,
): DebugResizeGeometry {
  const width = finiteViewport(viewportWidth);
  const height = finiteViewport(viewportHeight);
  const mode = debugResizeMode(width);
  const inlineMax = Math.max(PANEL_INLINE_MIN, width - MAIN_INLINE_MIN);
  const blockMax = Math.max(
    SHORT_VIEWPORT_PANEL_FLOOR,
    height - MAIN_BLOCK_MIN,
  );
  const blockMin = Math.min(PANEL_BLOCK_MIN, blockMax);
  const fallbackPanelHeight = Math.max(
    1,
    height - (mode === "compact"
      ? COMPACT_PANEL_VERTICAL_INSET
      : WIDE_PANEL_TOP_INSET),
  );
  const measuredPanelHeight = Number.isFinite(panelHeight) && panelHeight > 0
    ? panelHeight
    : fallbackPanelHeight;
  const detailsMax = Math.max(
    DETAILS_BLOCK_MIN,
    Math.min(
      measuredPanelHeight * DETAILS_MAX_PANEL_RATIO,
      measuredPanelHeight - DETAILS_NON_DRAWER_RESERVE,
    ),
  );

  return Object.freeze({
    mode,
    panelInline: Object.freeze({ min: PANEL_INLINE_MIN, max: inlineMax }),
    panelBlock: Object.freeze({ min: blockMin, max: blockMax }),
    detailsBlock: Object.freeze({ min: DETAILS_BLOCK_MIN, max: detailsMax }),
  });
}

function defaultInlineSize(width: number): number {
  return Math.min(560, Math.max(360, finiteViewport(width) * 0.34));
}

function defaultBlockSize(height: number): number {
  return Math.min(480, Math.max(300, finiteViewport(height) * 0.44));
}

function defaultDetailsSize(bounds: DebugResizeBounds): number {
  return clamp(DEFAULT_DETAILS_BLOCK_SIZE, bounds);
}

type ResizeTarget = "panel" | "details";

interface PointerDrag {
  readonly target: ResizeTarget;
  readonly pointerId: number;
  readonly mode: DebugResizeMode;
  readonly startCoordinate: number;
  readonly startSize: number;
  readonly separator: HTMLElement;
}

interface SeparatorListeners {
  readonly pointerdown: (event: PointerEvent) => void;
  readonly pointermove: (event: PointerEvent) => void;
  readonly pointerup: (event: PointerEvent) => void;
  readonly pointercancel: (event: PointerEvent) => void;
  readonly lostpointercapture: (event: PointerEvent) => void;
  readonly keydown: (event: KeyboardEvent) => void;
  readonly dblclick: (event: MouseEvent) => void;
}

function setAria(
  separator: HTMLElement,
  orientation: "horizontal" | "vertical",
  enabled: boolean,
  bounds: DebugResizeBounds,
  value: number,
): void {
  separator.setAttribute("aria-orientation", orientation);
  separator.setAttribute("aria-disabled", String(!enabled));
  separator.setAttribute("aria-valuemin", String(rounded(bounds.min)));
  separator.setAttribute("aria-valuemax", String(rounded(bounds.max)));
  separator.setAttribute("aria-valuenow", String(rounded(value)));
  separator.setAttribute("tabindex", enabled ? "0" : "-1");
}

function setControls(separator: HTMLElement, controlled: HTMLElement): void {
  if (controlled.id !== "") separator.setAttribute("aria-controls", controlled.id);
}

export function createDebugResizeController(
  options: DebugResizeControllerOptions,
): DebugResizeController {
  const {
    container,
    panel,
    panelSeparator,
    details,
    detailsSeparator,
    viewport,
  } = options;
  let disposed = false;
  let mode = debugResizeMode(viewport.innerWidth);
  let geometry = debugResizeGeometry(
    viewport.innerWidth,
    viewport.innerHeight,
    panel.getBoundingClientRect().height,
  );
  let panelInlineSize = defaultInlineSize(viewport.innerWidth);
  let panelBlockSize = defaultBlockSize(viewport.innerHeight);
  let detailsBlockSize = defaultDetailsSize(geometry.detailsBlock);
  let drag: PointerDrag | null = null;

  setControls(panelSeparator, panel);
  setControls(detailsSeparator, details);

  function writeSize(property: string, size: number): void {
    container.style.setProperty(property, `${rounded(size)}px`);
  }

  function panelResizeEnabled(): boolean {
    return mode !== "compact";
  }

  function detailsResizeEnabled(): boolean {
    return mode !== "bottom";
  }

  function refreshPanelAria(): void {
    if (mode === "wide") {
      setAria(
        panelSeparator,
        "vertical",
        true,
        geometry.panelInline,
        panelInlineSize,
      );
      return;
    }
    if (mode === "bottom") {
      setAria(
        panelSeparator,
        "horizontal",
        true,
        geometry.panelBlock,
        panelBlockSize,
      );
      return;
    }
    setAria(
      panelSeparator,
      "horizontal",
      false,
      Object.freeze({ min: 0, max: 0 }),
      0,
    );
  }

  function refreshDetailsAria(): void {
    if (mode === "bottom") {
      setAria(
        detailsSeparator,
        "vertical",
        false,
        Object.freeze({ min: 0, max: 0 }),
        0,
      );
      return;
    }
    setAria(
      detailsSeparator,
      "horizontal",
      true,
      geometry.detailsBlock,
      detailsBlockSize,
    );
  }

  function refreshAria(): void {
    refreshPanelAria();
    refreshDetailsAria();
  }

  function releaseDrag(releaseCapture: boolean): void {
    const active = drag;
    drag = null;
    if (active === null) return;
    active.separator.removeAttribute("data-resizing");
    container.removeAttribute("data-debug-resizing");
    container.removeAttribute("data-debug-resize-axis");
    if (!releaseCapture) return;
    try {
      if (active.separator.hasPointerCapture(active.pointerId)) {
        active.separator.releasePointerCapture(active.pointerId);
      }
    } catch {
      // Pointer capture can already be gone after a canceled OS gesture.
    }
  }

  function commitPanelSize(size: number): void {
    if (mode === "wide") {
      panelInlineSize = clamp(size, geometry.panelInline);
      writeSize(PANEL_INLINE_PROPERTY, panelInlineSize);
    } else if (mode === "bottom") {
      panelBlockSize = clamp(size, geometry.panelBlock);
      writeSize(PANEL_BLOCK_PROPERTY, panelBlockSize);
    }
    refreshPanelAria();
  }

  function commitDetailsSize(size: number): void {
    detailsBlockSize = clamp(size, geometry.detailsBlock);
    writeSize(DETAILS_BLOCK_PROPERTY, detailsBlockSize);
    refreshDetailsAria();
  }

  function coordinate(target: ResizeTarget, event: PointerEvent): number {
    return target === "panel" && mode === "wide"
      ? event.clientX
      : event.clientY;
  }

  function currentSize(target: ResizeTarget): number {
    if (target === "details") return detailsBlockSize;
    return mode === "wide" ? panelInlineSize : panelBlockSize;
  }

  function enabled(target: ResizeTarget): boolean {
    return target === "panel"
      ? panelResizeEnabled()
      : detailsResizeEnabled();
  }

  function startDrag(
    target: ResizeTarget,
    separator: HTMLElement,
    event: PointerEvent,
  ): void {
    if (disposed || !enabled(target) || event.button !== 0) return;
    releaseDrag(true);
    drag = {
      target,
      pointerId: event.pointerId,
      mode,
      startCoordinate: coordinate(target, event),
      startSize: currentSize(target),
      separator,
    };
    separator.setAttribute("data-resizing", "true");
    container.setAttribute("data-debug-resizing", "true");
    container.setAttribute(
      "data-debug-resize-axis",
      target === "panel" && mode === "wide" ? "inline" : "block",
    );
    try {
      separator.setPointerCapture(event.pointerId);
    } catch {
      // Continue without capture for small DOM shims and retired pointers.
    }
    event.preventDefault();
  }

  function moveDrag(event: PointerEvent): void {
    const active = drag;
    if (
      disposed
      || active === null
      || active.pointerId !== event.pointerId
      || active.mode !== mode
    ) {
      return;
    }
    // Both splitters sit on the leading edge of a panel/drawer anchored to the
    // viewport's trailing/bottom edge, so moving up/left increases its size.
    const delta = active.startCoordinate - coordinate(active.target, event);
    if (active.target === "panel") commitPanelSize(active.startSize + delta);
    else commitDetailsSize(active.startSize + delta);
    event.preventDefault();
  }

  function endDrag(event: PointerEvent): void {
    if (drag?.pointerId !== event.pointerId) return;
    releaseDrag(true);
    event.preventDefault();
  }

  function loseCapture(event: PointerEvent): void {
    if (drag?.pointerId === event.pointerId) releaseDrag(false);
  }

  function keyboardValue(
    target: ResizeTarget,
    event: KeyboardEvent,
  ): number | null {
    const bounds = target === "details"
      ? geometry.detailsBlock
      : mode === "wide"
      ? geometry.panelInline
      : geometry.panelBlock;
    const value = currentSize(target);
    const step = event.shiftKey ? LARGE_KEYBOARD_STEP : KEYBOARD_STEP;
    if (event.key === "Home") return bounds.min;
    if (event.key === "End") return bounds.max;

    if (target === "panel" && mode === "wide") {
      if (event.key === "ArrowLeft") return value + step;
      if (event.key === "ArrowRight") return value - step;
      return null;
    }
    if (event.key === "ArrowUp") return value + step;
    if (event.key === "ArrowDown") return value - step;
    return null;
  }

  function resizeFromKeyboard(
    target: ResizeTarget,
    event: KeyboardEvent,
  ): void {
    if (disposed || !enabled(target)) return;
    const value = keyboardValue(target, event);
    if (value === null) return;
    if (target === "panel") commitPanelSize(value);
    else commitDetailsSize(value);
    event.preventDefault();
  }

  function reset(target: ResizeTarget, event: MouseEvent): void {
    if (disposed || !enabled(target)) return;
    if (target === "details") {
      commitDetailsSize(defaultDetailsSize(geometry.detailsBlock));
    } else if (mode === "wide") {
      commitPanelSize(defaultInlineSize(viewport.innerWidth));
    } else {
      commitPanelSize(defaultBlockSize(viewport.innerHeight));
    }
    event.preventDefault();
  }

  function separatorListeners(
    target: ResizeTarget,
    separator: HTMLElement,
  ): SeparatorListeners {
    return {
      pointerdown: (event) => startDrag(target, separator, event),
      pointermove: moveDrag,
      pointerup: endDrag,
      pointercancel: endDrag,
      lostpointercapture: loseCapture,
      keydown: (event) => resizeFromKeyboard(target, event),
      dblclick: (event) => reset(target, event),
    };
  }

  const panelListeners = separatorListeners("panel", panelSeparator);
  const detailsListeners = separatorListeners("details", detailsSeparator);

  function addListeners(
    separator: HTMLElement,
    listeners: SeparatorListeners,
  ): void {
    separator.addEventListener("pointerdown", listeners.pointerdown);
    separator.addEventListener("pointermove", listeners.pointermove);
    separator.addEventListener("pointerup", listeners.pointerup);
    separator.addEventListener("pointercancel", listeners.pointercancel);
    separator.addEventListener(
      "lostpointercapture",
      listeners.lostpointercapture,
    );
    separator.addEventListener("keydown", listeners.keydown);
    separator.addEventListener("dblclick", listeners.dblclick);
  }

  function removeListeners(
    separator: HTMLElement,
    listeners: SeparatorListeners,
  ): void {
    separator.removeEventListener("pointerdown", listeners.pointerdown);
    separator.removeEventListener("pointermove", listeners.pointermove);
    separator.removeEventListener("pointerup", listeners.pointerup);
    separator.removeEventListener("pointercancel", listeners.pointercancel);
    separator.removeEventListener(
      "lostpointercapture",
      listeners.lostpointercapture,
    );
    separator.removeEventListener("keydown", listeners.keydown);
    separator.removeEventListener("dblclick", listeners.dblclick);
  }

  function syncLayout(): void {
    if (disposed) return;
    const nextMode = debugResizeMode(viewport.innerWidth);
    if (nextMode !== mode) releaseDrag(true);
    mode = nextMode;
    geometry = debugResizeGeometry(
      viewport.innerWidth,
      viewport.innerHeight,
      panel.getBoundingClientRect().height,
    );
    if (mode === "wide") {
      panelInlineSize = clamp(panelInlineSize, geometry.panelInline);
    } else if (mode === "bottom") {
      panelBlockSize = clamp(panelBlockSize, geometry.panelBlock);
    }
    // The bottom dock renders details as a side column, so its block-size
    // preference is dormant there. Preserve it for a later wide/compact mode.
    if (mode !== "bottom") {
      detailsBlockSize = clamp(detailsBlockSize, geometry.detailsBlock);
    }
    writeSize(PANEL_INLINE_PROPERTY, panelInlineSize);
    writeSize(PANEL_BLOCK_PROPERTY, panelBlockSize);
    writeSize(DETAILS_BLOCK_PROPERTY, detailsBlockSize);
    refreshAria();
  }

  const onViewportResize = (): void => syncLayout();
  addListeners(panelSeparator, panelListeners);
  addListeners(detailsSeparator, detailsListeners);
  viewport.addEventListener("resize", onViewportResize);
  syncLayout();

  const controller: DebugResizeController = {
    get mode() {
      return mode;
    },
    get isDisposed() {
      return disposed;
    },
    syncLayout,
    dispose(): boolean {
      if (disposed) return false;
      disposed = true;
      releaseDrag(true);
      viewport.removeEventListener("resize", onViewportResize);
      removeListeners(panelSeparator, panelListeners);
      removeListeners(detailsSeparator, detailsListeners);
      return true;
    },
  };
  return Object.freeze(controller);
}
