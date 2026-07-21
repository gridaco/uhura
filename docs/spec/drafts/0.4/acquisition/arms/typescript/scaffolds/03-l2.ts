export type WorkId = string;

export type WorkPhase =
  | { readonly type: "queued" }
  | {
      readonly type: "running";
      readonly attempt: bigint;
      readonly progress: number;
    }
  | { readonly type: "succeeded" }
  | { readonly type: "failed" }
  | { readonly type: "cancelled" };

export interface Work {
  readonly started: bigint;
  readonly phase: WorkPhase;
}

export interface ProgressState {
  readonly work: ReadonlyMap<WorkId, Work>;
}

export interface ProgressInput {
  readonly type: "progress";
  readonly work: WorkId;
  readonly attempt: bigint;
  readonly value: number;
}

export type ProgressClassification =
  | "accepted"
  | "duplicate"
  | "stale"
  | "invalid";

export interface ProgressStep {
  readonly state: ProgressState;
  readonly classification: ProgressClassification;
  readonly commands: readonly [];
}

export function stepProgress(
  state: ProgressState,
  input: ProgressInput,
): ProgressStep {
  // TRIAL-TODO: implement the complete declared precedence and update only
  // accepted progress.
  throw new Error("TRIAL-TODO");
}
