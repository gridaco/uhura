// Dependency-free zoom and pan behavior for Play's debugger graph. The
// controller owns only scroll geometry and transforms: graph rendering stays
// with debug-surface, while the stable content wrapper supplies scroll extent.

export const DEBUG_VIEWPORT_MIN_ZOOM = 0.5;
export const DEBUG_VIEWPORT_MAX_ZOOM = 2;
export const DEBUG_VIEWPORT_ZOOM_STEP = 0.1;

const WHEEL_ZOOM_SENSITIVITY = 0.002;
const WHEEL_LINE_PIXELS = 16;

export interface DebugViewportPoint {
  readonly x: number;
  readonly y: number;
}

export interface DebugViewportSize {
  readonly width: number;
  readonly height: number;
}

export type DebugViewportWindow = Pick<
  Window,
  "addEventListener" | "removeEventListener"
>;

export interface DebugViewportOptions {
  /** The native two-axis scrollport. */
  viewport: HTMLElement;
  /** Stable child whose dimensions represent the transformed graph extent. */
  content: HTMLElement;
  zoomIn: HTMLButtonElement;
  zoomOut: HTMLButtonElement;
  zoomReset: HTMLButtonElement;
  /** Visible percentage, normally nested inside `zoomReset`. */
  zoomOutput: HTMLElement;
  /** Optional future control; `fit()` is available without it. */
  zoomFit?: HTMLButtonElement;
  /** Injectable only for lifecycle wiring; defaults to the document view. */
  window?: DebugViewportWindow;
}

export interface DebugViewportController {
  readonly zoom: number;
  readonly hasLayout: boolean;
  readonly isDisposed: boolean;
  /** Sets bounded zoom, preserving the supplied scrollport-local anchor. */
  setZoom(zoom: number, anchor?: DebugViewportPoint): boolean;
  /** Restores exactly 100%, centered on the current viewport. */
  reset(): boolean;
  /** Fits both axes into the current viewport, within the global bounds. */
  fit(): boolean;
  /**
   * Attaches the latest rendered canvas and logical dimensions. The canvas
   * must already be a child of `content`; its transform does not own extent.
   */
  setLayout(canvas: HTMLElement, width: number, height: number): void;
  /** Clears canvas geometry, scroll extent, position, and zoom state. */
  clearLayout(): void;
  /** Idempotently removes listeners and releases active pointer capture. */
  dispose(): boolean;
}

interface DebugViewportLayout extends DebugViewportSize {
  readonly canvas: HTMLElement;
}

interface PanGesture {
  readonly pointerId: number;
  lastX: number;
  lastY: number;
}

function finitePositive(name: string, value: number): number {
  if (!Number.isFinite(value) || value <= 0) {
    throw new RangeError(`${name} must be a positive finite number`);
  }
  return value;
}

function finiteCoordinate(name: string, value: number): number {
  if (!Number.isFinite(value)) {
    throw new RangeError(`${name} must be a finite number`);
  }
  return value;
}

/** Pure zoom normalization shared by direct, wheel, reset, and fit actions. */
export function clampDebugViewportZoom(zoom: number): number {
  finiteCoordinate("zoom", zoom);
  return Math.min(
    DEBUG_VIEWPORT_MAX_ZOOM,
    Math.max(DEBUG_VIEWPORT_MIN_ZOOM, zoom),
  );
}

/** Pure fit calculation. Fit may enlarge a small graph, up to the 200% cap. */
export function fitDebugViewportZoom(
  viewport: DebugViewportSize,
  layout: DebugViewportSize,
): number {
  const viewportWidth = finitePositive("viewport width", viewport.width);
  const viewportHeight = finitePositive("viewport height", viewport.height);
  const layoutWidth = finitePositive("layout width", layout.width);
  const layoutHeight = finitePositive("layout height", layout.height);
  return clampDebugViewportZoom(
    Math.min(viewportWidth / layoutWidth, viewportHeight / layoutHeight),
  );
}

/**
 * Pure anchor equation. The logical point under `anchor` remains under it
 * after the scale changes; callers may clamp the result to native extents.
 */
export function anchoredDebugViewportScroll(
  scroll: DebugViewportPoint,
  anchor: DebugViewportPoint,
  previousZoom: number,
  nextZoom: number,
): DebugViewportPoint {
  finiteCoordinate("scroll x", scroll.x);
  finiteCoordinate("scroll y", scroll.y);
  finiteCoordinate("anchor x", anchor.x);
  finiteCoordinate("anchor y", anchor.y);
  finitePositive("previous zoom", previousZoom);
  finitePositive("next zoom", nextZoom);
  return Object.freeze({
    x: ((scroll.x + anchor.x) / previousZoom) * nextZoom - anchor.x,
    y: ((scroll.y + anchor.y) / previousZoom) * nextZoom - anchor.y,
  });
}

/** Smooth, delta-mode-aware modified-wheel zoom. */
export function wheelDebugViewportZoom(
  zoom: number,
  deltaY: number,
  deltaMode: number,
  pagePixels: number,
): number {
  const current = clampDebugViewportZoom(zoom);
  finiteCoordinate("wheel delta", deltaY);
  const page = finitePositive("wheel page size", pagePixels);
  const pixels = deltaY * (deltaMode === 1
    ? WHEEL_LINE_PIXELS
    : deltaMode === 2
    ? page
    : 1);
  return clampDebugViewportZoom(
    current * Math.exp(-pixels * WHEEL_ZOOM_SENSITIVITY),
  );
}

function cssNumber(value: number): string {
  const rounded = Math.round(value * 10_000) / 10_000;
  return String(Object.is(rounded, -0) ? 0 : rounded);
}

function buttonZoom(zoom: number, direction: -1 | 1): number {
  const stepped = zoom + direction * DEBUG_VIEWPORT_ZOOM_STEP;
  return clampDebugViewportZoom(Math.round(stepped * 10_000) / 10_000);
}

function closestInteractive(target: EventTarget | null): boolean {
  const closest = (target as { closest?: (selector: string) => unknown } | null)
    ?.closest;
  if (typeof closest !== "function") return false;
  return closest.call(
    target,
    "button, a, input, select, textarea, summary, [role='button'], "
      + "[contenteditable='true'], [data-uh-debug-node]",
  ) !== null;
}

export function createDebugViewport(
  options: DebugViewportOptions,
): DebugViewportController {
  const {
    viewport,
    content,
    zoomIn,
    zoomOut,
    zoomReset,
    zoomOutput,
    zoomFit,
  } = options;
  const view = options.window ?? viewport.ownerDocument.defaultView;
  if (view === null) {
    throw new Error("debug viewport requires a Window-like document view");
  }

  let disposed = false;
  let zoom = 1;
  let layout: DebugViewportLayout | null = null;
  let pan: PanGesture | null = null;

  function viewportCenter(): DebugViewportPoint {
    return {
      x: viewport.clientWidth / 2,
      y: viewport.clientHeight / 2,
    };
  }

  function maxScroll(): DebugViewportPoint {
    if (layout === null) return { x: 0, y: 0 };
    return {
      x: Math.max(0, layout.width * zoom - viewport.clientWidth),
      y: Math.max(0, layout.height * zoom - viewport.clientHeight),
    };
  }

  function writeScroll(point: DebugViewportPoint): void {
    const maximum = maxScroll();
    viewport.scrollLeft = Math.min(maximum.x, Math.max(0, point.x));
    viewport.scrollTop = Math.min(maximum.y, Math.max(0, point.y));
  }

  function writeGeometry(): void {
    if (layout === null) {
      content.style.inlineSize = "";
      content.style.blockSize = "";
      return;
    }
    content.style.inlineSize = `${cssNumber(layout.width * zoom)}px`;
    content.style.blockSize = `${cssNumber(layout.height * zoom)}px`;
    layout.canvas.style.transformOrigin = "0 0";
    layout.canvas.style.transform = `scale(${cssNumber(zoom)})`;
  }

  function writeControls(): void {
    const percent = Math.round(zoom * 100);
    zoomOutput.textContent = `${String(percent)}%`;
    zoomOutput.setAttribute("aria-label", `Graph zoom ${String(percent)}%`);
    zoomOutput.setAttribute("aria-live", "polite");
    zoomOutput.setAttribute("aria-atomic", "true");
    const active = layout !== null && !disposed;
    zoomOut.disabled = !active || zoom <= DEBUG_VIEWPORT_MIN_ZOOM;
    zoomIn.disabled = !active || zoom >= DEBUG_VIEWPORT_MAX_ZOOM;
    zoomReset.disabled = !active || zoom === 1;
    if (zoomFit) zoomFit.disabled = !active;
  }

  function applyZoom(next: number, anchor = viewportCenter()): boolean {
    if (disposed || layout === null) return false;
    const normalized = clampDebugViewportZoom(next);
    if (normalized === zoom) return false;
    const nextScroll = anchoredDebugViewportScroll(
      { x: viewport.scrollLeft, y: viewport.scrollTop },
      anchor,
      zoom,
      normalized,
    );
    zoom = normalized;
    writeGeometry();
    writeScroll(nextScroll);
    writeControls();
    return true;
  }

  function releasePan(releaseCapture: boolean): void {
    const active = pan;
    pan = null;
    viewport.removeAttribute("data-panning");
    if (active === null || !releaseCapture) return;
    try {
      if (viewport.hasPointerCapture(active.pointerId)) {
        viewport.releasePointerCapture(active.pointerId);
      }
    } catch {
      // Capture can already be gone after an OS gesture or detached viewport.
    }
  }

  function localPointer(event: Pick<PointerEvent, "clientX" | "clientY">) {
    const bounds = viewport.getBoundingClientRect();
    return {
      x: event.clientX - bounds.left - viewport.clientLeft,
      y: event.clientY - bounds.top - viewport.clientTop,
    };
  }

  function onWheel(event: WheelEvent): void {
    if (!event.ctrlKey && !event.metaKey) return;
    // Modified wheel belongs to graph zoom even before a graph is available;
    // never let it bubble into browser page zoom while over the debugger.
    event.preventDefault();
    if (disposed || layout === null || event.deltaY === 0) return;
    applyZoom(
      wheelDebugViewportZoom(
        zoom,
        event.deltaY,
        event.deltaMode,
        Math.max(1, viewport.clientHeight),
      ),
      localPointer(event),
    );
  }

  function onPointerDown(event: PointerEvent): void {
    if (
      disposed
      || layout === null
      || event.button !== 0
      || !event.isPrimary
      || event.pointerType === "touch"
      || closestInteractive(event.target)
    ) {
      return;
    }
    const local = localPointer(event);
    // Do not intercept native scrollbar presses, whose target can be viewport.
    if (
      local.x < 0
      || local.y < 0
      || local.x > viewport.clientWidth
      || local.y > viewport.clientHeight
    ) {
      return;
    }
    releasePan(true);
    pan = {
      pointerId: event.pointerId,
      lastX: event.clientX,
      lastY: event.clientY,
    };
    viewport.dataset["panning"] = "true";
    try {
      viewport.setPointerCapture(event.pointerId);
    } catch {
      // Pointer capture is an enhancement; local movement still pans.
    }
    event.preventDefault();
  }

  function onPointerMove(event: PointerEvent): void {
    const active = pan;
    if (disposed || active === null || active.pointerId !== event.pointerId) {
      return;
    }
    const deltaX = event.clientX - active.lastX;
    const deltaY = event.clientY - active.lastY;
    active.lastX = event.clientX;
    active.lastY = event.clientY;
    writeScroll({
      x: viewport.scrollLeft - deltaX,
      y: viewport.scrollTop - deltaY,
    });
    event.preventDefault();
  }

  function onPointerEnd(event: PointerEvent): void {
    if (pan?.pointerId !== event.pointerId) return;
    releasePan(true);
    event.preventDefault();
  }

  function onLostPointerCapture(event: PointerEvent): void {
    if (pan?.pointerId === event.pointerId) releasePan(false);
  }

  const onBlur = (): void => releasePan(true);
  const onZoomIn = (): void => {
    applyZoom(buttonZoom(zoom, 1));
  };
  const onZoomOut = (): void => {
    applyZoom(buttonZoom(zoom, -1));
  };
  const onZoomReset = (): void => {
    applyZoom(1);
  };
  const onZoomFit = (): void => {
    if (layout === null) return;
    applyZoom(fitDebugViewportZoom(
      {
        width: Math.max(1, viewport.clientWidth),
        height: Math.max(1, viewport.clientHeight),
      },
      layout,
    ));
  };

  viewport.addEventListener("wheel", onWheel, { passive: false });
  viewport.addEventListener("pointerdown", onPointerDown);
  viewport.addEventListener("pointermove", onPointerMove);
  viewport.addEventListener("pointerup", onPointerEnd);
  viewport.addEventListener("pointercancel", onPointerEnd);
  viewport.addEventListener("lostpointercapture", onLostPointerCapture);
  zoomIn.addEventListener("click", onZoomIn);
  zoomOut.addEventListener("click", onZoomOut);
  zoomReset.addEventListener("click", onZoomReset);
  zoomFit?.addEventListener("click", onZoomFit);
  view.addEventListener("blur", onBlur);
  writeControls();

  const controller: DebugViewportController = {
    get zoom() {
      return zoom;
    },
    get hasLayout() {
      return layout !== null;
    },
    get isDisposed() {
      return disposed;
    },
    setZoom(next, anchor): boolean {
      finiteCoordinate("zoom", next);
      if (anchor) {
        finiteCoordinate("anchor x", anchor.x);
        finiteCoordinate("anchor y", anchor.y);
      }
      return applyZoom(next, anchor);
    },
    reset(): boolean {
      return applyZoom(1);
    },
    fit(): boolean {
      if (disposed || layout === null) return false;
      return applyZoom(fitDebugViewportZoom(
        {
          width: Math.max(1, viewport.clientWidth),
          height: Math.max(1, viewport.clientHeight),
        },
        layout,
      ));
    },
    setLayout(canvas, width, height): void {
      if (disposed) return;
      const nextWidth = finitePositive("layout width", width);
      const nextHeight = finitePositive("layout height", height);
      const center = viewportCenter();
      const logicalCenter = layout === null
        ? null
        : {
            x: (viewport.scrollLeft + center.x) / zoom,
            y: (viewport.scrollTop + center.y) / zoom,
          };
      if (layout !== null && layout.canvas !== canvas) {
        layout.canvas.style.transform = "";
        layout.canvas.style.transformOrigin = "";
      }
      layout = { canvas, width: nextWidth, height: nextHeight };
      writeGeometry();
      if (logicalCenter === null) {
        writeScroll({ x: viewport.scrollLeft, y: viewport.scrollTop });
      } else {
        writeScroll({
          x: logicalCenter.x * zoom - center.x,
          y: logicalCenter.y * zoom - center.y,
        });
      }
      writeControls();
    },
    clearLayout(): void {
      if (disposed) return;
      releasePan(true);
      if (layout !== null) {
        layout.canvas.style.transform = "";
        layout.canvas.style.transformOrigin = "";
      }
      layout = null;
      zoom = 1;
      writeGeometry();
      viewport.scrollLeft = 0;
      viewport.scrollTop = 0;
      writeControls();
    },
    dispose(): boolean {
      if (disposed) return false;
      releasePan(true);
      if (layout !== null) {
        layout.canvas.style.transform = "";
        layout.canvas.style.transformOrigin = "";
      }
      layout = null;
      zoom = 1;
      writeGeometry();
      viewport.scrollLeft = 0;
      viewport.scrollTop = 0;
      disposed = true;
      writeControls();
      viewport.removeEventListener("wheel", onWheel);
      viewport.removeEventListener("pointerdown", onPointerDown);
      viewport.removeEventListener("pointermove", onPointerMove);
      viewport.removeEventListener("pointerup", onPointerEnd);
      viewport.removeEventListener("pointercancel", onPointerEnd);
      viewport.removeEventListener(
        "lostpointercapture",
        onLostPointerCapture,
      );
      zoomIn.removeEventListener("click", onZoomIn);
      zoomOut.removeEventListener("click", onZoomOut);
      zoomReset.removeEventListener("click", onZoomReset);
      zoomFit?.removeEventListener("click", onZoomFit);
      view.removeEventListener("blur", onBlur);
      return true;
    },
  };
  return Object.freeze(controller);
}
