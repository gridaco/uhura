import {
  type EditorRender,
  semanticPreviewKey,
  type EditorRevisionEvent,
  type EditorState,
  type PreviewIdentity,
} from "./editor-state.js";

export interface EditorFetchToken {
  readonly sequence: number;
  readonly expectedRevision: number | null;
  readonly reason: "open" | "event" | "retry";
}

export type EditorResponseDecision =
  | { kind: "ignored" }
  | { kind: "behind"; expectedRevision: number; receivedRevision: number }
  | { kind: "prepare"; state: EditorState };

const structurallyEqual = (left: unknown, right: unknown): boolean => {
  if (Object.is(left, right)) return true;
  if (typeof left !== "object" || left === null || typeof right !== "object" || right === null) {
    return false;
  }
  if (Array.isArray(left) || Array.isArray(right)) {
    return Array.isArray(left)
      && Array.isArray(right)
      && left.length === right.length
      && left.every((value, index) => structurallyEqual(value, right[index]));
  }
  const leftRecord = left as Record<string, unknown>;
  const rightRecord = right as Record<string, unknown>;
  const leftKeys = Object.keys(leftRecord);
  const rightKeys = Object.keys(rightRecord);
  return leftKeys.length === rightKeys.length
    && leftKeys.every((key) => Object.hasOwn(rightRecord, key)
      && structurallyEqual(leftRecord[key], rightRecord[key]));
};

/**
 * Returns the previews whose already-realized frame is safe to retain.
 *
 * CSS is deliberately not a frame dependency: retained ShadowRoots can adopt
 * the next model's sheet. Icons and assets are realization-time dependencies,
 * so a changed global table conservatively invalidates every frame.
 */
export const reusablePreviewIds = (
  previous: EditorRender | null,
  next: EditorRender | null,
): ReadonlySet<string> => {
  if (
    !previous
    || !next
    || !structurallyEqual(previous.icons, next.icons)
    || !structurallyEqual(previous.assets, next.assets)
  ) {
    return new Set();
  }
  const previousById = new Map(previous.previews.map((preview) => [preview.id, preview]));
  return new Set(next.previews.flatMap((preview) => {
    const candidate = previousById.get(preview.id);
    return candidate && structurallyEqual(candidate, preview) ? [preview.id] : [];
  }));
};

/** True when a new state changes diagnostics/chrome but not the canvas model. */
export const editorBoardUnchanged = (
  previous: EditorRender | null,
  next: EditorRender | null,
): boolean => {
  if (!previous || !next) return previous === next;
  return previous.stylesheet === next.stylesheet
    && structurallyEqual(previous.groups, next.groups)
    && structurallyEqual(previous.previews, next.previews)
    && structurallyEqual(previous.icons, next.icons)
    && structurallyEqual(previous.assets, next.assets);
};

/**
 * Owns transport ordering without owning DOM. State publication is a two-step
 * operation: decode/consider, prepare the detached board, then commit only if
 * no newer fetch started in the meantime.
 */
export class EditorUpdateSession {
  #sequence = 0;
  #latest: EditorFetchToken | null = null;
  #activeRevision: number | null = null;

  get activeRevision(): number | null {
    return this.#activeRevision;
  }

  isCurrent(token: EditorFetchToken): boolean {
    return token === this.#latest;
  }

  /** Opening (and every reopening) is authoritative across host restarts. */
  opened(): EditorFetchToken {
    return this.#begin("open", null);
  }

  announced(event: EditorRevisionEvent): EditorFetchToken | null {
    if (
      event.sourceRevision === this.#activeRevision
      || event.sourceRevision === this.#latest?.expectedRevision
    ) {
      return null;
    }
    return this.#begin("event", event.sourceRevision);
  }

  retry(
    previous: EditorFetchToken,
    expectedRevision: number | null = previous.expectedRevision,
  ): EditorFetchToken | null {
    return previous === this.#latest
      ? this.#begin("retry", expectedRevision)
      : null;
  }

  consider(token: EditorFetchToken, state: EditorState): EditorResponseDecision {
    if (token !== this.#latest) return { kind: "ignored" };
    if (
      token.expectedRevision !== null
      && state.sourceRevision < token.expectedRevision
    ) {
      return {
        kind: "behind",
        expectedRevision: token.expectedRevision,
        receivedRevision: state.sourceRevision,
      };
    }
    return { kind: "prepare", state };
  }

  commit(
    token: EditorFetchToken,
    state: EditorState,
    install: () => void = () => {},
  ): boolean {
    if (token !== this.#latest) return false;
    install();
    this.#activeRevision = state.sourceRevision;
    this.#latest = null;
    return true;
  }

  #begin(
    reason: EditorFetchToken["reason"],
    expectedRevision: number | null,
  ): EditorFetchToken {
    const token = Object.freeze({
      sequence: ++this.#sequence,
      expectedRevision,
      reason,
    });
    this.#latest = token;
    return token;
  }
}

/** A replacement keeps selection by meaning, never by transient DOM id. */
export const retainPreviewSelection = (
  selection: PreviewIdentity | null,
  state: EditorState,
): PreviewIdentity | null => {
  if (!selection || !state.render) return null;
  const target = semanticPreviewKey(selection);
  return state.render.previews.find((preview) =>
    semanticPreviewKey(preview.identity) === target)?.identity ?? null;
};
