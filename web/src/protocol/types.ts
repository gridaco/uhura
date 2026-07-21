// The frozen browser ABI shapes mirrored from the Rust protocol. A structural
// change here requires the corresponding protocol/version and ABI test change.

import type { HostInspection } from "../protocol/host-inspection.js";
import type {
  Receipt,
  RuntimeSnapshot,
} from "../protocol/machine.js";

/** The board-facing slice of one `uhura-interaction-graph/0` node. */
export interface InteractionGraphNode {
  id: string;
  kind: "page" | "surface" | "command" | "dynamic";
  label: string;
}

/** The board-facing slice of one `uhura-interaction-graph/0` edge. */
export interface InteractionGraphEdge {
  kind:
    | "navigate"
    | "navigate-back"
    | "present"
    | "dismiss"
    | "state-change"
    | "send-command"
    | "receive-outcome";
  from: string;
  to: string;
  event: string;
}

/**
 * The app's static interaction structure, projected by the native check.
 * The editor only reads the fields it draws; extra native fields (guards,
 * commands, source spans) are deliberately not mirrored here.
 */
export interface InteractionGraph {
  protocol: "uhura-interaction-graph/0";
  nodes: InteractionGraphNode[];
  edges: InteractionGraphEdge[];
}

export type SystemStatus = "starting" | "ready" | "error";

export interface SystemActor {
  id: string;
  username: string;
  label: string;
}

export interface SystemState {
  status: SystemStatus;
  hasProvider: boolean;
  actor: string | null;
  actors: SystemActor[];
  canSwitchActor: boolean;
  error?: string;
}

export interface SystemInfo {
  hasProvider?: boolean;
  actor?: string | null;
  actors?: SystemActor[];
}

export interface ProviderHost {
  /** Aborts when the Play route that owns this provider is retired. */
  readonly signal: AbortSignal;
  pickFile(options?: { accept?: string }): Promise<File | null>;
}

export interface RuntimeInspectionArtifacts {
  readonly generation: number;
  readonly deployment: HostInspection;
}

export interface RuntimeInspectedStep {
  readonly receipt: Receipt;
  readonly snapshot: RuntimeSnapshot;
}

export interface RuntimeInspectionState {
  readonly protocol: "uhura-runtime-inspection-state/1";
  readonly disposed: boolean;
  readonly historyLimit: number;
  readonly artifacts: RuntimeInspectionArtifacts | null;
  readonly latest: RuntimeInspectedStep | null;
  readonly history: readonly RuntimeInspectedStep[];
  readonly evictedSteps: number;
}

export type RuntimeInspectionListener = (state: RuntimeInspectionState) => void;

export interface RuntimeInspectionHandle {
  readonly state: RuntimeInspectionState;
  subscribe(listener: RuntimeInspectionListener): () => void;
}

export interface DevEvent {
  generation: number;
  ok: boolean;
  diagnostics?: Record<string, unknown>;
}

export interface RuntimeHandle {
  readonly system: SystemState;
  readonly inspection: RuntimeInspectionHandle;
  restart(): void;
  setActor(actor: string): void;
  session: unknown;
  provider: unknown | null;
  readonly steps: readonly Receipt[];
}
