import {
  type EditorRender,
  type EditorState,
  type PreviewIdentity,
  semanticPreviewKey,
} from "./editor-state.js";

export interface EditorCameraSnapshot {
  readonly x: number;
  readonly y: number;
  readonly scale: number;
}

export interface EditorWorldRect {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

export interface PreviewFocusState {
  readonly identity: PreviewIdentity;
  readonly restoreCamera: EditorCameraSnapshot;
}

type FocusSource = EditorState | EditorRender | null;

const cameraSnapshot = (camera: EditorCameraSnapshot): EditorCameraSnapshot => ({
  x: camera.x,
  y: camera.y,
  scale: camera.scale,
});

const nonNegativeFinite = (value: number, name: string): void => {
  if (!Number.isFinite(value) || value < 0) {
    throw new RangeError(`${name} must be a non-negative finite number`);
  }
};

/** Fits a world-space preview into a viewport under translate-then-scale camera semantics. */
export const fitPreviewCamera = (
  rect: EditorWorldRect,
  viewportWidth: number,
  viewportHeight: number,
  padding: number,
  minScale: number,
  maxScale: number,
): EditorCameraSnapshot => {
  if (!Number.isFinite(rect.x) || !Number.isFinite(rect.y)) {
    throw new RangeError("rect position must contain finite numbers");
  }
  nonNegativeFinite(rect.width, "rect.width");
  nonNegativeFinite(rect.height, "rect.height");
  nonNegativeFinite(viewportWidth, "viewportWidth");
  nonNegativeFinite(viewportHeight, "viewportHeight");
  nonNegativeFinite(padding, "padding");
  if (!Number.isFinite(minScale) || minScale <= 0) {
    throw new RangeError("minScale must be a positive finite number");
  }
  if (!Number.isFinite(maxScale) || maxScale < minScale) {
    throw new RangeError("maxScale must be finite and at least minScale");
  }

  const availableWidth = Math.max(0, viewportWidth - padding * 2);
  const availableHeight = Math.max(0, viewportHeight - padding * 2);
  const canFit = rect.width > 0
    && rect.height > 0
    && availableWidth > 0
    && availableHeight > 0;
  const naturalScale = canFit
    ? Math.min(availableWidth / rect.width, availableHeight / rect.height)
    : minScale;
  const scale = Math.min(Math.max(naturalScale, minScale), maxScale);
  return {
    x: viewportWidth / 2 - (rect.x + rect.width / 2) * scale,
    y: viewportHeight / 2 - (rect.y + rect.height / 2) * scale,
    scale,
  };
};

const sourceRender = (source: FocusSource): EditorRender | null => {
  if (!source) return null;
  return "protocol" in source ? source.render : source;
};

/** Entering an existing focus switches its target without replacing its restore camera. */
export const enterPreviewFocus = (
  current: PreviewFocusState | null,
  identity: PreviewIdentity,
  camera: EditorCameraSnapshot,
): PreviewFocusState => ({
  identity,
  restoreCamera: current?.restoreCamera ?? cameraSnapshot(camera),
});

/** Rebinds focus to the matching preview in a replacement model. */
export const retainPreviewFocus = (
  current: PreviewFocusState | null,
  source: FocusSource,
): PreviewFocusState | null => {
  if (!current) return null;
  const target = semanticPreviewKey(current.identity);
  const preview = sourceRender(source)?.previews.find((candidate) =>
    semanticPreviewKey(candidate.identity) === target);
  return preview
    ? { identity: preview.identity, restoreCamera: current.restoreCamera }
    : null;
};

/** Returns the camera captured on first entry; the caller owns clearing focus. */
export const exitPreviewFocus = (
  current: PreviewFocusState | null,
): EditorCameraSnapshot | null => current ? cameraSnapshot(current.restoreCamera) : null;
