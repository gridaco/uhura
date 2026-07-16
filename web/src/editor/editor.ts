import {
  decodeEditorRevisionEvent,
  decodeEditorState,
  type EditorPreview,
  type EditorRender,
  type EditorState,
  type JsonValue,
  type PreviewDataField,
  type PreviewIdentity,
  type ReplayStep,
} from "./editor-state.js";
import {
  disposePreparedEditorModel,
  prepareEditorModel,
  previewIdForIdentity,
  reconcilePreparedEditorModel,
  renderStructureConnectorLabel,
  setAnnotationConnectorsHidden,
  watchPreparedEditorModel,
  type PreparedEditorModel,
  type PreparedStructureConnector,
  type PreparedWorkflowConnector,
} from "./editor-board.js";
import {
  AnnotationOverlay,
  renderPreviewDocumentation,
  renderSourcePanel,
  validateAnnotationRealizations,
} from "./annotation-overlay.js";
import { EDITOR_STYLES } from "./editor-styles.js";
import {
  surfaceHierarchy,
  type SurfaceHierarchyNode,
} from "./surface-hierarchy.js";
import {
  routeWorkflowConnector,
  workflowRailHeight,
} from "./workflow-connectors.js";
import {
  enterPreviewFocus,
  exitPreviewFocus,
  fitPreviewCamera,
  retainPreviewFocus,
  type PreviewFocusState,
} from "./editor-focus.js";
import {
  incomingLeftLabelShift,
  layoutMapStructureConnectors,
  layoutStructureConnectors,
  routeStructureConnector,
  splitGlobalNavConnectors,
  structureDefinitionNode,
  visibleStructureConnectors,
  type PlacedStructureConnector,
} from "./structure-connectors.js";
import {
  layoutInteractionMap,
  MAP_NODE_SCALE,
  mapNodePreviewIds,
  scaledMapNodeSize,
  type MapNodeSize,
} from "./map-layout.js";
import {
  applyMapOverrides,
  draggedMapPosition,
  isDragGesture,
  retainMapOverrides,
  setMapOverride,
  type MapPoint,
} from "./map-interaction.js";
import type { InteractionGraphNode } from "../protocol/types.js";
import {
  roundedStructurePath,
  shouldReplayStructureDraw,
  structureDrawDelayMs,
} from "./structure-presentation.js";
import { sourceShortcutAction } from "./editor-shortcuts.js";
import {
  EditorUpdateSession,
  retainPreviewSelection,
  type EditorFetchToken,
} from "./editor-updates.js";

const EDITOR_STATE_PATH = "/api/editor/state";
const EDITOR_EVENTS_PATH = "/api/editor/events";
const UI_VISIBLE_KEY = "uhura.editor.ui-visible";
const MIN_SCALE = 0.02;
const MAX_SCALE = 3;
const FOCUS_MAX_SCALE = 1.5;
const FOCUS_PADDING = 64;
const ZOOM_STEP = 1.2;
const WHEEL_ZOOM_SENSITIVITY = 0.01;

type Tool = "cursor" | "hand";

type ActiveStructureConnector = PlacedStructureConnector<PreparedStructureConnector>;

/** Font size of structure labels in marker units (constant on-screen size). */
const STRUCTURE_LABEL_FONT = 11;
const STRUCTURE_MARKER_SCALE_MAX = 8;

/** Viewport padding when the camera fits the whole map, screen pixels. */
const MAP_FIT_PADDING = 72;
/** Map node footprints when measurement is unavailable, board units. */
const MAP_PAGE_FALLBACK: MapNodeSize = { width: 392, height: 920 };
const MAP_SURFACE_FALLBACK: MapNodeSize = { width: 392, height: 636 };
const MAP_PLACEHOLDER_FALLBACK: MapNodeSize = { width: 300, height: 180 };

interface Point {
  x: number;
  y: number;
}

interface Rect extends Point {
  width: number;
  height: number;
}

interface PanState {
  pointerId: number;
  pointerX: number;
  pointerY: number;
  x: number;
  y: number;
  moved: boolean;
}

interface PinchState {
  distance: number;
  scale: number;
  worldX: number;
  worldY: number;
}

interface PreparedCanvas {
  context: CanvasRenderingContext2D;
  width: number;
  height: number;
}

interface EditorShell {
  shell: HTMLElement;
  navigatorApplication: HTMLElement;
  navigatorCount: HTMLElement;
  navigatorSearch: HTMLInputElement;
  navigatorResults: HTMLElement;
  navigatorEmpty: HTMLElement;
  viewport: HTMLElement;
  board: HTMLElement;
  annotationOverlay: HTMLElement;
  rulerX: HTMLCanvasElement;
  rulerY: HTMLCanvasElement;
  tools: HTMLElement;
  cursorButton: HTMLButtonElement;
  handButton: HTMLButtonElement;
  zoomOutButton: HTMLButtonElement;
  zoomOutput: HTMLButtonElement;
  zoomInButton: HTMLButtonElement;
  focusSelectionButton: HTMLButtonElement;
  mapToggleButton: HTMLButtonElement;
  navToggleButton: HTMLButtonElement;
  mapResetButton: HTMLButtonElement;
  sourceDrawerButton: HTMLButtonElement;
  focusHeader: HTMLElement;
  exitFocusButton: HTMLButtonElement;
  focusBreadcrumbKind: HTMLElement;
  focusBreadcrumbSubject: HTMLElement;
  focusBreadcrumbExample: HTMLElement;
  inspectorOverview: HTMLElement;
  inspectorSelection: HTMLElement;
  overviewApplication: HTMLElement;
  overviewFreshness: HTMLElement;
  overviewStats: HTMLElement;
  overviewCallout: HTMLElement;
  clearSelectionButton: HTMLButtonElement;
  selectionKind: HTMLElement;
  selectionName: HTMLElement;
  selectionSubject: HTMLElement;
  selectionExample: HTMLElement;
  selectionSize: HTMLElement;
  selectionOrigin: HTMLElement;
  selectionSourceRow: HTMLElement;
  selectionSource: HTMLButtonElement;
  selectionFromRow: HTMLElement;
  selectionFrom: HTMLElement;
  selectionReplayRow: HTMLElement;
  selectionReplay: HTMLElement;
  selectionHierarchyBlock: HTMLElement;
  selectionHierarchy: HTMLUListElement;
  selectionWorkflowBlock: HTMLElement;
  selectionWorkflow: HTMLOListElement;
  selectionStatus: HTMLElement;
  selectionData: HTMLElement;
  selectionNoData: HTMLElement;
  selectionNoteBlock: HTMLElement;
  selectionNote: HTMLElement;
  selectionInteractions: HTMLUListElement;
  selectionNoInteractions: HTMLElement;
  selectionDocumentationBlock: HTMLElement;
  selectionDocumentation: HTMLElement;
  selectionAnnouncement: HTMLElement;
  sourceDrawer: HTMLElement;
  sourceDrawerClose: HTMLButtonElement;
  sourcePanel: HTMLElement;
  status: HTMLElement;
  statusTitle: HTMLElement;
  statusDetail: HTMLElement;
  statusDiagnostics: HTMLOListElement;
  statusDismiss: HTMLButtonElement;
}

export type EditorDispose = () => void;

const SHELL_HTML = `
  <nav class="editor-navigator" aria-label="Preview navigator">
    <div class="panel-heading"><strong data-navigator-application>Uhura</strong><span data-navigator-count>0 groups</span></div>
    <div class="navigator-results"></div>
    <p class="navigator-empty" hidden>No matching previews</p>
    <label class="navigator-search">
      <svg aria-hidden="true" viewBox="0 0 16 16"><circle cx="7" cy="7" r="4.25"></circle><path d="m10.25 10.25 3 3"></path></svg>
      <input type="search" placeholder="Search previews" autocomplete="off" aria-label="Search previews">
    </label>
  </nav>
  <main class="editor-stage">
    <header class="focus-header" hidden>
      <button class="focus-exit" type="button" aria-label="Back to all previews" aria-keyshortcuts="Escape" title="Back to all previews (Esc)"><svg aria-hidden="true" viewBox="0 0 16 16"><path d="m9.5 3.5-4.5 4.5 4.5 4.5M5 8h8"></path></svg>All previews</button>
      <nav class="focus-breadcrumb" aria-label="Focused preview">
        <span class="focus-breadcrumb-kind"></span><span class="focus-breadcrumb-separator" aria-hidden="true">/</span><strong class="focus-breadcrumb-subject"></strong><span class="focus-breadcrumb-separator" aria-hidden="true">/</span><span class="focus-breadcrumb-example" aria-current="page"></span>
      </nav>
    </header>
    <div class="ruler-corner" aria-hidden="true"></div>
    <canvas class="canvas-ruler ruler-x" aria-hidden="true"></canvas>
    <canvas class="canvas-ruler ruler-y" aria-hidden="true"></canvas>
    <div class="editor-viewport" role="region" aria-label="Canvas viewport" tabindex="0">
      <div class="canvas-tools" role="group" aria-label="Canvas tools">
        <button class="canvas-tool tool-cursor" type="button" aria-label="Cursor tool" aria-keyshortcuts="V" aria-pressed="true" title="Cursor (V)">
          <svg aria-hidden="true" viewBox="0 0 16 16"><path d="M3 2.25v11.5l3.05-3.05 2.15 3.72 2.08-1.2-2.13-3.69 4.15-1.12L3 2.25Z"></path></svg>
        </button>
        <button class="canvas-tool tool-hand" type="button" aria-label="Hand tool" aria-keyshortcuts="H" aria-pressed="false" title="Hand (H or hold Space)">
          <svg aria-hidden="true" viewBox="0 0 16 16"><path d="M5.15 7.4V3.75a1 1 0 0 1 2 0V6.5h.35V2.75a1 1 0 0 1 2 0V6.5h.35V3.75a1 1 0 0 1 2 0V7h.35V5.25a1 1 0 0 1 2 0v3.8c0 3.05-1.8 5.2-4.9 5.2H8.2c-1.5 0-2.45-.65-3.35-1.8L2.3 9.2a1.13 1.13 0 0 1 1.75-1.42l1.1 1.2V7.4Z"></path></svg>
        </button>
        <span class="tool-divider" aria-hidden="true"></span>
        <button class="canvas-tool stroke zoom-out" type="button" aria-label="Zoom out" title="Zoom out"><svg aria-hidden="true" viewBox="0 0 16 16"><path d="M3.5 8h9"></path></svg></button>
        <button class="canvas-zoom" type="button" aria-label="Reset zoom to 100%" title="Reset zoom to 100%">100%</button>
        <button class="canvas-tool stroke zoom-in" type="button" aria-label="Zoom in" title="Zoom in"><svg aria-hidden="true" viewBox="0 0 16 16"><path d="M3.5 8h9M8 3.5v9"></path></svg></button>
        <span class="tool-divider" aria-hidden="true"></span>
        <button class="canvas-tool stroke focus-selection" type="button" aria-label="Center selected preview" title="Center selected preview" disabled><svg aria-hidden="true" viewBox="0 0 16 16"><path d="M5.5 2.5h-3v3M10.5 2.5h3v3M13.5 10.5v3h-3M5.5 13.5h-3v-3"></path></svg></button>
        <button class="canvas-tool stroke map-toggle" type="button" aria-label="Toggle map view" aria-pressed="false" title="Map view: one node per page, arranged by app flow" disabled><svg aria-hidden="true" viewBox="0 0 16 16"><path d="M1.75 3.9 5.9 2.4l4.2 1.5 4.15-1.5v9.7L10.1 13.6l-4.2-1.5-4.15 1.5zM5.9 2.4v9.7M10.1 3.9v9.7"></path></svg><span class="map-toggle-label">Map</span></button>
        <button class="canvas-tool stroke nav-toggle" type="button" aria-label="Show tab-bar navigation edges" aria-pressed="false" title="Show tab-bar navigation edges" hidden><svg aria-hidden="true" viewBox="0 0 16 16"><rect x="2" y="10.5" width="12" height="3" rx="1"></rect><path d="M5 8.5v2M8 6.5v4M11 8.5v2"></path></svg><span class="nav-toggle-label">Nav</span></button>
        <button class="canvas-tool stroke map-reset" type="button" aria-label="Reset map layout" title="Reset dragged nodes to the derived layout" hidden><svg aria-hidden="true" viewBox="0 0 16 16"><path d="M12.9 9.2a5 5 0 1 1-.5-3.9M12.7 2.6v2.9H9.8"></path></svg><span class="map-reset-label">Reset layout</span></button>
        <button class="canvas-tool stroke source-drawer-toggle" type="button" aria-label="Open Source documentation" aria-controls="editor-source-drawer" aria-expanded="false" aria-keyshortcuts="Y" title="Source documentation (Y)"><svg aria-hidden="true" viewBox="0 0 16 16"><path d="M3 2.5h7.5L13 5v8.5H3zM10.5 2.5V5H13M5.5 8h5M5.5 10.5h5"></path></svg></button>
      </div>
      <div class="editor-board"><section class="empty-board"><h2>Starting Editor</h2><p>Loading static previews…</p></section></div>
      <div class="annotation-overlay" aria-label="Canvas annotations"></div>
    </div>
  </main>
  <aside class="editor-inspector" aria-label="Preview details">
    <div class="panel-heading"><a class="play-link" href="/play" aria-label="Open Play"><svg aria-hidden="true" viewBox="0 0 16 16"><path d="M5 3.25v9.5L12.5 8z"></path></svg>Play</a></div>
    <section class="inspector-section inspector-overview">
      <div class="inspector-hero"><span class="inspector-hero-icon" aria-hidden="true">U</span><div><strong data-overview-application>Uhura</strong><span data-overview-freshness>Loading preview model</span></div></div>
      <dl class="inspector-grid" data-overview-stats></dl>
      <div class="inspector-callout" data-overview-callout><strong>Read-only projection</strong><p>Save a <code>.uhura</code> file to rebuild these previews automatically.</p></div>
    </section>
    <section class="inspector-section inspector-selection" hidden>
      <div class="selection-heading"><div><span class="selection-kind">Page</span><h2 class="selection-name">Preview</h2></div><button class="icon-button clear-selection" type="button" aria-label="Clear preview selection" title="Clear selection"><svg aria-hidden="true" viewBox="0 0 16 16"><path d="m4 4 8 8m0-8-8 8"></path></svg></button></div>
      <dl class="property-list">
        <div><dt>Subject</dt><dd class="selection-subject"></dd></div>
        <div><dt>Example</dt><dd class="selection-example"></dd></div>
        <div><dt>Size</dt><dd class="selection-size"></dd></div>
        <div><dt>Origin</dt><dd class="selection-origin"></dd></div>
        <div class="selection-source-row" hidden><dt>Source</dt><dd><button class="selection-source source-location" type="button"></button></dd></div>
        <div class="selection-from-row" hidden><dt>From</dt><dd class="selection-from"></dd></div>
        <div class="selection-replay-row" hidden><dt>Replay</dt><dd class="selection-replay"></dd></div>
        <div><dt>Status</dt><dd class="selection-status"></dd></div>
      </dl>
      <section class="inspector-block selection-hierarchy-block" aria-labelledby="selection-hierarchy-title" hidden>
        <h3 id="selection-hierarchy-title">Surface hierarchy</h3>
        <p class="inspector-block-intro">Mounted children in back-to-front stack order.</p>
        <ul class="surface-hierarchy selection-hierarchy"></ul>
      </section>
      <section class="inspector-block selection-workflow-block" aria-labelledby="selection-workflow-title" hidden>
        <h3 id="selection-workflow-title">Workflow trace</h3>
        <p class="inspector-block-intro">Checked event payload, handler selection, guards, and committed effects.</p>
        <ol class="workflow-step-list selection-workflow"></ol>
      </section>
      <section class="inspector-block" aria-labelledby="selection-data-title">
        <h3 id="selection-data-title">Example data</h3>
        <p class="inspector-block-intro">Computed values and where they come from.</p>
        <div class="selection-data"></div>
        <p class="inspector-muted selection-no-data">No example data is set directly for this preview.</p>
      </section>
      <div class="inspector-block selection-note-block" hidden><h3>Note</h3><p class="selection-note"></p></div>
      <section class="inspector-block selection-documentation" hidden><h3>Documentation & annotations</h3><div class="selection-documentation-content"></div></section>
      <div class="inspector-block"><h3>Declared interactions</h3><ul class="interaction-list selection-interactions"></ul><p class="inspector-muted selection-no-interactions">No interactions declared in this snapshot.</p></div>
      <p class="visually-hidden selection-announcement" aria-live="polite"></p>
    </section>
  </aside>
  <aside class="editor-source-drawer" id="editor-source-drawer" aria-label="Source documentation" hidden>
    <div class="source-drawer-heading"><div><strong>Source</strong><span>Documentation and annotations</span></div><button class="icon-button source-drawer-close" type="button" aria-label="Close Source documentation"><svg aria-hidden="true" viewBox="0 0 16 16"><path d="m4 4 8 8m0-8-8 8"></path></svg></button></div>
    <div class="source-panel"></div>
  </aside>
  <section class="editor-status" role="status" aria-live="polite" data-tone="neutral">
    <div class="status-heading"><div class="status-copy"><strong>Starting Editor</strong><p>Loading the current project state…</p></div><button class="status-dismiss" type="button" aria-label="Dismiss Editor status" title="Dismiss">×</button></div>
    <ol class="diagnostic-list"></ol>
  </section>
`;

const required = <T extends Element>(root: ParentNode, selector: string): T => {
  const node = root.querySelector(selector);
  if (!node) throw new Error(`missing Editor shell element ${selector}`);
  return node as T;
};

const buildShell = (root: HTMLElement): EditorShell => {
  const document = root.ownerDocument;
  const style = document.createElement("style");
  style.dataset.uhuraEditorStyles = "";
  style.textContent = EDITOR_STYLES;
  const shell = document.createElement("div");
  shell.className = "uhura-editor";
  shell.innerHTML = SHELL_HTML;
  root.replaceChildren(style, shell);
  return {
    shell,
    navigatorApplication: required(shell, "[data-navigator-application]"),
    navigatorCount: required(shell, "[data-navigator-count]"),
    navigatorSearch: required(shell, ".navigator-search input"),
    navigatorResults: required(shell, ".navigator-results"),
    navigatorEmpty: required(shell, ".navigator-empty"),
    viewport: required(shell, ".editor-viewport"),
    board: required(shell, ".editor-board"),
    annotationOverlay: required(shell, ".annotation-overlay"),
    rulerX: required(shell, ".ruler-x"),
    rulerY: required(shell, ".ruler-y"),
    tools: required(shell, ".canvas-tools"),
    cursorButton: required(shell, ".tool-cursor"),
    handButton: required(shell, ".tool-hand"),
    zoomOutButton: required(shell, ".zoom-out"),
    zoomOutput: required(shell, ".canvas-zoom"),
    zoomInButton: required(shell, ".zoom-in"),
    focusSelectionButton: required(shell, ".focus-selection"),
    mapToggleButton: required(shell, ".map-toggle"),
    navToggleButton: required(shell, ".nav-toggle"),
    mapResetButton: required(shell, ".map-reset"),
    sourceDrawerButton: required(shell, ".source-drawer-toggle"),
    focusHeader: required(shell, ".focus-header"),
    exitFocusButton: required(shell, ".focus-exit"),
    focusBreadcrumbKind: required(shell, ".focus-breadcrumb-kind"),
    focusBreadcrumbSubject: required(shell, ".focus-breadcrumb-subject"),
    focusBreadcrumbExample: required(shell, ".focus-breadcrumb-example"),
    inspectorOverview: required(shell, ".inspector-overview"),
    inspectorSelection: required(shell, ".inspector-selection"),
    overviewApplication: required(shell, "[data-overview-application]"),
    overviewFreshness: required(shell, "[data-overview-freshness]"),
    overviewStats: required(shell, "[data-overview-stats]"),
    overviewCallout: required(shell, "[data-overview-callout]"),
    clearSelectionButton: required(shell, ".clear-selection"),
    selectionKind: required(shell, ".selection-kind"),
    selectionName: required(shell, ".selection-name"),
    selectionSubject: required(shell, ".selection-subject"),
    selectionExample: required(shell, ".selection-example"),
    selectionSize: required(shell, ".selection-size"),
    selectionOrigin: required(shell, ".selection-origin"),
    selectionSourceRow: required(shell, ".selection-source-row"),
    selectionSource: required(shell, ".selection-source"),
    selectionFromRow: required(shell, ".selection-from-row"),
    selectionFrom: required(shell, ".selection-from"),
    selectionReplayRow: required(shell, ".selection-replay-row"),
    selectionReplay: required(shell, ".selection-replay"),
    selectionHierarchyBlock: required(shell, ".selection-hierarchy-block"),
    selectionHierarchy: required(shell, ".selection-hierarchy"),
    selectionWorkflowBlock: required(shell, ".selection-workflow-block"),
    selectionWorkflow: required(shell, ".selection-workflow"),
    selectionStatus: required(shell, ".selection-status"),
    selectionData: required(shell, ".selection-data"),
    selectionNoData: required(shell, ".selection-no-data"),
    selectionNoteBlock: required(shell, ".selection-note-block"),
    selectionNote: required(shell, ".selection-note"),
    selectionInteractions: required(shell, ".selection-interactions"),
    selectionNoInteractions: required(shell, ".selection-no-interactions"),
    selectionDocumentationBlock: required(shell, ".selection-documentation"),
    selectionDocumentation: required(shell, ".selection-documentation-content"),
    selectionAnnouncement: required(shell, ".selection-announcement"),
    sourceDrawer: required(shell, ".editor-source-drawer"),
    sourceDrawerClose: required(shell, ".source-drawer-close"),
    sourcePanel: required(shell, ".source-panel"),
    status: required(shell, ".editor-status"),
    statusTitle: required(shell, ".status-copy strong"),
    statusDetail: required(shell, ".status-copy p"),
    statusDiagnostics: required(shell, ".diagnostic-list"),
    statusDismiss: required(shell, ".status-dismiss"),
  };
};

const closest = <T extends Element>(
  target: EventTarget | null,
  selector: string,
): T | null => target instanceof Element ? target.closest<T>(selector) : null;

const storedUiVisible = (storage: Storage | undefined): boolean => {
  if (!storage) return true;
  try {
    return storage.getItem(UI_VISIBLE_KEY) !== "false";
  } catch {
    return true;
  }
};

const storeUiVisible = (storage: Storage | undefined, visible: boolean): void => {
  try {
    storage?.setItem(UI_VISIBLE_KEY, String(visible));
  } catch {
    // UI persistence is a convenience, never a requirement for Editor.
  }
};

const diagnostics = (state: EditorState): string[] => {
  const list = state.diagnostics?.["diagnostics"];
  if (!Array.isArray(list)) return [];
  return list.flatMap((item) => {
    if (typeof item !== "object" || item === null || Array.isArray(item)) return [];
    const value = item as Record<string, JsonValue>;
    const message = value["message"];
    if (typeof message !== "string") return [];
    const code = typeof value["code"] === "string" ? value["code"] : "";
    const file = typeof value["file"] === "string" ? value["file"] : "";
    return [`${code ? `${code}: ` : ""}${message}${file ? ` — ${file}` : ""}`];
  });
};

const formatValue = (value: JsonValue | undefined): string => {
  if (value === undefined || value === null) return "Not set";
  if (typeof value === "boolean") return value ? "Yes" : "No";
  if (typeof value === "string" || typeof value === "number") return String(value);
  return JSON.stringify(value, null, 2);
};

const dataGroupTitle = (group: PreviewDataField["group"]): string => ({
  "page-address": "Page address",
  properties: "Properties",
  "provided-data": "Provided data",
})[group];

const sourceDescription = (field: PreviewDataField): string | null => {
  const source = field.source;
  if (!source) return null;
  const declared = source.declaredIn ? ` · declared in ${source.declaredIn}` : "";
  const timeline = source.timeline ? " · timeline" : "";
  if (source.kind === "inline") return `Inline${declared}${timeline}`;
  const path = source.path.length > 0 ? ` → ${source.path.join(" → ")}` : "";
  const prefix = source.kind === "automatic-fixture" ? "Automatic fixture" : "Fixture";
  return `${prefix} ${source.fixture}${path}${declared}${timeline}`;
};

const shellSize = (preview: EditorPreview): string => {
  if (preview.identity.kind === "page") return "390 × 844";
  if (preview.identity.kind === "surface") return "390 × 560";
  return "390 × content";
};

const origin = (preview: EditorPreview): string => {
  if (preview.pinned) return "Pinned example";
  if (preview.derived) return "Replay-derived";
  return "Checked example";
};

const stat = (document: Document, label: string, value: number): HTMLElement => {
  const group = document.createElement("div");
  const term = document.createElement("dt");
  term.textContent = label;
  const description = document.createElement("dd");
  description.textContent = String(value);
  group.append(term, description);
  return group;
};

export const mountEditor = (root: HTMLElement): EditorDispose => {
  const document = root.ownerDocument;
  const window = document.defaultView;
  if (!window) throw new Error("Uhura Editor requires a browser window");
  const shell = buildShell(root);
  const updates = new EditorUpdateSession();
  let model = prepareEditorModel(document, null);
  let state: EditorState | null = null;
  let selectedIdentity: PreviewIdentity | null = null;
  let selectedTool: Tool = "cursor";
  let spaceHeld = false;
  let pan: PanState | null = null;
  let pinch: PinchState | null = null;
  let suppressClickUntil = 0;
  let x = 0;
  let y = 0;
  let scale = 1;
  let rulerFrame = 0;
  let focusFitFrame = 0;
  let focusFitGeneration = 0;
  let focusState: PreviewFocusState | null = null;
  let focusFrameObserver: ResizeObserver | null = null;
  let connectorFrame = 0;
  // The selection-scoped structural subset, fanned around the clicked frame
  // only. Empty whenever nothing is selected (Figma behavior).
  let activeStructureConnectors: ActiveStructureConnector[] = [];
  // Draw-in replays only when the SELECTED PREVIEW changes: relayouts of the
  // same selection keep the arrows steady, deselect+reselect replays.
  let lastStructureDrawPreviewId: string | null = null;
  let pendingStructureDraw = false;
  let hoveredStructureConnector: ActiveStructureConnector | null = null;
  // Map view: the board rearranged BY the interaction graph — one frame per
  // page/surface definition, all structural arrows up at once. The prepared
  // model is shared with the example board; map mode only repositions.
  let mapMode = false;
  let mapReturnCamera: { x: number; y: number; scale: number } | null = null;
  let mapPlaceholderByNode = new Map<string, HTMLElement>();
  let mapNodeFrameIds = new Set<string>();
  let mapPreviewIdByNode = new Map<string, string>();
  let mapStructureConnectors: ActiveStructureConnector[] = [];
  // Global-nav plumbing (tab-bar edges repeated from most pages), collapsed
  // out of the default map and revealed by the Nav sub-toggle as faint
  // dashed hairlines. Visibility resets to hidden on every map entry.
  let mapNavConnectors: ActiveStructureConnector[] = [];
  let mapNavVisible = false;
  // Dragged node positions are session VIEW STATE, keyed by graph node id:
  // they survive mode toggles and revision reloads (pruned to nodes the new
  // graph still has) but never persist to disk. Never mutated in place —
  // every drag or reset swaps in a fresh map.
  let mapPositionOverrides: ReadonlyMap<string, MapPoint> = new Map();
  let mapDrag: {
    pointerId: number;
    nodeId: string;
    element: HTMLElement;
    start: MapPoint;
    origin: MapPoint;
    moved: boolean;
  } | null = null;
  let annotationLayerVisible = true;
  let destroyed = false;
  let retryTimer: number | undefined;
  const touches = new Map<number, Point>();
  const disposers: Array<() => void> = [];
  const annotationOverlay = new AnnotationOverlay({
    viewport: shell.viewport,
    root: shell.annotationOverlay,
    chrome: [shell.tools, shell.sourceDrawer, shell.status],
    focusPreview: (previewId, anchors) => {
      navigatePreview(previewId, true);
      const anchor = anchors?.find(anchorVisibleInViewport) ?? anchors?.[0] ?? null;
      if (anchor) scrollAnchorWithinPreview(anchor, model.frameById.get(previewId) ?? null);
      revealElement(anchor);
    },
    focusSourceTarget: (targetId) => {
      setSourceDrawer(true, false);
      const target = Array.from(
        shell.sourcePanel.querySelectorAll<HTMLElement>("[data-source-target-id]"),
      ).find((candidate) => candidate.dataset.sourceTargetId === targetId);
      target?.scrollIntoView({ block: "nearest" });
    },
  });

  const listen = <T extends EventTarget>(
    target: T,
    type: string,
    listener: EventListenerOrEventListenerObject,
    options?: AddEventListenerOptions | boolean,
  ): void => {
    target.addEventListener(type, listener, options);
    disposers.push(() => target.removeEventListener(type, listener, options));
  };

  const requestRulers = (): void => {
    if (!rulerFrame) rulerFrame = window.requestAnimationFrame(drawRulers);
  };

  const boardLocalRect = (node: Element): Rect => {
    const nodeRect = node.getBoundingClientRect();
    const boardRect = shell.board.getBoundingClientRect();
    return {
      x: (nodeRect.left - boardRect.left) / scale,
      y: (nodeRect.top - boardRect.top) / scale,
      width: nodeRect.width / scale,
      height: nodeRect.height / scale,
    };
  };

  const layoutConnector = (connector: PreparedWorkflowConnector): void => {
    const sourceShell = model.frameById.get(connector.sourceId)
      ?.querySelector<HTMLElement>(":scope > .preview-shell");
    const targetShell = model.frameById.get(connector.targetId)
      ?.querySelector<HTMLElement>(":scope > .preview-shell");
    const path = connector.element.querySelector<SVGPathElement>(".workflow-connector-path");
    const arrow = connector.element.querySelector<SVGPathElement>(".workflow-connector-arrow");
    const origin = connector.element.querySelector<SVGCircleElement>(".workflow-connector-origin");
    const label = connector.element.querySelector<SVGTextElement>(".workflow-connector-label");
    if (!sourceShell || !targetShell || !path || !arrow || !origin || !label) return;

    const source = boardLocalRect(sourceShell);
    const target = boardLocalRect(targetShell);
    // Replay connectors stay inside one subject row.
    const row = sourceShell.closest<HTMLElement>(".preview-row");
    const obstacles = row
      ? [...row.querySelectorAll<HTMLElement>(".preview-shell")].map(boardLocalRect)
      : [source, target];
    const route = routeWorkflowConnector(connector, source, target, obstacles);
    path.setAttribute("d", route.path);
    arrow.setAttribute("d", route.arrow);
    origin.setAttribute("cx", String(route.origin.x));
    origin.setAttribute("cy", String(route.origin.y));
    label.setAttribute("x", String(route.label.x));
    label.setAttribute("y", String(route.label.y));
  };

  // Every other frame on the board, as routing obstacles: stub fans clamp
  // inside the gap to the nearest neighbor so no drop crosses a frame.
  const structureNeighborRects = (selectedId: string): Rect[] =>
    [...model.frameById].flatMap(([previewId, frame]) => {
      if (previewId === selectedId) return [];
      const previewShell = frame.querySelector<HTMLElement>(":scope > .preview-shell");
      return previewShell ? [boardLocalRect(previewShell)] : [];
    });

  // Structural connectors anchor at the frame the user clicked and route
  // direction-aware edges to the far definition's first frame. Markers and
  // labels counter-scale with zoom so they stay legible on a zoomed-out map.
  const layoutStructureConnector = (
    connector: ActiveStructureConnector,
    markerScale: number,
    neighbors: readonly Rect[],
  ): void => {
    const selectedShell = model.frameById.get(connector.placement.selectedId)
      ?.querySelector<HTMLElement>(":scope > .preview-shell");
    const farShell = model.frameById.get(connector.placement.farId)
      ?.querySelector<HTMLElement>(":scope > .preview-shell");
    const path = connector.element.querySelector<SVGPathElement>(".workflow-connector-path");
    const arrow = connector.element.querySelector<SVGPathElement>(".workflow-connector-arrow");
    const origin = connector.element.querySelector<SVGCircleElement>(".workflow-connector-origin");
    const label = connector.element.querySelector<SVGTextElement>(".workflow-connector-label");
    const labelBackground = connector.element
      .querySelector<SVGRectElement>(".structure-connector-label-bg");
    if (!selectedShell || !farShell || !path || !arrow || !origin || !label || !labelBackground) {
      return;
    }

    const selectedRect = boardLocalRect(selectedShell);
    const route = routeStructureConnector(
      connector.placement,
      selectedRect,
      boardLocalRect(farShell),
      markerScale,
      neighbors,
    );
    // Rounded corners are presentation-only: the routed waypoints (and hence
    // the arrowhead, origin, and label anchors) never move.
    path.setAttribute("d", roundedStructurePath(route.path));
    // The draw-in dash sweep needs the rendered length; environments without
    // SVG geometry (jsdom) fall back to 0, which the animation treats as a
    // no-op rather than an error.
    connector.element.style.setProperty(
      "--structure-path-length",
      String(typeof path.getTotalLength === "function" ? path.getTotalLength() : 0),
    );
    arrow.setAttribute("d", route.arrow);
    origin.setAttribute("cx", String(route.origin.x));
    origin.setAttribute("cy", String(route.origin.y));
    origin.setAttribute("r", String(3 * markerScale));
    label.setAttribute("x", String(route.label.x));
    label.setAttribute("y", String(route.label.y));
    label.style.textAnchor = route.label.anchor;
    label.style.fontSize = `${STRUCTURE_LABEL_FONT * markerScale}px`;
    const box = label.getBBox();
    const paddingX = 4 * markerScale;
    const paddingY = 2 * markerScale;
    // Incoming left-edge pills recenter inside the inter-frame gap when the
    // measured pill fits with clearance; narrow gaps keep the flush anchor
    // and rely on the layer's z-lift for readability.
    const shift = connector.placement.direction === "incoming"
        && connector.placement.side === "left"
      ? incomingLeftLabelShift(
        { left: box.x - paddingX, right: box.x + box.width + paddingX },
        selectedRect,
        neighbors,
        markerScale,
      )
      : 0;
    if (shift !== 0) label.setAttribute("x", String(route.label.x + shift));
    labelBackground.setAttribute("x", String(box.x + shift - paddingX));
    labelBackground.setAttribute("y", String(box.y - paddingY));
    labelBackground.setAttribute("width", String(box.width + paddingX * 2));
    labelBackground.setAttribute("height", String(box.height + paddingY * 2));
    labelBackground.setAttribute("rx", String(4 * markerScale));
  };

  const layoutConnectors = (): void => {
    connectorFrame = 0;
    const boardRect = shell.board.getBoundingClientRect();
    model.connectorLayer.setAttribute("width", String(boardRect.width / scale));
    model.connectorLayer.setAttribute("height", String(boardRect.height / scale));
    for (const connector of model.connectors) layoutConnector(connector);
    const markerScale = Math.min(Math.max(1 / scale, 1), STRUCTURE_MARKER_SCALE_MAX);
    if (mapMode) {
      // Map connectors anchor at their own source frames, so the shared
      // obstacle set is every visible map node; each route drops its own
      // endpoints. Hidden example frames measure 0×0 and must not qualify.
      const mapObstacles = mapObstacleRects();
      for (const connector of activeStructureConnectors) {
        layoutStructureConnector(connector, markerScale, mapObstacles);
      }
    } else {
      // All active structural connectors share the clicked frame, so the
      // neighbor obstacle rects are measured once per layout pass.
      const selectedId = activeStructureConnectors[0]?.placement.selectedId;
      const structureNeighbors = selectedId === undefined
        ? []
        : structureNeighborRects(selectedId);
      for (const connector of activeStructureConnectors) {
        layoutStructureConnector(connector, markerScale, structureNeighbors);
      }
    }
    // Draw-in classes attach here — one animation frame after the selection
    // pass stripped any previous is-drawing class — so a replay reliably
    // restarts the sweep and the paths are already routed and measured.
    if (pendingStructureDraw) {
      pendingStructureDraw = false;
      activeStructureConnectors.forEach((connector, index) => {
        connector.element.style.setProperty(
          "--structure-draw-delay",
          `${structureDrawDelayMs(index)}ms`,
        );
        connector.element.classList.add("is-drawing");
      });
    }
  };

  const requestConnectors = (): void => {
    if (!connectorFrame) connectorFrame = window.requestAnimationFrame(layoutConnectors);
  };

  // Hovering a connector thickens its stroke and pill via CSS; this mirror
  // state additionally rings both endpoint frames in the connector's color.
  const setStructureHover = (next: ActiveStructureConnector | null): void => {
    if (next === hoveredStructureConnector) return;
    const previous = hoveredStructureConnector;
    hoveredStructureConnector = next;
    if (previous) {
      previous.element.classList.remove("is-hovered");
      for (const id of [previous.placement.selectedId, previous.placement.farId]) {
        const frame = model.frameById.get(id);
        frame?.classList.remove("is-connector-hover");
        frame?.style.removeProperty("--structure-hover-color");
      }
    }
    if (!next) return;
    next.element.classList.add("is-hovered");
    for (const id of [next.placement.selectedId, next.placement.farId]) {
      const frame = model.frameById.get(id);
      frame?.classList.add("is-connector-hover");
      frame?.style.setProperty("--structure-hover-color", `var(--structure-${next.kind})`);
    }
  };

  const setAnnotationLayerVisible = (visible: boolean): void => {
    annotationLayerVisible = visible;
    annotationOverlay.setCanvasVisible(visible);
    // Only replay connectors follow the annotation toggle; the layer itself
    // stays rendered so selection-driven structural arrows keep working (and
    // keep valid getBBox measurements) while annotations are hidden.
    setAnnotationConnectorsHidden(model.connectorLayer, !visible);
    requestConnectors();
  };

  const applyCamera = (): void => {
    shell.board.style.transform = `translate(${x}px, ${y}px) scale(${scale})`;
    shell.board.style.setProperty("--selection-stroke", `${2 / scale}px`);
    shell.board.style.setProperty("--selection-offset", `${4 / scale}px`);
    shell.board.style.setProperty("--connector-stroke", `${1.5 / scale}px`);
    shell.zoomOutput.textContent = `${Math.round(scale * 100)}%`;
    requestRulers();
    annotationOverlay.invalidate();
    requestConnectors();
  };

  const clampScale = (value: number): number => Math.min(Math.max(value, MIN_SCALE), MAX_SCALE);
  const viewportCenter = (): Point => ({
    x: shell.viewport.clientWidth / 2,
    y: shell.viewport.clientHeight / 2,
  });
  const localPoint = (clientX: number, clientY: number): Point => {
    const rect = shell.viewport.getBoundingClientRect();
    return { x: clientX - rect.left, y: clientY - rect.top };
  };
  const zoomAt = (value: number, point: Point): void => {
    const nextScale = clampScale(value);
    const ratio = nextScale / scale;
    x = point.x - (point.x - x) * ratio;
    y = point.y - (point.y - y) * ratio;
    scale = nextScale;
    applyCamera();
  };

  const chooseRulerStep = (): number => {
    const desiredWorldUnits = 76 / scale;
    const magnitude = 10 ** Math.floor(Math.log10(desiredWorldUnits));
    const normalized = desiredWorldUnits / magnitude;
    return (normalized <= 1 ? 1 : normalized <= 2 ? 2 : normalized <= 5 ? 5 : 10) * magnitude;
  };
  const prepareCanvas = (canvas: HTMLCanvasElement): PreparedCanvas => {
    const rect = canvas.getBoundingClientRect();
    const ratio = window.devicePixelRatio || 1;
    const width = Math.max(1, Math.round(rect.width));
    const height = Math.max(1, Math.round(rect.height));
    const pixelWidth = Math.round(width * ratio);
    const pixelHeight = Math.round(height * ratio);
    if (canvas.width !== pixelWidth || canvas.height !== pixelHeight) {
      canvas.width = pixelWidth;
      canvas.height = pixelHeight;
    }
    const context = canvas.getContext("2d");
    if (!context) throw new Error("2D canvas rendering is unavailable");
    context.setTransform(ratio, 0, 0, ratio, 0, 0);
    context.clearRect(0, 0, width, height);
    context.strokeStyle = "#aeb5be";
    context.fillStyle = "#68717d";
    context.lineWidth = 1;
    context.font = "9px ui-monospace, SFMono-Regular, Menlo, monospace";
    return { context, width, height };
  };

  function drawRulers(): void {
    rulerFrame = 0;
    const horizontal = prepareCanvas(shell.rulerX);
    const vertical = prepareCanvas(shell.rulerY);
    const step = chooseRulerStep();
    const minor = step / 5;
    const firstX = Math.floor((-x / scale) / minor) * minor;
    const lastX = (horizontal.width - x) / scale;
    horizontal.context.beginPath();
    for (let worldX = firstX; worldX <= lastX + minor; worldX += minor) {
      const screen = Math.round(x + worldX * scale) + 0.5;
      const major = Math.abs(worldX / step - Math.round(worldX / step)) < 0.001;
      horizontal.context.moveTo(screen, horizontal.height);
      horizontal.context.lineTo(screen, horizontal.height - (major ? 9 : 4));
      if (major) horizontal.context.fillText(String(Math.round(worldX)), screen + 3, 9);
    }
    horizontal.context.stroke();

    const firstY = Math.floor((-y / scale) / minor) * minor;
    const lastY = (vertical.height - y) / scale;
    vertical.context.beginPath();
    for (let worldY = firstY; worldY <= lastY + minor; worldY += minor) {
      const screen = Math.round(y + worldY * scale) + 0.5;
      const major = Math.abs(worldY / step - Math.round(worldY / step)) < 0.001;
      vertical.context.moveTo(vertical.width, screen);
      vertical.context.lineTo(vertical.width - (major ? 9 : 4), screen);
      if (major) {
        vertical.context.save();
        vertical.context.translate(9, screen - 3);
        vertical.context.rotate(-Math.PI / 2);
        vertical.context.fillText(String(Math.round(worldY)), 0, 0);
        vertical.context.restore();
      }
    }
    vertical.context.stroke();
  }

  const effectiveTool = (): Tool => selectedTool === "hand" || spaceHeld || pan ? "hand" : "cursor";
  const renderTools = (): void => {
    const tool = effectiveTool();
    shell.cursorButton.setAttribute("aria-pressed", String(tool === "cursor"));
    shell.handButton.setAttribute("aria-pressed", String(tool === "hand"));
    shell.viewport.dataset.tool = tool;
  };
  const selectTool = (tool: Tool): void => {
    selectedTool = tool;
    renderTools();
  };
  const finishPan = (pointerId?: number): void => {
    if (!pan || (pointerId !== undefined && pan.pointerId !== pointerId)) return;
    if (pan.moved) suppressClickUntil = performance.now() + 250;
    pan = null;
    shell.viewport.classList.remove("panning");
    renderTools();
  };
  const beginPan = (pointerId: number, point: Point): void => {
    pan = { pointerId, pointerX: point.x, pointerY: point.y, x, y, moved: false };
    shell.viewport.classList.add("panning");
    renderTools();
  };
  const updatePan = (pointerId: number, point: Point): void => {
    if (!pan || pan.pointerId !== pointerId) return;
    const deltaX = point.x - pan.pointerX;
    const deltaY = point.y - pan.pointerY;
    if (Math.hypot(deltaX, deltaY) > 3) pan.moved = true;
    x = pan.x + deltaX;
    y = pan.y + deltaY;
    applyCamera();
  };

  const frameWorldRect = (element: HTMLElement): Rect => {
    const frameRect = element.getBoundingClientRect();
    const viewportRect = shell.viewport.getBoundingClientRect();
    return {
      x: (frameRect.left - viewportRect.left - x) / scale,
      y: (frameRect.top - viewportRect.top - y) / scale,
      width: frameRect.width / scale,
      height: frameRect.height / scale,
    };
  };
  const composedElementParent = (element: HTMLElement): HTMLElement | null => {
    if (element.parentElement) return element.parentElement;
    const root = element.getRootNode();
    return root instanceof ShadowRoot && root.host instanceof HTMLElement ? root.host : null;
  };
  const intersectRects = (left: DOMRect, right: DOMRect): Rect | null => {
    const x = Math.max(left.left, right.left);
    const y = Math.max(left.top, right.top);
    const farX = Math.min(left.right, right.right);
    const farY = Math.min(left.bottom, right.bottom);
    return farX > x && farY > y
      ? { x, y, width: farX - x, height: farY - y }
      : null;
  };
  const clipsContent = (element: HTMLElement): boolean => {
    const style = window.getComputedStyle(element);
    return /(?:auto|scroll|hidden|clip)/.test(
      `${style.overflow} ${style.overflowX} ${style.overflowY}`,
    );
  };
  const anchorVisibleInViewport = (anchor: HTMLElement): boolean => {
    const anchorRect = anchor.getBoundingClientRect();
    if (!intersectRects(anchorRect, shell.viewport.getBoundingClientRect())) return false;
    let ancestor = composedElementParent(anchor);
    while (ancestor && ancestor !== shell.viewport) {
      if (clipsContent(ancestor) && !intersectRects(anchorRect, ancestor.getBoundingClientRect())) {
        return false;
      }
      ancestor = composedElementParent(ancestor);
    }
    return true;
  };
  const scrollAnchorWithinPreview = (
    anchor: HTMLElement,
    frame: HTMLElement | null,
  ): void => {
    let ancestor = composedElementParent(anchor);
    while (ancestor && ancestor !== frame && ancestor !== shell.viewport) {
      const style = window.getComputedStyle(ancestor);
      const overflow = `${style.overflow} ${style.overflowX} ${style.overflowY}`;
      const scrolls = /(?:auto|scroll)/.test(overflow);
      if (scrolls) {
        const target = anchor.getBoundingClientRect();
        const viewport = ancestor.getBoundingClientRect();
        if (ancestor.scrollHeight > ancestor.clientHeight) {
          ancestor.scrollTop += target.top + target.height / 2
            - (viewport.top + viewport.height / 2);
        }
        if (ancestor.scrollWidth > ancestor.clientWidth) {
          ancestor.scrollLeft += target.left + target.width / 2
            - (viewport.left + viewport.width / 2);
        }
      }
      ancestor = composedElementParent(ancestor);
    }
  };
  const revealElement = (element: HTMLElement | null): void => {
    if (!element) return;
    const rect = frameWorldRect(element);
    x = shell.viewport.clientWidth / 2 - (rect.x + rect.width / 2) * scale;
    y = shell.viewport.clientHeight / 2 - (rect.y + rect.height / 2) * scale;
    applyCamera();
  };

  const focusedPreviewId = (): string | null =>
    previewIdForIdentity(model, focusState?.identity ?? null);
  const cancelFocusFit = (): void => {
    focusFitGeneration += 1;
    if (!focusFitFrame) return;
    window.cancelAnimationFrame(focusFitFrame);
    focusFitFrame = 0;
  };
  const scheduleFocusFit = (): void => {
    const previewId = focusedPreviewId();
    if (!previewId) return;
    cancelFocusFit();
    const generation = focusFitGeneration;
    focusFitFrame = window.requestAnimationFrame(() => {
      focusFitFrame = 0;
      if (
        destroyed
        || generation !== focusFitGeneration
        || focusedPreviewId() !== previewId
      ) return;
      const frame = model.frameById.get(previewId);
      if (!frame) return;
      const camera = fitPreviewCamera(
        frameWorldRect(frame),
        shell.viewport.clientWidth,
        shell.viewport.clientHeight,
        FOCUS_PADDING,
        MIN_SCALE,
        FOCUS_MAX_SCALE,
      );
      x = camera.x;
      y = camera.y;
      scale = camera.scale;
      applyCamera();
    });
  };
  const observeFocusedFrame = (frame: HTMLElement | null): void => {
    focusFrameObserver?.disconnect();
    if (!frame || !window.ResizeObserver) return;
    focusFrameObserver ??= new window.ResizeObserver(scheduleFocusFit);
    focusFrameObserver.observe(frame);
  };
  const syncFocusPresentation = (): void => {
    const previewId = focusedPreviewId();
    for (const frame of model.frameById.values()) frame.classList.remove("is-focus-target");
    shell.board.querySelectorAll<HTMLElement>(".preview-row").forEach((row) => {
      row.classList.remove("is-focus-row");
    });
    shell.navigatorResults.querySelectorAll<HTMLElement>(".navigator-frame").forEach((button) => {
      button.classList.remove("is-focus-target");
    });

    const focusedPreview = previewId ? model.previewById.get(previewId) ?? null : null;
    const frame = previewId ? model.frameById.get(previewId) ?? null : null;
    const active = Boolean(focusState && focusedPreview && frame);
    shell.shell.classList.toggle("is-focus-mode", active);
    shell.board.classList.toggle("is-focus-mode", active);
    shell.focusHeader.hidden = !active;
    shell.focusSelectionButton.setAttribute(
      "aria-label",
      active ? "Fit focused preview" : "Center selected preview",
    );
    shell.focusSelectionButton.title = active ? "Fit focused preview" : "Center selected preview";
    annotationOverlay.setFocusedPreview(active ? previewId : null);
    if (!active || !focusedPreview || !frame || !previewId) {
      observeFocusedFrame(null);
      return;
    }
    shell.focusBreadcrumbKind.textContent = focusedPreview.identity.kind;
    shell.focusBreadcrumbSubject.textContent = focusedPreview.identity.subject;
    shell.focusBreadcrumbExample.textContent = focusedPreview.identity.example;
    frame.classList.add("is-focus-target");
    frame.closest<HTMLElement>(".preview-row")?.classList.add("is-focus-row");
    const navigatorButton = Array.from(
      shell.navigatorResults.querySelectorAll<HTMLElement>(".navigator-frame[data-preview-id]"),
    ).find((button) => button.dataset.previewId === previewId);
    navigatorButton?.classList.add("is-focus-target");
    observeFocusedFrame(frame);
  };

  const revealPreviewFlow = (previewId: string): void => {
    const selected = model.frameById.get(previewId);
    if (!selected) return;
    // Replay connectors are hidden on the map (and connect hidden example
    // frames), so map reveals center the node itself.
    if (mapMode) {
      revealElement(selected);
      return;
    }
    const active = model.connectors.filter((connector) =>
      connector.sourceId === previewId || connector.targetId === previewId);
    if (active.length === 0) {
      revealElement(selected);
      return;
    }

    const frameIds = new Set([previewId]);
    for (const connector of active) {
      frameIds.add(connector.sourceId);
      frameIds.add(connector.targetId);
    }
    const rects = [...frameIds].flatMap((id) => {
      const frame = model.frameById.get(id);
      return frame ? [frameWorldRect(frame)] : [];
    });
    if (rects.length === 0) return;

    const minX = Math.min(...rects.map((rect) => rect.x));
    const maxX = Math.max(...rects.map((rect) => rect.x + rect.width));
    const frameTop = Math.min(...rects.map((rect) => rect.y));
    const laneCount = Math.max(...active.map((connector) => connector.lane + 1));
    const minY = frameTop - workflowRailHeight(laneCount);
    const maxY = Math.max(...rects.map((rect) => rect.y + rect.height));
    const width = Math.max(1, maxX - minX);
    const height = Math.max(1, maxY - minY);
    const padding = 72;
    const fitScale = Math.min(
      (shell.viewport.clientWidth - padding) / width,
      (shell.viewport.clientHeight - padding) / height,
    );
    scale = clampScale(Math.min(scale, fitScale));
    x = shell.viewport.clientWidth / 2 - (minX + width / 2) * scale;
    y = shell.viewport.clientHeight / 2 - (minY + height / 2) * scale;
    applyCamera();
  };

  // --- Map view ------------------------------------------------------------
  // Map mode swaps the example-grouped board for a graph-derived layout: the
  // first preview frame of every page/surface definition, positioned by
  // navigation depth (map-layout.ts), with every structural edge drawn at
  // once. Toggling is pure presentation over the same prepared model — no
  // reload, and Board mode DOM is restored exactly on the way back.

  const mapFallbackSize = (nodeId: string): MapNodeSize => {
    if (mapPlaceholderByNode.has(nodeId)) return MAP_PLACEHOLDER_FALLBACK;
    return nodeId.startsWith("surface:") ? MAP_SURFACE_FALLBACK : MAP_PAGE_FALLBACK;
  };

  /** A dashed stand-in card for a graph node without any preview frame. */
  const buildMapPlaceholder = (node: InteractionGraphNode): HTMLElement => {
    const figure = document.createElement("figure");
    figure.className = "editor-frame is-map-node map-placeholder";
    figure.dataset.mapNode = node.id;
    const card = document.createElement("div");
    card.className = "preview-shell map-card";
    const kind = document.createElement("span");
    kind.className = "map-card-kind";
    kind.textContent = node.kind;
    const name = document.createElement("strong");
    name.className = "map-card-name";
    name.textContent = node.label;
    const hint = document.createElement("span");
    hint.className = "map-card-hint";
    hint.textContent = "No preview in this snapshot";
    card.append(kind, name, hint);
    figure.append(card);
    return figure;
  };

  const teardownMapDom = (): void => {
    shell.board.classList.remove("is-map-mode");
    model.connectorLayer.classList.remove("is-map-mode");
    shell.board.style.removeProperty("width");
    shell.board.style.removeProperty("height");
    shell.board.style.removeProperty("--map-node-scale");
    cancelMapDrag();
    for (const frame of model.frameById.values()) {
      frame.classList.remove("is-map-node", "is-map-dragging");
      frame.style.removeProperty("left");
      frame.style.removeProperty("top");
      delete frame.dataset.mapNode;
    }
    for (const placeholder of mapPlaceholderByNode.values()) placeholder.remove();
    for (const connector of model.structureConnectors) {
      connector.element.classList.remove("is-global-nav");
    }
    mapPlaceholderByNode = new Map();
    mapNodeFrameIds = new Set();
    mapPreviewIdByNode = new Map();
    mapStructureConnectors = [];
    mapNavConnectors = [];
    mapNavVisible = false;
    // mapPositionOverrides intentionally survives: toggling back into map
    // mode within the session restores dragged positions.
    syncNavToggle();
    syncMapResetButton();
  };

  const applyMapLayout = (): void => {
    const render = model.render;
    if (!render) return;
    cancelMapDrag();
    for (const placeholder of mapPlaceholderByNode.values()) placeholder.remove();
    mapPlaceholderByNode = new Map();
    const graph = render.interactionGraph;
    const frames = mapNodePreviewIds(graph.nodes, render.previews);
    mapPreviewIdByNode = frames;
    mapNodeFrameIds = new Set(frames.values());
    shell.board.classList.add("is-map-mode");
    shell.board.style.setProperty("--map-node-scale", String(MAP_NODE_SCALE));
    model.connectorLayer.classList.add("is-map-mode");
    const elementByNode = new Map<string, HTMLElement>();
    for (const node of graph.nodes) {
      if (node.kind !== "page" && node.kind !== "surface") continue;
      const previewId = frames.get(node.id);
      const frame = previewId === undefined ? undefined : model.frameById.get(previewId);
      if (frame) {
        frame.classList.add("is-map-node");
        // The drag handler resolves a pressed frame back to its graph node
        // through this tag (placeholders carry it from construction).
        frame.dataset.mapNode = node.id;
        elementByNode.set(node.id, frame);
        continue;
      }
      const placeholder = buildMapPlaceholder(node);
      shell.board.append(placeholder);
      mapPlaceholderByNode.set(node.id, placeholder);
      elementByNode.set(node.id, placeholder);
    }
    // Nodes are measured AFTER the mode class flips so the map CSS (absolute
    // frames, hidden example rows) governs the measured footprint. Frames
    // render under a `scale(MAP_NODE_SCALE)` transform that offsetWidth/
    // offsetHeight do NOT reflect, so the raw box scales here — positions are
    // computed from the same visual rects getBoundingClientRect reports to
    // the connector router and the camera fit.
    const sizeOf = (nodeId: string): MapNodeSize => {
      const element = elementByNode.get(nodeId);
      const width = element?.offsetWidth ?? 0;
      const height = element?.offsetHeight ?? 0;
      return scaledMapNodeSize(
        width > 0 && height > 0 ? { width, height } : mapFallbackSize(nodeId),
      );
    };
    // Derived layout first, then session drag overrides on top — pruned so
    // a node the new revision dropped never leaves a stale pin behind.
    mapPositionOverrides = retainMapOverrides(
      mapPositionOverrides,
      new Set(elementByNode.keys()),
    );
    const positions = applyMapOverrides(
      layoutInteractionMap(graph, sizeOf),
      mapPositionOverrides,
    );
    let width = 0;
    let height = 0;
    for (const [nodeId, position] of positions) {
      const element = elementByNode.get(nodeId);
      if (!element) continue;
      element.style.left = `${position.x}px`;
      element.style.top = `${position.y}px`;
      const size = sizeOf(nodeId);
      width = Math.max(width, position.x + size.width);
      height = Math.max(height, position.y + size.height);
    }
    // With the flow content hidden the board box no longer tracks the map;
    // explicit extents keep hit-testing and layer sizing over every node.
    shell.board.style.width = `${width}px`;
    shell.board.style.height = `${height}px`;
    // Global-nav plumbing is fanned separately from the flow edges so the
    // default (nav hidden) map keeps compact slot fans with no gaps.
    const { flow, globalNav } = splitGlobalNavConnectors(model.structureConnectors);
    mapStructureConnectors = layoutMapStructureConnectors(flow);
    mapNavConnectors = layoutMapStructureConnectors(globalNav);
    for (const connector of model.structureConnectors) {
      connector.element.classList.remove("is-global-nav");
    }
    for (const connector of mapNavConnectors) {
      connector.element.classList.add("is-global-nav");
    }
    syncNavToggle();
    syncMapResetButton();
  };

  /** The Nav sub-toggle exists only on a map that actually collapsed edges. */
  const syncNavToggle = (): void => {
    shell.navToggleButton.hidden = !mapMode || mapNavConnectors.length === 0;
    shell.navToggleButton.setAttribute("aria-pressed", String(mapNavVisible));
  };

  /** Reset layout exists only while dragged nodes diverge from the map. */
  const syncMapResetButton = (): void => {
    shell.mapResetButton.hidden = !mapMode || mapPositionOverrides.size === 0;
  };

  // --- Map drag ------------------------------------------------------------
  // Pointer down on a map node arms a potential drag; crossing the movement
  // threshold turns it into one (below it, the release clicks-to-select as
  // always). While dragging, the frame tracks the pointer in board units and
  // the structural arrows relayout live through the existing rAF batch. The
  // result lands in mapPositionOverrides — draw-in never replays, because a
  // drag is not a selection change.

  const beginMapDrag = (event: PointerEvent): boolean => {
    if (
      !mapMode
      || mapDrag !== null
      || event.button !== 0
      || event.pointerType === "touch"
      || effectiveTool() !== "cursor"
    ) return false;
    const frame = closest<HTMLElement>(event.target, ".editor-frame.is-map-node");
    const nodeId = frame?.dataset.mapNode;
    if (!frame || nodeId === undefined) return false;
    mapDrag = {
      pointerId: event.pointerId,
      nodeId,
      element: frame,
      start: { x: event.clientX, y: event.clientY },
      origin: {
        x: Number.parseFloat(frame.style.left) || 0,
        y: Number.parseFloat(frame.style.top) || 0,
      },
      moved: false,
    };
    return true;
  };

  const updateMapDrag = (event: PointerEvent): boolean => {
    if (!mapDrag || event.pointerId !== mapDrag.pointerId) return false;
    const current = { x: event.clientX, y: event.clientY };
    if (!mapDrag.moved && isDragGesture(mapDrag.start, current)) {
      mapDrag.moved = true;
      mapDrag.element.classList.add("is-map-dragging");
      // Capture only once the gesture IS a drag: capturing at press would
      // retarget the pointerup — and the click the browser synthesizes from
      // it — to the viewport, silently breaking click-to-select on nodes.
      shell.viewport.setPointerCapture(event.pointerId);
    }
    if (!mapDrag.moved) return true;
    const position = draggedMapPosition(mapDrag.origin, mapDrag.start, current, scale);
    mapDrag.element.style.left = `${position.x}px`;
    mapDrag.element.style.top = `${position.y}px`;
    requestConnectors();
    return true;
  };

  const finishMapDrag = (pointerId: number): void => {
    if (!mapDrag || mapDrag.pointerId !== pointerId) return;
    const drag = mapDrag;
    mapDrag = null;
    drag.element.classList.remove("is-map-dragging");
    // A press that never crossed the threshold is a click: the click event
    // that follows runs the normal select path, untouched.
    if (!drag.moved) return;
    suppressClickUntil = performance.now() + 250;
    mapPositionOverrides = setMapOverride(mapPositionOverrides, drag.nodeId, {
      x: Number.parseFloat(drag.element.style.left) || 0,
      y: Number.parseFloat(drag.element.style.top) || 0,
    });
    syncMapResetButton();
    requestConnectors();
  };

  const cancelMapDrag = (): void => {
    if (!mapDrag) return;
    mapDrag.element.classList.remove("is-map-dragging");
    mapDrag = null;
  };

  /** Clears every drag override and returns the map to the derived layout. */
  const resetMapLayout = (): void => {
    if (!mapMode || mapPositionOverrides.size === 0) return;
    mapPositionOverrides = new Map();
    applyMapLayout();
    // Re-arm the arrows through the one selection pipeline so spotlight and
    // emphasis states stay exactly as they were before the reset.
    const selectedId = previewIdForIdentity(model, selectedIdentity);
    if (selectedId) selectPreview(selectedId, false);
    else activateMapStructureConnectors();
    requestConnectors();
  };

  /** Every visible map node's shell, as routing obstacles for the arrows. */
  const mapObstacleRects = (): Rect[] => {
    const shells: Element[] = [];
    for (const id of mapNodeFrameIds) {
      const frameShell = model.frameById.get(id)
        ?.querySelector<HTMLElement>(":scope > .preview-shell");
      if (frameShell) shells.push(frameShell);
    }
    for (const placeholder of mapPlaceholderByNode.values()) {
      const cardShell = placeholder.querySelector<HTMLElement>(":scope > .preview-shell");
      if (cardShell) shells.push(cardShell);
    }
    return shells.map(boardLocalRect);
  };

  /**
   * Re-arms the structural set after any selection pass: in map mode every
   * flow arrow stays up (selection only dims unrelated ones), global-nav
   * plumbing joins only while the Nav sub-toggle is on, and every pill reads
   * source-relative (`event → target`).
   */
  const activateMapStructureConnectors = (): void => {
    for (const connector of mapNavConnectors) {
      connector.element.classList.toggle("is-active", mapNavVisible);
    }
    activeStructureConnectors = mapNavVisible
      ? [...mapStructureConnectors, ...mapNavConnectors]
      : mapStructureConnectors;
    if (activeStructureConnectors.length === 0) return;
    model.connectorLayer.classList.add("has-structure");
    for (const connector of activeStructureConnectors) {
      connector.element.classList.add("is-active");
      connector.element.classList.remove(
        "is-incoming",
        "is-map-dimmed",
        "is-map-emphasized",
      );
      connector.element.dataset.direction = "outgoing";
      connector.element.dataset.edge = connector.placement.side;
      connector.element.dataset.slot =
        `${connector.placement.slot + 1}/${connector.placement.slotCount}`;
      const label = connector.element
        .querySelector<SVGTextElement>(".workflow-connector-label");
      if (label) renderStructureConnectorLabel(label, connector, "outgoing");
    }
  };

  /**
   * The Nav sub-toggle: reveals or re-collapses the global-nav plumbing
   * without touching selection. Re-selecting the current preview reuses the
   * one selection pipeline so spotlight dimming stays consistent.
   */
  const setMapNavVisible = (next: boolean): void => {
    if (!mapMode || next === mapNavVisible) return;
    mapNavVisible = next;
    syncNavToggle();
    const selectedId = previewIdForIdentity(model, selectedIdentity);
    if (selectedId) selectPreview(selectedId, false);
    else activateMapStructureConnectors();
    requestConnectors();
  };

  /** Fits the camera around every map node, measured in world coordinates. */
  const fitMapCamera = (): void => {
    // The camera model owns all panning via the board transform; any native
    // scroll smuggled onto the viewport (e.g. browser scroll-on-focus) would
    // shift every measured rect, so it resets before the fit measures.
    shell.viewport.scrollLeft = 0;
    shell.viewport.scrollTop = 0;
    const elements = [
      ...[...mapNodeFrameIds].flatMap((id) => {
        const frame = model.frameById.get(id);
        return frame ? [frame] : [];
      }),
      ...mapPlaceholderByNode.values(),
    ];
    if (elements.length === 0) return;
    const rects = elements.map(frameWorldRect);
    const minX = Math.min(...rects.map((rect) => rect.x));
    const minY = Math.min(...rects.map((rect) => rect.y));
    const width = Math.max(1, Math.max(...rects.map((rect) => rect.x + rect.width)) - minX);
    const height = Math.max(1, Math.max(...rects.map((rect) => rect.y + rect.height)) - minY);
    scale = clampScale(Math.min(
      1,
      (shell.viewport.clientWidth - MAP_FIT_PADDING) / width,
      (shell.viewport.clientHeight - MAP_FIT_PADDING) / height,
    ));
    x = shell.viewport.clientWidth / 2 - (minX + width / 2) * scale;
    y = shell.viewport.clientHeight / 2 - (minY + height / 2) * scale;
    applyCamera();
  };

  /**
   * The frame a selection lands on in map mode: previews whose frame the map
   * hides (later examples of a definition) resolve to their definition's map
   * node, so navigator clicks always select something visible.
   */
  const mapSelectionTargetId = (previewId: string): string => {
    if (mapNodeFrameIds.has(previewId)) return previewId;
    const preview = model.previewById.get(previewId);
    if (!preview) return previewId;
    return mapPreviewIdByNode.get(structureDefinitionNode(preview.identity)) ?? previewId;
  };

  const setMapMode = (next: boolean): void => {
    if (next === mapMode || (next && !model.render)) return;
    if (next && focusState) leavePreviewFocus(false);
    mapMode = next;
    shell.mapToggleButton.setAttribute("aria-pressed", String(next));
    // A mode flip resets selection through the same path as Escape / a click
    // on empty stage, before the layout swap: entering or leaving the map
    // yields one deterministic board no matter what was selected (no carried
    // spotlight or inspector), and selection starts fresh on the new surface.
    clearSelection();
    if (next) {
      mapReturnCamera = { x, y, scale };
      applyMapLayout();
      // The map's structural arrows are mode state, not selection state:
      // they arm as soon as the layout exists.
      activateMapStructureConnectors();
      fitMapCamera();
      pendingStructureDraw = activeStructureConnectors.length > 0;
      requestConnectors();
    } else {
      teardownMapDom();
      if (mapReturnCamera) {
        x = mapReturnCamera.x;
        y = mapReturnCamera.y;
        scale = mapReturnCamera.scale;
        mapReturnCamera = null;
        applyCamera();
      }
    }
  };

  const clearSelectionDom = (): void => {
    for (const frame of model.frameById.values()) {
      frame.classList.remove("is-selected", "is-related", "is-connector-hover");
      frame.style.removeProperty("--structure-hover-color");
      frame.setAttribute("aria-pressed", "false");
    }
    shell.board.classList.remove("is-spotlight");
    model.connectorLayer.classList.remove("has-selection", "has-structure");
    for (const connector of [...model.connectors, ...model.structureConnectors]) {
      connector.element.classList.remove(
        "is-active",
        "is-incoming",
        "is-drawing",
        "is-hovered",
        "is-map-dimmed",
        "is-map-emphasized",
      );
    }
    hoveredStructureConnector = null;
    pendingStructureDraw = false;
    activeStructureConnectors = [];
    // The map's structural arrows are mode state, not selection state: any
    // selection reset immediately re-arms the full set.
    if (mapMode) activateMapStructureConnectors();
    shell.navigatorResults.querySelectorAll<HTMLElement>("[data-preview-id]").forEach((button) => {
      button.setAttribute("aria-pressed", "false");
      button.removeAttribute("aria-current");
    });
  };

  const renderData = (preview: EditorPreview): void => {
    shell.selectionData.replaceChildren();
    const order: PreviewDataField["group"][] = ["page-address", "properties", "provided-data"];
    for (const group of order) {
      const fields = preview.data.filter((field) => field.group === group);
      if (fields.length === 0) continue;
      const section = document.createElement("section");
      section.className = "preview-data-group";
      const heading = document.createElement("h4");
      heading.textContent = dataGroupTitle(group);
      const list = document.createElement("dl");
      list.className = "preview-data-list";
      for (const field of fields) {
        const row = document.createElement("div");
        row.className = "preview-data-row";
        const term = document.createElement("dt");
        term.textContent = field.name;
        const description = document.createElement("dd");
        const value = document.createElement("span");
        value.className = field.status === "ready" ? "preview-data-value" : "preview-data-state";
        value.textContent = field.status === "ready"
          ? formatValue(field.value)
          : field.status === "waiting"
            ? "Waiting for data"
            : "Couldn’t load";
        description.append(value);
        if (field.reason) {
          const reason = document.createElement("span");
          reason.className = "preview-data-reason";
          reason.textContent = field.reason;
          description.append(reason);
        }
        const source = sourceDescription(field);
        if (source) {
          const sourceNode = document.createElement("p");
          sourceNode.className = "preview-data-source";
          sourceNode.textContent = `Source: ${source}`;
          description.append(sourceNode);
        }
        row.append(term, description);
        list.append(row);
      }
      section.append(heading, list);
      shell.selectionData.append(section);
    }
    shell.selectionNoData.hidden = preview.data.length > 0;
  };

  const replayEffectGroups = (step: ReplayStep): Array<[string, JsonValue[]]> => {
    const groups: Array<[string, JsonValue[]]> = [
      ["State writes", step.effects.writes],
      ["Commands", step.effects.commands],
      ["Intents", step.effects.intents],
      ["Structure", step.effects.structural],
      ["Projection deliveries", step.effects.projections],
    ];
    return groups.filter(([, values]) => values.length > 0);
  };

  const renderWorkflow = (preview: EditorPreview): void => {
    shell.selectionWorkflow.replaceChildren(...preview.replay.map((step, index) => {
      const item = document.createElement("li");
      item.className = "workflow-step";

      const heading = document.createElement("div");
      heading.className = "workflow-step-heading";
      const ordinal = document.createElement("span");
      ordinal.className = "workflow-step-ordinal";
      ordinal.textContent = String(index + 1);
      const title = document.createElement("strong");
      title.textContent = step.label;
      const kind = document.createElement("span");
      kind.className = "workflow-step-kind";
      kind.textContent = step.kind;
      heading.append(ordinal, title, kind);
      item.append(heading);

      const payload = document.createElement("details");
      payload.className = "workflow-detail";
      const payloadSummary = document.createElement("summary");
      payloadSummary.textContent = "Step payload";
      const payloadValue = document.createElement("pre");
      payloadValue.textContent = JSON.stringify(step.payload, null, 2);
      payload.append(payloadSummary, payloadValue);
      item.append(payload);

      if (step.dispatch) {
        const dispatch = document.createElement("div");
        dispatch.className = "workflow-dispatch";
        const selected = step.dispatch.selected === null
          ? "no handler selected"
          : `handler #${step.dispatch.selected}`;
        dispatch.textContent = `${step.dispatch.definition} · ${selected} · on ${step.dispatch.on}`;
        dispatch.title = `Scope: ${step.dispatch.scope}`;
        item.append(dispatch);

        if (step.dispatch.guards.length > 0) {
          const guards = document.createElement("ul");
          guards.className = "workflow-guards";
          for (const guard of step.dispatch.guards) {
            const guardNode = document.createElement("li");
            guardNode.dataset.result = guard.result;
            guardNode.textContent = `#${guard.handler} ${guard.result}`;
            guards.append(guardNode);
          }
          item.append(guards);
        }
      }

      const effectGroups = replayEffectGroups(step);
      for (const [label, values] of effectGroups) {
        const effects = document.createElement("details");
        effects.className = "workflow-detail workflow-effects";
        const summary = document.createElement("summary");
        summary.textContent = `${label} · ${values.length}`;
        const value = document.createElement("pre");
        value.textContent = JSON.stringify(values, null, 2);
        effects.append(summary, value);
        item.append(effects);
      }
      if (effectGroups.length === 0) {
        const noEffects = document.createElement("p");
        noEffects.className = "workflow-no-effects";
        noEffects.textContent = "No committed effects";
        item.append(noEffects);
      }
      return item;
    }));
    shell.selectionWorkflowBlock.hidden = preview.replay.length === 0;
  };

  const renderSurfaceHierarchy = (preview: EditorPreview): void => {
    const hierarchy = surfaceHierarchy(preview, model.render?.previews ?? [preview]);
    if (!hierarchy || hierarchy.surfaces.length === 0) {
      shell.selectionHierarchy.replaceChildren();
      shell.selectionHierarchyBlock.hidden = true;
      return;
    }
    const renderNode = (node: SurfaceHierarchyNode): HTMLLIElement => {
      const child = document.createElement("li");
      child.className = "surface-hierarchy-child";
      child.dataset.surfaceKey = node.surface.key;
      if (node.opener !== null) child.dataset.opener = node.opener;
      const label = document.createElement("strong");
      label.textContent = `${node.surface.modality} ${node.surface.definition}`;
      const relation = document.createElement("span");
      relation.textContent = {
        direct: "opened by this replay",
        inherited: "inherited from replay ancestry",
        mounted: "mounted in this snapshot",
      }[node.surface.relation];
      child.append(label, relation);
      if (node.children.length > 0) {
        const descendants = document.createElement("ul");
        descendants.append(...node.children.map(renderNode));
        child.append(descendants);
      }
      return child;
    };
    const root = document.createElement("li");
    root.className = "surface-hierarchy-root";
    root.textContent = `page ${hierarchy.page}`;
    const children = document.createElement("ul");
    children.append(...hierarchy.roots.map(renderNode));
    root.append(children);
    shell.selectionHierarchy.replaceChildren(root);
    shell.selectionHierarchyBlock.hidden = false;
  };

  const renderInspector = (preview: EditorPreview): void => {
    const focused = focusedPreviewId() === preview.id;
    shell.inspectorOverview.hidden = true;
    shell.inspectorSelection.hidden = false;
    shell.focusSelectionButton.disabled = false;
    shell.clearSelectionButton.hidden = focused;
    shell.selectionKind.textContent = focused
      ? `Focused ${preview.identity.kind}`
      : preview.identity.kind;
    shell.selectionName.textContent = `${preview.identity.subject} / ${preview.identity.example}`;
    shell.selectionSubject.textContent = preview.identity.subject;
    shell.selectionExample.textContent = preview.identity.example;
    shell.selectionSize.textContent = shellSize(preview);
    shell.selectionOrigin.textContent = origin(preview);
    shell.selectionSourceRow.hidden = preview.identity.kind !== "page";
    shell.selectionSource.textContent = preview.sourceFile;
    shell.selectionSource.dataset.sourcePath = preview.sourceFile;
    shell.selectionSource.disabled = model.render?.freshness === "stale";
    shell.selectionSource.setAttribute("aria-label", `Copy page source path ${preview.sourceFile}`);
    shell.selectionSource.title = shell.selectionSource.disabled
      ? "Copy is disabled while the preview is stale"
      : "Copy page source path";
    shell.selectionFromRow.hidden = preview.from === null || preview.from === "";
    shell.selectionFrom.textContent = preview.from ?? "";
    shell.selectionReplayRow.hidden = preview.replaySteps.length === 0;
    shell.selectionReplay.textContent = preview.replaySteps.join(" → ");
    renderSurfaceHierarchy(preview);
    renderWorkflow(preview);
    const status = preview.default ? ["Default"] : [];
    status.push(preview.inFlight > 0 ? `${preview.inFlight} in flight` : "Settled");
    shell.selectionStatus.textContent = status.join(" · ");
    renderData(preview);
    shell.selectionNoteBlock.hidden = !preview.note;
    shell.selectionNote.textContent = preview.note ?? "";
    renderPreviewDocumentation(
      shell.selectionDocumentation,
      model.authoring,
      preview,
      model.render?.freshness === "stale",
    );
    shell.selectionDocumentationBlock.hidden = shell.selectionDocumentation.hidden;
    shell.selectionInteractions.replaceChildren(...preview.interactions.map((interaction) => {
      const item = document.createElement("li");
      item.textContent = `${interaction.element} · ${interaction.event} → ${interaction.emit}`;
      item.title = JSON.stringify({
        nodeKey: interaction.nodeKey,
        kind: interaction.kind,
        scope: interaction.scope,
        payload: interaction.payload,
        carries: interaction.carries,
      });
      return item;
    }));
    shell.selectionNoInteractions.hidden = preview.interactions.length > 0;
    shell.selectionAnnouncement.textContent = focused
      ? `${preview.identity.subject} / ${preview.identity.example} focused; details updated.`
      : `${preview.identity.subject} / ${preview.identity.example} selected; details updated.`;
  };

  const clearSelection = (): void => {
    clearSelectionDom();
    selectedIdentity = null;
    // Deselecting arms the next selection's draw-in, even for the same frame.
    lastStructureDrawPreviewId = null;
    annotationOverlay.activatePreviewOccurrences(null);
    const previewId = focusedPreviewId();
    const focusedPreview = previewId ? model.previewById.get(previewId) : null;
    if (focusedPreview) {
      renderInspector(focusedPreview);
      return;
    }
    shell.inspectorOverview.hidden = false;
    shell.inspectorSelection.hidden = true;
    shell.focusSelectionButton.disabled = true;
    shell.clearSelectionButton.hidden = false;
    shell.selectionData.replaceChildren();
    shell.selectionSource.dataset.sourcePath = "";
    shell.selectionDocumentation.replaceChildren();
    shell.selectionDocumentation.hidden = true;
    shell.selectionDocumentationBlock.hidden = true;
    shell.selectionHierarchy.replaceChildren();
    shell.selectionHierarchyBlock.hidden = true;
    shell.selectionWorkflow.replaceChildren();
    shell.selectionWorkflowBlock.hidden = true;
    shell.selectionAnnouncement.textContent = "";
  };

  const selectPreview = (previewId: string, reveal = false): void => {
    if (mapMode) previewId = mapSelectionTargetId(previewId);
    const preview = model.previewById.get(previewId);
    const frame = model.frameById.get(previewId);
    if (!preview || !frame) return;
    clearSelectionDom();
    selectedIdentity = preview.identity;
    annotationOverlay.activatePreviewOccurrences(previewId);
    frame.classList.add("is-selected");
    frame.setAttribute("aria-pressed", "true");
    model.connectorLayer.classList.add("has-selection");
    for (const connector of model.connectors) {
      const active = connector.sourceId === previewId || connector.targetId === previewId;
      connector.element.classList.toggle("is-active", active);
      if (!active) continue;
      const relatedId = connector.sourceId === previewId
        ? connector.targetId
        : connector.sourceId;
      model.frameById.get(relatedId)?.classList.add("is-related");
    }
    if (mapMode) {
      // Map mode keeps every structural arrow up (re-armed by the clear pass
      // above) and spotlights the clicked definition: its arrows regain
      // their label pills and full strength (is-map-emphasized) while
      // unrelated ones fade back with their frames. Global-nav plumbing
      // stays uniform background chrome either way.
      const selectedNode = structureDefinitionNode(preview.identity);
      let relatedCount = 0;
      for (const connector of mapStructureConnectors) {
        const related = connector.sourceNode === selectedNode
          || connector.targetNode === selectedNode;
        connector.element.classList.toggle("is-map-dimmed", !related);
        connector.element.classList.toggle("is-map-emphasized", related);
        if (!related) continue;
        relatedCount += 1;
        const farId = connector.sourceNode === selectedNode
          ? connector.targetId
          : connector.sourceId;
        model.frameById.get(farId)?.classList.add("is-related");
      }
      shell.board.classList.toggle("is-spotlight", relatedCount > 0);
      lastStructureDrawPreviewId = previewId;
      finishPreviewSelection(previewId, preview, reveal);
      return;
    }
    // Structural connectors are selection-scoped: only the arrows entering
    // or leaving the selected preview's definition draw, anchored at the
    // frame the user actually clicked. Outgoing navigation fans down the
    // right edge, incoming arrives muted at the left edge, and presents
    // leave the bottom edge (or arrive at a selected surface's top edge).
    activeStructureConnectors = layoutStructureConnectors(
      visibleStructureConnectors(model.structureConnectors, preview.identity),
      { node: structureDefinitionNode(preview.identity), previewId },
    );
    // Active structural arrows lift the whole connector layer above the
    // preview rows so edge label pills and arrowheads never clip behind a
    // neighboring frame; without them the layer keeps its below-frame
    // stacking for replay connectors.
    model.connectorLayer.classList.toggle(
      "has-structure",
      activeStructureConnectors.length > 0,
    );
    // Spotlight: unrelated frames step back while structural arrows are up.
    shell.board.classList.toggle("is-spotlight", activeStructureConnectors.length > 0);
    if (
      activeStructureConnectors.length > 0
      && shouldReplayStructureDraw(lastStructureDrawPreviewId, previewId)
    ) {
      pendingStructureDraw = true;
    }
    lastStructureDrawPreviewId = previewId;
    for (const connector of activeStructureConnectors) {
      connector.element.classList.add("is-active");
      connector.element.classList.toggle(
        "is-incoming",
        connector.placement.direction === "incoming",
      );
      connector.element.dataset.direction = connector.placement.direction;
      connector.element.dataset.edge = connector.placement.side;
      connector.element.dataset.slot =
        `${connector.placement.slot + 1}/${connector.placement.slotCount}`;
      // The pill names the far endpoint relative to the selection: the same
      // connector reads `event → target` from its source frame and
      // `event ← source` from its target frame.
      const connectorLabel = connector.element
        .querySelector<SVGTextElement>(".workflow-connector-label");
      if (connectorLabel) {
        renderStructureConnectorLabel(
          connectorLabel,
          connector,
          connector.placement.direction,
        );
      }
      model.frameById.get(connector.placement.farId)?.classList.add("is-related");
    }
    finishPreviewSelection(previewId, preview, reveal);
  };

  function finishPreviewSelection(
    previewId: string,
    preview: EditorPreview,
    reveal: boolean,
  ): void {
    requestConnectors();
    const navigatorButton = Array.from(
      shell.navigatorResults.querySelectorAll<HTMLElement>("[data-preview-id]"),
    ).find((button) => button.dataset.previewId === previewId);
    if (navigatorButton) {
      navigatorButton.setAttribute("aria-pressed", "true");
      navigatorButton.setAttribute("aria-current", "true");
      navigatorButton.scrollIntoView({ block: "nearest" });
    }
    renderInspector(preview);
    if (reveal) revealPreviewFlow(previewId);
  }

  const focusPreview = (previewId: string): void => {
    const preview = model.previewById.get(previewId);
    if (!preview || !model.frameById.has(previewId)) return;
    // Focus is an example-board affordance: it isolates one preview, which
    // has no meaning on the map. Entering focus returns to Board first.
    if (mapMode) setMapMode(false);
    focusState = enterPreviewFocus(focusState, preview.identity, { x, y, scale });
    syncFocusPresentation();
    selectPreview(previewId, false);
    scheduleFocusFit();
  };

  const navigatePreview = (previewId: string, reveal = false): void => {
    if (focusState) {
      focusPreview(previewId);
      return;
    }
    selectPreview(previewId, reveal);
  };

  const leavePreviewFocus = (restoreDomFocus = true): void => {
    const previewId = focusedPreviewId();
    const returnTarget = previewId ? model.frameById.get(previewId) ?? null : null;
    const camera = exitPreviewFocus(focusState);
    if (!camera) return;
    focusState = null;
    cancelFocusFit();
    syncFocusPresentation();
    x = camera.x;
    y = camera.y;
    scale = camera.scale;
    applyCamera();
    const selectedId = previewIdForIdentity(model, selectedIdentity);
    if (selectedId) selectPreview(selectedId, false);
    else clearSelection();
    if (restoreDomFocus) (returnTarget ?? shell.viewport).focus({ preventScroll: true });
  };

  const applySearch = (): void => {
    const query = shell.navigatorSearch.value.trim().toLocaleLowerCase();
    let visibleGroups = 0;
    shell.navigatorResults.querySelectorAll<HTMLElement>("[data-navigator-group]").forEach((group) => {
      const groupMatches = group.dataset.search?.includes(query) ?? false;
      let visibleFrames = 0;
      group.querySelectorAll<HTMLElement>(".navigator-frame").forEach((button) => {
        const matches = !query || groupMatches || (button.dataset.search?.includes(query) ?? false);
        button.hidden = !matches;
        if (matches) visibleFrames += 1;
      });
      const visible = !query || groupMatches || visibleFrames > 0;
      group.hidden = !visible;
      if (visible) visibleGroups += 1;
    });
    shell.navigatorEmpty.hidden = visibleGroups > 0;
  };

  const renderOverview = (render: EditorRender | null): void => {
    const previews = render?.previews ?? [];
    shell.mapToggleButton.disabled = !render;
    shell.navigatorApplication.textContent = render?.application.name ?? "Uhura";
    shell.navigatorCount.textContent = `${render?.groups.length ?? 0} groups`;
    shell.overviewApplication.textContent = render?.application.name ?? "Uhura";
    shell.overviewFreshness.textContent = !render
      ? "No renderable revision yet"
      : render.freshness === "stale"
        ? `Stale preview · revision ${render.revision}`
        : `Current preview · revision ${render.revision}`;
    shell.overviewStats.replaceChildren(
      stat(document, "Previews", previews.length),
      stat(document, "Groups", render?.groups.length ?? 0),
      stat(document, "Defaults", previews.filter((preview) => preview.default).length),
      stat(document, "Derived", previews.filter((preview) => preview.derived).length),
      stat(document, "Pinned", previews.filter((preview) => preview.pinned).length),
      stat(document, "Flows", previews.filter((preview) => preview.from !== null).length),
      stat(document, "Assets", Object.keys(render?.assets ?? {}).length),
    );
    const callout = shell.overviewCallout.querySelector("p");
    if (callout) {
      callout.textContent = render?.freshness === "stale"
        ? "The current source has errors. These previews come from the last renderable saved revision."
        : "Save a .uhura file to rebuild these previews automatically.";
    }
  };

  const showStatus = (
    title: string,
    detail: string,
    tone: "neutral" | "warning" | "error",
    messages: string[] = [],
  ): void => {
    shell.status.dataset.tone = tone;
    shell.statusTitle.textContent = title;
    shell.statusDetail.textContent = detail;
    shell.statusDiagnostics.replaceChildren(...messages.slice(0, 8).map((message) => {
      const item = document.createElement("li");
      item.textContent = message;
      return item;
    }));
    shell.status.hidden = false;
    annotationOverlay.invalidate();
  };

  const showStateStatus = (nextState: EditorState): void => {
    const messages = diagnostics(nextState);
    if (!nextState.render) {
      showStatus(
        "No valid preview yet",
        `Source revision ${nextState.sourceRevision} cannot be rendered. Editor will recover after a valid save.`,
        "error",
        messages,
      );
    } else if (nextState.render.freshness === "stale") {
      showStatus(
        "Previewing the last valid version",
        `Source revision ${nextState.sourceRevision} has errors; preview revision ${nextState.render.revision} remains visible.`,
        "warning",
        messages,
      );
    } else if (messages.length > 0) {
      showStatus(
        "Preview updated with diagnostics",
        `Source revision ${nextState.sourceRevision} is current.`,
        "warning",
        messages,
      );
    } else {
      shell.status.hidden = true;
      shell.statusDiagnostics.replaceChildren();
      annotationOverlay.invalidate();
    }
  };

  const finishStateInstall = (nextState: EditorState): void => {
    const previousFocus = focusState;
    const retainedFocus = retainPreviewFocus(previousFocus, nextState);
    const restoreCamera = previousFocus && !retainedFocus
      ? exitPreviewFocus(previousFocus)
      : null;
    if (previousFocus) cancelFocusFit();
    focusState = retainedFocus;
    selectedIdentity = retainPreviewSelection(selectedIdentity, nextState);
    state = nextState;
    renderOverview(nextState.render);
    renderSourcePanel(
      shell.sourcePanel,
      model.authoring,
      nextState.render?.freshness === "stale",
      (targetId) => {
        setAnnotationLayerVisible(true);
        annotationOverlay.selectSourceTarget(targetId);
      },
    );
    applySearch();
    syncFocusPresentation();
    if (restoreCamera) {
      x = restoreCamera.x;
      y = restoreCamera.y;
      scale = restoreCamera.scale;
      applyCamera();
    }
    const selectedId = previewIdForIdentity(model, selectedIdentity);
    if (selectedId) selectPreview(selectedId, false);
    else clearSelection();
    showStateStatus(nextState);
    if (focusState) scheduleFocusFit();
  };

  const installModel = (nextState: EditorState, nextModel: PreparedEditorModel): void => {
    const previousModel = model;
    const previousBoard = shell.board;
    const keyboardFocusedFrame = closest<HTMLElement>(
      document.activeElement,
      ".editor-frame[data-preview-id]",
    );
    const keyboardFocusedIdentity = keyboardFocusedFrame?.dataset.previewId
      ? previousModel.previewById.get(keyboardFocusedFrame.dataset.previewId)?.identity ?? null
      : null;
    nextModel.board.style.transform = previousBoard.style.transform;
    nextModel.board.style.setProperty("--selection-stroke", `${2 / scale}px`);
    nextModel.board.style.setProperty("--selection-offset", `${4 / scale}px`);
    const prospectiveResources = new Map(nextModel.resourcesByPreviewId);
    for (const id of nextModel.reusableRealizationIds) {
      const previousResources = previousModel.resourcesByPreviewId.get(id);
      if (previousResources) prospectiveResources.set(id, previousResources);
    }
    validateAnnotationRealizations({
      render: nextModel.render,
      authoring: nextModel.authoring,
      resourcesByPreviewId: prospectiveResources,
    });
    reconcilePreparedEditorModel(previousModel, nextModel);
    nextModel.board.style.setProperty("--connector-stroke", `${1.5 / scale}px`);
    previousBoard.replaceWith(nextModel.board);
    shell.board = nextModel.board;
    shell.navigatorResults.replaceChildren(nextModel.navigator);
    model = nextModel;
    setAnnotationConnectorsHidden(model.connectorLayer, !annotationLayerVisible);
    disposePreparedEditorModel(previousModel);
    // A live update replaces the board wholesale, dropping the previous map
    // DOM with it. Map mode re-derives its layout over the fresh model before
    // the selection pipeline below re-arms the arrows; losing the render
    // (invalid save) falls back to the example board's empty state.
    if (mapMode) {
      if (model.render) {
        applyMapLayout();
      } else {
        mapMode = false;
        mapReturnCamera = null;
        shell.mapToggleButton.setAttribute("aria-pressed", "false");
        teardownMapDom();
      }
    }
    annotationOverlay.install({
      render: nextModel.render,
      authoring: nextModel.authoring,
      resourcesByPreviewId: nextModel.resourcesByPreviewId,
    });
    const refreshOverlays = (): void => {
      annotationOverlay.invalidate();
      requestConnectors();
    };
    watchPreparedEditorModel(nextModel, window, refreshOverlays);
    void document.fonts?.ready.then(() => {
      if (model === nextModel) refreshOverlays();
    });
    finishStateInstall(nextState);
    const reboundKeyboardFocusId = previewIdForIdentity(model, keyboardFocusedIdentity);
    if (reboundKeyboardFocusId) {
      model.frameById.get(reboundKeyboardFocusId)?.focus({ preventScroll: true });
    }
    requestRulers();
    annotationOverlay.invalidate();
    requestConnectors();
  };

  const scheduleRetry = (
    token: EditorFetchToken,
    expectedRevision: number | null,
    delay: number,
  ): void => {
    if (retryTimer !== undefined) window.clearTimeout(retryTimer);
    retryTimer = window.setTimeout(() => {
      retryTimer = undefined;
      const retry = updates.retry(token, expectedRevision);
      if (retry) void loadState(retry);
    }, delay);
  };

  const loadState = async (token: EditorFetchToken): Promise<void> => {
    let prepared: PreparedEditorModel | null = null;
    try {
      const response = await window.fetch(EDITOR_STATE_PATH, {
        headers: { Accept: "application/json" },
        cache: "no-store",
      });
      if (!response.ok) throw new Error(`Editor state request failed (${response.status})`);
      const nextState = decodeEditorState(await response.json());
      const decision = updates.consider(token, nextState);
      if (decision.kind === "ignored") return;
      if (decision.kind === "behind") {
        scheduleRetry(token, decision.expectedRevision, 50);
        return;
      }
      const nextModel = prepareEditorModel(document, decision.state.render, model);
      prepared = nextModel;
      const committed = updates.commit(
        token,
        decision.state,
        () => installModel(decision.state, nextModel),
      );
      if (!committed) {
        disposePreparedEditorModel(nextModel);
        prepared = null;
      }
    } catch (error) {
      if (prepared && model !== prepared) disposePreparedEditorModel(prepared);
      if (destroyed || !updates.isCurrent(token)) return;
      showStatus(
        "Editor state unavailable",
        error instanceof Error ? error.message : "Could not load the Editor state.",
        "error",
      );
      scheduleRetry(token, token.expectedRevision, 750);
    }
  };

  const setUiVisible = (visible: boolean, persist = true): void => {
    shell.shell.classList.toggle("ui-hidden", !visible);
    if (persist) storeUiVisible(window.localStorage, visible);
    requestRulers();
    annotationOverlay.invalidate();
  };
  const setSourceDrawer = (open: boolean, focusClose = false): void => {
    shell.sourceDrawer.hidden = !open;
    shell.sourceDrawerButton.setAttribute("aria-expanded", String(open));
    if (open && focusClose) shell.sourceDrawerClose.focus({ preventScroll: true });
    annotationOverlay.invalidate();
  };
  setUiVisible(storedUiVisible(window.localStorage), false);

  listen(shell.cursorButton, "click", () => selectTool("cursor"));
  listen(shell.handButton, "click", () => selectTool("hand"));
  listen(shell.zoomOutButton, "click", () => zoomAt(scale / ZOOM_STEP, viewportCenter()));
  listen(shell.zoomInButton, "click", () => zoomAt(scale * ZOOM_STEP, viewportCenter()));
  listen(shell.zoomOutput, "click", () => zoomAt(1, viewportCenter()));
  listen(shell.focusSelectionButton, "click", () => {
    if (focusState) {
      scheduleFocusFit();
      return;
    }
    const selectedId = previewIdForIdentity(model, selectedIdentity);
    if (selectedId) revealPreviewFlow(selectedId);
  });
  listen(shell.mapToggleButton, "click", () => setMapMode(!mapMode));
  listen(shell.navToggleButton, "click", () => setMapNavVisible(!mapNavVisible));
  listen(shell.mapResetButton, "click", resetMapLayout);
  listen(shell.exitFocusButton, "click", () => leavePreviewFocus());
  listen(shell.sourceDrawerButton, "click", () => {
    setSourceDrawer(shell.sourceDrawer.hasAttribute("hidden"), true);
  });
  listen(shell.sourceDrawerClose, "click", () => {
    setSourceDrawer(false);
    shell.sourceDrawerButton.focus({ preventScroll: true });
  });
  listen(shell.clearSelectionButton, "click", clearSelection);
  listen(shell.selectionSource, "click", () => {
    const path = shell.selectionSource.dataset.sourcePath;
    if (!path || shell.selectionSource.disabled) return;
    void window.navigator.clipboard?.writeText(path);
  });
  listen(shell.statusDismiss, "click", () => {
    shell.status.hidden = true;
    annotationOverlay.invalidate();
  });
  listen(shell.tools, "pointerdown", (event) => event.stopPropagation());

  listen(shell.navigatorResults, "click", (event) => {
    const frameButton = closest<HTMLElement>(event.target, ".navigator-frame[data-preview-id]");
    if (frameButton?.dataset.previewId) {
      navigatePreview(frameButton.dataset.previewId, true);
      return;
    }
    const groupButton = closest<HTMLElement>(event.target, ".navigator-row[data-group-id]");
    const groupId = groupButton?.dataset.groupId;
    if (!groupId) return;
    const group = state?.render?.groups.find((candidate) => candidate.id === groupId);
    const first = group?.previews[0];
    if (!first) return;
    if (focusState) focusPreview(first);
    else revealElement(model.frameById.get(first) ?? null);
  });
  listen(shell.navigatorSearch, "input", applySearch);
  listen(shell.navigatorSearch, "keydown", (rawEvent) => {
    const event = rawEvent as KeyboardEvent;
    if (event.key !== "Escape" || !shell.navigatorSearch.value) return;
    shell.navigatorSearch.value = "";
    applySearch();
    event.preventDefault();
    event.stopPropagation();
  });

  listen(shell.viewport, "click", (event) => {
    if (effectiveTool() !== "cursor" || performance.now() < suppressClickUntil) return;
    const frame = closest<HTMLElement>(event.target, ".editor-frame[data-preview-id]");
    if (frame?.dataset.previewId) selectPreview(frame.dataset.previewId, false);
  });
  listen(shell.viewport, "dblclick", (event) => {
    if (effectiveTool() !== "cursor" || performance.now() < suppressClickUntil) return;
    const frame = closest<HTMLElement>(event.target, ".editor-frame[data-preview-id]");
    if (!frame?.dataset.previewId) return;
    focusPreview(frame.dataset.previewId);
    event.preventDefault();
  });
  // Structural connector hover: only the drawn stroke and the label pill are
  // pointer-interactive (CSS pointer-events), so delegation on the viewport
  // fires exactly when the pointer is over one of those.
  listen(shell.viewport, "pointerover", (event) => {
    const group = closest<SVGGElement>(event.target, ".structure-connector.is-active");
    setStructureHover(
      group
        ? activeStructureConnectors.find((connector) => connector.element === group) ?? null
        : null,
    );
  });
  listen(shell.viewport, "pointerout", (rawEvent) => {
    const event = rawEvent as PointerEvent;
    if (!hoveredStructureConnector) return;
    const next = closest<SVGGElement>(event.relatedTarget, ".structure-connector.is-active");
    if (next !== hoveredStructureConnector.element) setStructureHover(null);
  });
  // Once the draw-in sweep completes, drop the class so present connectors
  // recover their dashed stroke and the next replay starts from a clean slate.
  listen(shell.viewport, "animationend", (rawEvent) => {
    const event = rawEvent as AnimationEvent;
    if (event.animationName !== "structure-draw") return;
    closest<SVGGElement>(event.target, ".structure-connector")?.classList.remove("is-drawing");
  });
  listen(shell.viewport, "keydown", (rawEvent) => {
    const event = rawEvent as KeyboardEvent;
    const frame = closest<HTMLElement>(event.target, ".editor-frame[data-preview-id]");
    if (
      !frame?.dataset.previewId
      || event.target !== frame
      || (event.key !== "Enter" && event.key !== " ")
    ) return;
    if (event.key === "Enter") focusPreview(frame.dataset.previewId);
    else selectPreview(frame.dataset.previewId, false);
    event.preventDefault();
  });

  const twoTouches = (): Point[] => Array.from(touches.values()).slice(0, 2);
  const beginPinch = (): void => {
    const [a, b] = twoTouches();
    if (!a || !b) return;
    finishPan();
    const midpoint = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
    const distance = Math.hypot(b.x - a.x, b.y - a.y);
    if (distance === 0) return;
    suppressClickUntil = performance.now() + 250;
    pinch = {
      distance,
      scale,
      worldX: (midpoint.x - x) / scale,
      worldY: (midpoint.y - y) / scale,
    };
  };
  const updatePinch = (): void => {
    const [a, b] = twoTouches();
    if (!pinch || !a || !b) return;
    const distance = Math.hypot(b.x - a.x, b.y - a.y);
    const midpoint = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
    const nextScale = clampScale(pinch.scale * distance / pinch.distance);
    x = midpoint.x - pinch.worldX * nextScale;
    y = midpoint.y - pinch.worldY * nextScale;
    scale = nextScale;
    applyCamera();
  };

  listen(shell.viewport, "pointerdown", (rawEvent) => {
    const event = rawEvent as PointerEvent;
    const frame = closest<HTMLElement>(event.target, ".editor-frame[data-preview-id]");
    const commentControl = closest<HTMLElement>(
      event.target,
      ".annotation-marker, .annotation-card",
    );
    const canvasTool = closest<HTMLElement>(event.target, ".canvas-tools");
    // A press on a structural connector's stroke or pill must not read as a
    // press on empty stage — that would deselect and vanish the connector.
    const structureConnector = closest<Element>(event.target, ".structure-connector");
    if (event.button === 0 && !frame && !commentControl && !canvasTool && !structureConnector) {
      clearSelection();
      annotationOverlay.dismissCards();
      setSourceDrawer(false);
    }
    // In map mode a primary press on a node arms a drag; the release decides
    // between drag (past the threshold) and the ordinary click-to-select.
    if (beginMapDrag(event)) return;
    if (event.pointerType === "touch") {
      const point = localPoint(event.clientX, event.clientY);
      touches.set(event.pointerId, point);
      shell.viewport.setPointerCapture(event.pointerId);
      if (touches.size === 2) beginPinch();
      else if (touches.size === 1 && effectiveTool() === "hand") beginPan(event.pointerId, point);
      return;
    }
    const shouldPan = event.button === 1 || (event.button === 0 && effectiveTool() === "hand");
    if (!shouldPan) return;
    beginPan(event.pointerId, localPoint(event.clientX, event.clientY));
    shell.viewport.setPointerCapture(event.pointerId);
    event.preventDefault();
  });
  listen(shell.viewport, "pointermove", (rawEvent) => {
    const event = rawEvent as PointerEvent;
    if (updateMapDrag(event)) return;
    if (event.pointerType === "touch" && touches.has(event.pointerId)) {
      const point = localPoint(event.clientX, event.clientY);
      touches.set(event.pointerId, point);
      if (!pinch && touches.size >= 2) beginPinch();
      if (pinch) updatePinch();
      else updatePan(event.pointerId, point);
      return;
    }
    updatePan(event.pointerId, localPoint(event.clientX, event.clientY));
  });
  const finishPointer = (rawEvent: Event): void => {
    const event = rawEvent as PointerEvent;
    finishMapDrag(event.pointerId);
    finishPan(event.pointerId);
    if (!touches.delete(event.pointerId)) return;
    pinch = null;
    if (touches.size >= 2) beginPinch();
    else if (touches.size === 1 && effectiveTool() === "hand") {
      const remaining = touches.entries().next().value;
      if (remaining) beginPan(remaining[0], remaining[1]);
    }
  };
  listen(shell.viewport, "pointerup", finishPointer);
  listen(shell.viewport, "pointercancel", finishPointer);
  listen(shell.viewport, "lostpointercapture", finishPointer);
  listen(shell.viewport, "wheel", (rawEvent) => {
    const event = rawEvent as WheelEvent;
    event.preventDefault();
    const unit = event.deltaMode === WheelEvent.DOM_DELTA_LINE
      ? 16
      : event.deltaMode === WheelEvent.DOM_DELTA_PAGE
        ? shell.viewport.clientHeight
        : 1;
    if (event.ctrlKey || event.metaKey) {
      const exponent = Math.min(Math.max(
        -event.deltaY * unit * WHEEL_ZOOM_SENSITIVITY,
        -0.25,
      ), 0.25);
      zoomAt(scale * Math.exp(exponent), localPoint(event.clientX, event.clientY));
      return;
    }
    if (event.shiftKey && event.deltaX === 0) x -= event.deltaY * unit;
    else {
      x -= event.deltaX * unit;
      y -= event.deltaY * unit;
    }
    applyCamera();
  }, { passive: false });

  const keyboardTarget = (event: KeyboardEvent): EventTarget | null =>
    event.composedPath().find((target) => target instanceof Element) ?? event.target;
  const isTextEntry = (target: EventTarget | null): boolean => {
    if (!(target instanceof Element)) return false;
    if (target.closest("input, select, textarea")) return true;
    const editable = target.closest("[contenteditable]");
    return editable !== null
      && editable.getAttribute("contenteditable")?.toLocaleLowerCase() !== "false";
  };
  const isInteractiveControl = (target: EventTarget | null): boolean =>
    target instanceof Element && Boolean(target.closest("button, a"));
  listen(window, "keydown", (rawEvent) => {
    const event = rawEvent as KeyboardEvent;
    const togglesUi = (event.metaKey || event.ctrlKey)
      && !event.altKey
      && !event.shiftKey
      && event.code === "Backslash";
    if (togglesUi) {
      if (!event.repeat) setUiVisible(shell.shell.classList.contains("ui-hidden"));
      event.preventDefault();
      return;
    }
    const target = keyboardTarget(event);
    const sourceAction = sourceShortcutAction(event, isTextEntry(target));
    if (sourceAction === "open-source") {
      setSourceDrawer(true);
      event.preventDefault();
      return;
    }
    if (sourceAction === "toggle-annotation-layer") {
      setAnnotationLayerVisible(!annotationLayerVisible);
      event.preventDefault();
      return;
    }
    if (
      !event.repeat
      && event.key === "Escape"
      && (!shell.sourceDrawer.hidden || focusState)
    ) {
      if (!shell.sourceDrawer.hidden) setSourceDrawer(false);
      else leavePreviewFocus();
      event.preventDefault();
      return;
    }
    if (
      isTextEntry(target)
      || event.metaKey
      || event.ctrlKey
      || event.altKey
      || (event.code === "Space" && isInteractiveControl(target))
    ) return;
    if (event.code === "Space") {
      spaceHeld = true;
      renderTools();
      event.preventDefault();
    } else if (!event.repeat && event.code === "KeyH") selectTool("hand");
    else if (!event.repeat && event.code === "KeyV") selectTool("cursor");
    else if (!event.repeat && (event.key === "+" || event.key === "=")) {
      zoomAt(scale * ZOOM_STEP, viewportCenter());
    } else if (!event.repeat && event.key === "-") {
      zoomAt(scale / ZOOM_STEP, viewportCenter());
    } else if (!event.repeat && event.key === "Escape") {
      clearSelection();
    }
  });
  listen(window, "keyup", (rawEvent) => {
    const event = rawEvent as KeyboardEvent;
    if (event.code !== "Space") return;
    spaceHeld = false;
    renderTools();
  });
  const resetPointers = (): void => {
    spaceHeld = false;
    cancelMapDrag();
    finishPan();
    touches.clear();
    pinch = null;
    renderTools();
  };
  listen(window, "blur", resetPointers);
  listen(document, "visibilitychange", () => {
    if (document.hidden) resetPointers();
  });

  let resizeObserver: ResizeObserver | null = null;
  if (window.ResizeObserver) {
    resizeObserver = new window.ResizeObserver(() => {
      requestRulers();
      annotationOverlay.invalidate();
      scheduleFocusFit();
      requestConnectors();
    });
    resizeObserver.observe(shell.viewport);
  } else {
    listen(window, "resize", () => {
      requestRulers();
      annotationOverlay.invalidate();
      scheduleFocusFit();
      requestConnectors();
    });
  }

  const events = new window.EventSource(EDITOR_EVENTS_PATH);
  listen(events, "open", () => {
    showStatus("Refreshing previews", "Connected to the Editor host…", "neutral");
    void loadState(updates.opened());
  });
  listen(events, "error", () => {
    showStatus(
      "Live preview disconnected",
      "The last loaded state remains visible while Editor reconnects…",
      "warning",
    );
  });
  listen(events, "message", (rawEvent) => {
    const event = rawEvent as MessageEvent<string>;
    try {
      const revision = decodeEditorRevisionEvent(JSON.parse(event.data));
      const token = updates.announced(revision);
      if (token) void loadState(token);
    } catch (error) {
      console.warn("ignored invalid Uhura Editor event", error);
    }
  });

  // Initial load does not wait for the SSE handshake. The mandatory `open`
  // fetch supersedes this request if the connection establishes first.
  void loadState(updates.opened());
  renderTools();
  applyCamera();

  return (): void => {
    if (destroyed) return;
    destroyed = true;
    events.close();
    annotationOverlay.dispose();
    disposePreparedEditorModel(model);
    resizeObserver?.disconnect();
    focusFrameObserver?.disconnect();
    if (retryTimer !== undefined) window.clearTimeout(retryTimer);
    if (rulerFrame) window.cancelAnimationFrame(rulerFrame);
    if (focusFitFrame) window.cancelAnimationFrame(focusFitFrame);
    if (connectorFrame) window.cancelAnimationFrame(connectorFrame);
    for (const dispose of disposers.splice(0)) dispose();
    root.replaceChildren();
  };
};
