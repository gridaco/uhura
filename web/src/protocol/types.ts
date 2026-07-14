// The frozen browser ABI shapes mirrored from the Rust protocol. A structural
// change here requires the corresponding protocol/version and ABI test change.

export interface Descriptor {
  kind: "input" | "observe";
  event: string;
  emit: string;
  scope: string;
  payload: unknown;
  carries?: Record<string, "text" | "bool" | "int">;
}

export interface VNode {
  key: string;
  element: string;
  class?: string;
  props: Record<string, VValue>;
  children?: VNode[];
  on?: Descriptor[];
}

export interface AssetValue {
  t: "image";
  asset: string;
}

export type VValue =
  | boolean
  | number
  | string
  | { t: "plain"; v: string }
  | AssetValue;

export interface VideoProps {
  src: AssetValue;
  poster?: AssetValue;
  label: string | { t: "plain"; v: string };
  autoplay?: boolean;
  muted?: boolean;
  loop?: boolean;
  controls?: boolean;
  playsinline?: boolean;
}

export interface SurfaceView {
  key: string;
  definition: string;
  modality: string;
  "restore-focus"?: string;
  dismiss: Descriptor;
  root: VNode;
}

export interface Snapshot {
  protocol: "uhura-view/0";
  revision: number;
  page: { route: string; root: VNode };
  surfaces: SurfaceView[];
}

export interface ProviderMsg {
  kind: "command" | "projection" | "projection-failed" | "outcome";
  port?: string;
  command?: string;
  correlation?: string;
  payload?: unknown;
  projection?: string;
  key?: unknown;
  revision?: number;
  value?: unknown;
  reason?: string;
  outcome?: unknown;
  updates?: unknown[];
}

export interface Driver {
  deliver(commandJson: string): void;
  tick(): string[];
  idle(): boolean;
}

export type PlayProvider =
  | { kind: "fixture" }
  | { kind: "module"; module: string; config: Record<string, string> };

export interface PlayConfig {
  provider: PlayProvider;
  allow_fixture?: boolean;
}

export type SystemStatus = "starting" | "ready" | "error";
export type ProviderMode = "remote" | "fixture";

export interface SystemActor {
  id: string;
  username: string;
  label: string;
}

export interface SystemState {
  status: SystemStatus;
  provider: ProviderMode | null;
  providers: ProviderMode[];
  actor: string | null;
  actors: SystemActor[];
  canSwitchActor: boolean;
  error?: string;
}

export interface SystemInfo {
  provider?: ProviderMode | null;
  providers?: ProviderMode[];
  actor?: string | null;
  actors?: SystemActor[];
}

export interface RemoteSystemInfo {
  actor: string | null;
  actors: SystemActor[];
}

export interface ProviderHost {
  /** Aborts when the Play route that owns this provider is retired. */
  readonly signal: AbortSignal;
  pickFile(options?: { accept?: string }): Promise<File | null>;
}

export interface RemoteDriver extends Driver {
  dispose(): void;
  assembleBoot(): Promise<string>;
  resolveAsset?(assetRef: string): Promise<string>;
  systemInfo?(): RemoteSystemInfo;
}

export type Intent =
  | { intent: "history-push"; route: string; params: Record<string, unknown> }
  | { intent: "history-replace"; route: string; params: Record<string, unknown> }
  | { intent: "history-back" }
  | { intent: "focus-restore"; "key-path": string };

export interface RuntimeDiagnostic {
  code: string;
  rule: string;
  message: string;
}

export type TraceApplyNote =
  | {
      apply: "applied";
      projection: string;
      key?: unknown;
      revision: number;
    }
  | {
      apply: "dropped-stale";
      projection: string;
      key?: unknown;
      revision: number;
      current: number;
    }
  | {
      apply: "failed";
      projection: string;
      key?: unknown;
      reason: string;
    };

export interface TraceGuardNote {
  handler: number;
  guard: "satisfied" | "unsatisfied" | "not-ready";
}

export interface TraceWrite {
  field: string;
  key?: string;
  value: unknown;
}

export interface TraceDispatch {
  scope: string;
  definition: string;
  on: string;
  guards: TraceGuardNote[];
  selected: number | null;
  writes?: TraceWrite[];
  aborted?: "projection-not-ready";
}

export type TraceStructural =
  | { op: "init"; route: string; serial: number }
  | { op: "already-open"; surface: string }
  | { op: "open-surface"; surface: string; opener: string }
  | { op: "navigate"; route: string; serial: number }
  | { op: "replace"; from: string; route: string; serial: number }
  | { op: "nav-underflow" }
  | { op: "back"; popped: string; to: string | null }
  | { op: "dismiss"; surface: string; top: boolean }
  | { op: "force-close"; surface: string };

export type TraceDropReason =
  | "stale-scope"
  | "occluded"
  | "ineligible"
  | "stale-outcome"
  | "unknown-correlation"
  | "no-handler"
  | "projection-not-ready";

/** The canonical, tooling-facing trace for one successful runtime step. */
export interface StepTrace extends Record<string, unknown> {
  event: Record<string, unknown>;
  applies?: TraceApplyNote[];
  dispatch?: TraceDispatch;
  drop?: TraceDropReason;
  "drop-detail"?: string;
  reserved?: { event: "dismiss"; scope: string };
  structural?: TraceStructural[];
  c?: ProviderMsg[];
  i?: Intent[];
  g?: RuntimeDiagnostic[];
  "u-hash": string;
  "v-hash": string;
  /** Present only in expanded trace presentation, not normal Play steps. */
  v?: Snapshot;
}

export interface InspectSourceSpan {
  file: string;
  /** Inclusive UTF-8 byte offset; this is not a JavaScript string index. */
  start: number;
  /** Exclusive UTF-8 byte offset; this is not a JavaScript string index. */
  end: number;
}

export type InspectProgramNode =
  | {
      id: string;
      kind: "definition";
      "definition-kind": "page" | "component" | "surface";
      name: string;
      entry?: boolean;
    }
  | {
      id: string;
      kind: "handler";
      definition: string;
      index: number;
      on: string;
      guarded: boolean;
      effects: ("dismiss" | "back")[];
    }
  | {
      id: string;
      kind: "event";
      definition: string;
      name: string;
      "event-kind": "semantic" | "outcome";
      command?: string;
      outcome?: "ok" | "err";
    }
  | {
      id: string;
      kind: "state";
      definition: string;
      name: string;
      initial: unknown;
    }
  | {
      id: string;
      kind: "projection";
      name: string;
      port: string;
      boot: boolean;
      keyed: boolean;
    }
  | {
      id: string;
      kind: "command";
      name: string;
      /** Outcome-only handlers can name a command before a Send supplies its port. */
      port?: string;
    };

export type InspectProgramEdge =
  | {
      kind: "handles" | "guard-reads" | "body-reads" | "settles";
      from: string;
      to: string;
    }
  | {
      kind: "writes" | "sends" | "opens";
      from: string;
      to: string;
      order: number;
    }
  | {
      kind: "navigates";
      from: string;
      to: string;
      order: number;
      mode: "push" | "replace";
    };

/** Static behavior topology served with the same generation as the Play IR. */
export interface InspectProgram {
  protocol: "uhura-inspect/0";
  kind: "program";
  "span-offset-encoding": "utf-8-bytes";
  ir: {
    protocol: "uhura-ir/0";
    hash: string;
    app: string;
    entry: string;
  };
  nodes: InspectProgramNode[];
  edges: InspectProgramEdge[];
  spans: Record<string, InspectSourceSpan>;
}

export interface InspectNavEntry {
  serial: number;
  route: string;
  params: Record<string, unknown>;
  state: Record<string, unknown>;
}

export interface InspectSurfaceState {
  serial: number;
  definition: string;
  props: Record<string, unknown>;
  state: Record<string, unknown>;
  opener: string;
  "restore-focus"?: string;
}

export interface InspectPendingCommand {
  port: string;
  command: string;
  payload: unknown;
  origin: string;
}

export interface InspectUiState {
  rev: number;
  nav: InspectNavEntry[];
  surfaces: InspectSurfaceState[];
  pending: Record<string, InspectPendingCommand>;
  counters: {
    tag: number;
    "page-serial": number;
    "surface-serial": number;
  };
}

export interface InspectProjectionSnapshot {
  projection: string;
  key: unknown | null;
  revision: number;
  value: unknown;
}

export interface InspectProjectionFailure {
  projection: string;
  key: unknown | null;
  reason: string;
}

export interface InspectViewMetadata {
  revision: number;
  route: string;
  "surface-count": number;
  "v-hash": string;
}

/** Complete committed U/X state, separate from the frozen step-result ABI. */
export interface InspectSnapshot {
  protocol: "uhura-inspect/0";
  kind: "snapshot";
  "ir-hash": string;
  revision: number;
  "configuration-hash": string;
  "u-hash": string;
  "x-hash": string;
  u: InspectUiState;
  x: {
    snapshots: InspectProjectionSnapshot[];
    failed: InspectProjectionFailure[];
  };
  view: InspectViewMetadata | null;
  "pending-applies": TraceApplyNote[];
}

export type DeepReadonly<T> =
  T extends (...args: never[]) => unknown
    ? T
    : T extends readonly (infer Item)[]
      ? readonly DeepReadonly<Item>[]
      : T extends object
        ? { readonly [Key in keyof T]: DeepReadonly<T[Key]> }
        : T;

export interface InspectionArtifacts {
  readonly generation: number;
  readonly program: DeepReadonly<InspectProgram>;
}

export interface InspectedStep {
  readonly trace: DeepReadonly<StepTrace>;
  readonly inspection: DeepReadonly<InspectSnapshot>;
}

export interface InspectionState {
  readonly disposed: boolean;
  readonly historyLimit: number;
  readonly artifacts: InspectionArtifacts | null;
  readonly latest: InspectedStep | null;
  readonly history: readonly InspectedStep[];
  readonly evictedSteps: number;
}

export type InspectionListener = (state: InspectionState) => void;

export interface InspectionHandle {
  readonly state: InspectionState;
  subscribe(listener: InspectionListener): () => void;
}

export interface StepResult {
  c: ProviderMsg[];
  i: Intent[];
  g: RuntimeDiagnostic[];
  t: StepTrace;
  v: Snapshot;
}

export interface DevEvent {
  generation: number;
  ok: boolean;
  diagnostics?: Record<string, unknown>;
}

export interface ProviderModule {
  createDriver?: (
    config: Record<string, string>,
    host: ProviderHost,
  ) => RemoteDriver;
}

export interface RuntimeHandle {
  readonly system: SystemState;
  readonly inspection: InspectionHandle;
  restart(): void;
  setActor(actor: string): void;
  setProvider(provider: ProviderMode): void;
  session: unknown;
  driver: Driver | null;
  readonly steps: readonly DeepReadonly<StepTrace>[];
  ticks: unknown;
}
