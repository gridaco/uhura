export const EDITOR_HOST_META_NAME = "uhura-editor-host";
export const EDITOR_BUILD_META_NAME = "uhura-editor-build";
export const EDITOR_EVENTS_PATH = "/editor/events";
export const EDITOR_CHECKPOINT_KEY = "uhura.editor.live-checkpoint.v2";

export type EditorTool = "cursor" | "hand";

export interface SemanticPreviewKey {
  kind: string;
  subject: string;
  example: string;
}

export interface EditorShellState {
  camera: {
    x: number;
    y: number;
    scale: number;
  };
  tool: EditorTool;
  search: string;
  uiVisible: boolean;
  selection: SemanticPreviewKey | null;
}

export interface EditorLiveEvent {
  candidateGeneration: number;
  activeGeneration: number | null;
  activeBuildId: string | null;
  status: "active" | "rejected";
  diagnostics?: Record<string, unknown>;
}

interface StoredCheckpoint extends EditorShellState {
  version: 2;
  targetBuildId: string;
}

export interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

export interface DiagnosticSummary {
  code: string;
  rule: string;
  severity: string;
  message: string;
  location: string;
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

const isGeneration = (value: unknown): value is number =>
  typeof value === "number" && Number.isSafeInteger(value) && value >= 0;

const isActiveGeneration = (value: unknown): value is number =>
  isGeneration(value) && value > 0;

const isBuildId = (value: unknown): value is string =>
  typeof value === "string" && value.length > 0;

const isFiniteNumber = (value: unknown): value is number =>
  typeof value === "number" && Number.isFinite(value);

const optionalString = (value: unknown, fallback = ""): string =>
  typeof value === "string" ? value : fallback;

/**
 * Parses the hosted document's active generation. `0` is the cold-invalid
 * sentinel: the Editor is live, but there is no last-known-good Canvas yet.
 */
export const parseHostedGeneration = (content: string | null): number | null | undefined => {
  if (content === null || content.trim() === "") return undefined;
  const generation = Number(content);
  if (!Number.isSafeInteger(generation) || generation < 0) return undefined;
  return generation === 0 ? null : generation;
};

/**
 * Parses the content-derived identity of the hosted Canvas. An empty value is
 * the cold-invalid sentinel; a missing meta element is represented by the
 * caller as `undefined` so standalone exports remain entirely offline.
 */
export const parseHostedBuildId = (content: string | null): string | null | undefined => {
  if (content === null) return undefined;
  const buildId = content.trim();
  return buildId === "" ? null : buildId;
};

/** Runtime-checks the SSE boundary instead of trusting a TypeScript cast. */
export const parseEditorLiveEvent = (value: unknown): EditorLiveEvent | null => {
  if (!isRecord(value)) return null;
  const candidateGeneration = value["candidateGeneration"];
  const activeGeneration = value["activeGeneration"];
  const activeBuildId = value["activeBuildId"];
  const status = value["status"];
  const diagnostics = value["diagnostics"];
  if (
    !isGeneration(candidateGeneration)
    || !(activeGeneration === null || isActiveGeneration(activeGeneration))
    || !(activeBuildId === null || isBuildId(activeBuildId))
    || ((activeGeneration === null) !== (activeBuildId === null))
    || (status !== "active" && status !== "rejected")
    || (diagnostics !== undefined && !isRecord(diagnostics))
    || (status === "active" && activeGeneration !== candidateGeneration)
    || (
      status === "rejected"
      && activeGeneration !== null
      && activeGeneration >= candidateGeneration
    )
  ) {
    return null;
  }
  return {
    candidateGeneration,
    activeGeneration,
    activeBuildId,
    status,
    ...(diagnostics === undefined ? {} : { diagnostics }),
  };
};

/** Numeric generations are process-local labels; only the content identity
 * can decide whether a hosted document has converged after a restart. */
export const shouldReloadEditor = (
  documentBuildId: string | null,
  event: EditorLiveEvent,
): boolean => event.activeBuildId !== documentBuildId;

export type EditorLiveDecision =
  | { kind: "ignored" }
  | { kind: "reload"; event: EditorLiveEvent }
  | { kind: "rejected"; event: EditorLiveEvent }
  | { kind: "active"; event: EditorLiveEvent };

/** Process-local event ordering and exactly-once reload state. Transport and
 * DOM effects stay in the Editor controller; this state machine is pure. */
export class EditorLiveSession {
  readonly #documentBuildId: string | null;
  #highestCandidate = -1;
  #reloading = false;

  constructor(documentBuildId: string | null) {
    this.#documentBuildId = documentBuildId;
  }

  get reloading(): boolean {
    return this.#reloading;
  }

  reconnect(): void {
    this.#highestCandidate = -1;
  }

  accept(event: EditorLiveEvent): EditorLiveDecision {
    if (this.#reloading || event.candidateGeneration < this.#highestCandidate) {
      return { kind: "ignored" };
    }
    this.#highestCandidate = event.candidateGeneration;
    if (shouldReloadEditor(this.#documentBuildId, event)) {
      this.#reloading = true;
      return { kind: "reload", event };
    }
    return event.status === "rejected"
      ? { kind: "rejected", event }
      : { kind: "active", event };
  }
}

const parseSemanticKey = (value: unknown): SemanticPreviewKey | null | undefined => {
  if (value === null) return null;
  if (!isRecord(value)) return undefined;
  const kind = value["kind"];
  const subject = value["subject"];
  const example = value["example"];
  if (
    typeof kind !== "string"
    || typeof subject !== "string"
    || typeof example !== "string"
    || !kind
    || !subject
    || !example
  ) {
    return undefined;
  }
  return { kind, subject, example };
};

const parseCheckpoint = (value: unknown): StoredCheckpoint | null => {
  if (!isRecord(value) || value["version"] !== 2) return null;
  const targetBuildId = value["targetBuildId"];
  const camera = value["camera"];
  const tool = value["tool"];
  const search = value["search"];
  const uiVisible = value["uiVisible"];
  const selection = parseSemanticKey(value["selection"]);
  if (
    !isBuildId(targetBuildId)
    || !isRecord(camera)
    || !isFiniteNumber(camera["x"])
    || !isFiniteNumber(camera["y"])
    || !isFiniteNumber(camera["scale"])
    || camera["scale"] <= 0
    || (tool !== "cursor" && tool !== "hand")
    || typeof search !== "string"
    || typeof uiVisible !== "boolean"
    || selection === undefined
  ) {
    return null;
  }
  return {
    version: 2,
    targetBuildId,
    camera: { x: camera["x"], y: camera["y"], scale: camera["scale"] },
    tool,
    search,
    uiVisible,
    selection,
  };
};

/**
 * Reads and consumes a checkpoint for one reload handoff. Whichever active
 * build the navigation serves may consume it: another candidate or a rapid
 * source reversion can win after the reload has already started.
 */
export const takeEditorCheckpoint = (
  storage: StorageLike,
  activeBuildId: string | null,
): EditorShellState | null => {
  let raw: string | null;
  try {
    raw = storage.getItem(EDITOR_CHECKPOINT_KEY);
    storage.removeItem(EDITOR_CHECKPOINT_KEY);
  } catch {
    return null;
  }
  if (raw === null || activeBuildId === null) return null;
  try {
    const checkpoint = parseCheckpoint(JSON.parse(raw));
    if (!checkpoint) return null;
    return {
      camera: checkpoint.camera,
      tool: checkpoint.tool,
      search: checkpoint.search,
      uiVisible: checkpoint.uiVisible,
      selection: checkpoint.selection,
    };
  } catch {
    return null;
  }
};

export const storeEditorCheckpoint = (
  storage: StorageLike,
  targetBuildId: string,
  state: EditorShellState,
): boolean => {
  const checkpoint: StoredCheckpoint = {
    version: 2,
    targetBuildId,
    ...state,
  };
  try {
    storage.setItem(EDITOR_CHECKPOINT_KEY, JSON.stringify(checkpoint));
    return true;
  } catch {
    return false;
  }
};

const diagnosticLocation = (diagnostic: Record<string, unknown>): string => {
  const file = optionalString(diagnostic["file"]);
  const span = diagnostic["span"];
  if (!isRecord(span)) return file;
  const start = span["start"];
  if (!isRecord(start)) return file;
  const line = start["line"];
  const col = start["col"];
  if (typeof line !== "number") return file;
  return `${file || "source"}:${line}${typeof col === "number" ? `:${col}` : ""}`;
};

export const summarizeDiagnostics = (envelope: Record<string, unknown> | undefined): DiagnosticSummary[] => {
  const diagnostics = envelope?.["diagnostics"];
  if (!Array.isArray(diagnostics)) return [];
  return diagnostics.filter(isRecord).map((diagnostic) => ({
    code: optionalString(diagnostic["code"], "UH????"),
    rule: optionalString(diagnostic["rule"]),
    severity: optionalString(diagnostic["severity"], "error"),
    message: optionalString(diagnostic["message"], "The Canvas candidate was rejected."),
    location: diagnosticLocation(diagnostic),
  }));
};
