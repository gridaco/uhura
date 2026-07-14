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

export interface StepResult {
  c: ProviderMsg[];
  i: Intent[];
  g: { code: string; rule: string; message: string }[];
  t: Record<string, unknown>;
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
  restart(): void;
  setActor(actor: string): void;
  setProvider(provider: ProviderMode): void;
  session: unknown;
  driver: Driver | null;
  steps: Record<string, unknown>[];
  ticks: unknown;
}
