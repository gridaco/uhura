// Framework-neutral, read-only publication of Uhura's inspection protocol.
// The store retains a bounded trace/state window and deliberately omits full
// view snapshots: Play already owns the current V, while Session.inspect()
// supplies the machine/projection state a behavior visualizer needs.

import type {
  InspectSnapshot,
  InspectedStep,
  InspectionArtifacts,
  InspectionHandle,
  InspectionListener,
  InspectionState,
  StepResult,
} from "../protocol/types.js";

export const DEFAULT_INSPECTION_HISTORY_LIMIT = 128;

export interface InspectionStoreOptions {
  historyLimit?: number;
  onListenerError?: (error: unknown) => void;
}

export interface InspectionStore {
  readonly handle: InspectionHandle;
  /** Installs the one generation-coherent program artifact for this mount. */
  installArtifacts(artifacts: InspectionArtifacts): boolean;
  /** Correlates and publishes one successful dispatch with committed U/X. */
  record(result: StepResult, inspection: InspectSnapshot): boolean;
  /** Idempotently clears retained developer data and retires subscriptions. */
  dispose(): void;
}

function deepFreeze<T>(value: T, seen = new WeakSet<object>()): T {
  if (typeof value !== "object" || value === null) return value;
  if (seen.has(value)) return value;
  seen.add(value);
  for (const child of Object.values(value as Record<string, unknown>)) {
    deepFreeze(child, seen);
  }
  return Object.freeze(value);
}

function frozenState(
  state: Omit<InspectionState, "history"> & {
    history: readonly InspectedStep[];
  },
): InspectionState {
  const history = Object.freeze([...state.history]);
  return Object.freeze({ ...state, history });
}

function assertGeneration(generation: number): void {
  if (!Number.isSafeInteger(generation) || generation < 0) {
    throw new Error("inspection artifact generation must be a non-negative safe integer");
  }
}

function assertProgram(artifacts: InspectionArtifacts): void {
  assertGeneration(artifacts.generation);
  const { program } = artifacts;
  if (program.protocol !== "uhura-inspect/0" || program.kind !== "program") {
    throw new Error("inspection artifact must be an uhura-inspect/0 program");
  }
  if (program["span-offset-encoding"] !== "utf-8-bytes") {
    throw new Error("inspection artifact spans must use UTF-8 byte offsets");
  }
  if (program.ir.protocol !== "uhura-ir/0" || program.ir.hash.length === 0) {
    throw new Error("inspection artifact must identify a hashed uhura-ir/0 program");
  }
}

function assertCorrelated(
  artifacts: InspectionArtifacts,
  previous: InspectedStep | null,
  result: StepResult,
  inspection: InspectSnapshot,
): void {
  if (inspection.protocol !== "uhura-inspect/0" || inspection.kind !== "snapshot") {
    throw new Error("inspection step must be an uhura-inspect/0 snapshot");
  }
  if (inspection["ir-hash"] !== artifacts.program.ir.hash) {
    throw new Error("inspection snapshot IR hash does not match the program artifact");
  }
  if (!Number.isSafeInteger(inspection.revision) || inspection.revision < 1) {
    throw new Error("inspection snapshot revision must be a positive safe integer");
  }
  if (
    previous !== null
    && inspection.revision <= previous.inspection.revision
  ) {
    throw new Error("inspection snapshot revisions must increase monotonically");
  }
  if (inspection.u.rev !== inspection.revision) {
    throw new Error("inspection U revision does not match its snapshot revision");
  }
  if (inspection["u-hash"] !== result.t["u-hash"]) {
    throw new Error("inspection U hash does not match the step trace");
  }
  if (result.v.revision !== inspection.revision) {
    throw new Error("inspection revision does not match the step view revision");
  }
  if (inspection.view === null) {
    throw new Error("a successful Play step inspection must include view metadata");
  }
  if (inspection.view.revision !== result.v.revision) {
    throw new Error("inspection view metadata revision does not match the step view");
  }
  if (inspection.view["v-hash"] !== result.t["v-hash"]) {
    throw new Error("inspection view hash does not match the step trace");
  }
}

export function createInspectionStore(
  options: InspectionStoreOptions = {},
): InspectionStore {
  const historyLimit = options.historyLimit ?? DEFAULT_INSPECTION_HISTORY_LIMIT;
  if (!Number.isSafeInteger(historyLimit) || historyLimit < 1) {
    throw new RangeError("inspection history limit must be a positive safe integer");
  }

  const onListenerError =
    options.onListenerError
    ?? ((error: unknown) => console.error("uhura inspection listener failed", error));
  const listeners = new Set<InspectionListener>();
  let state = frozenState({
    disposed: false,
    historyLimit,
    artifacts: null,
    latest: null,
    history: [],
    evictedSteps: 0,
  });

  function notifyOne(listener: InspectionListener, published: InspectionState): void {
    try {
      listener(published);
    } catch (error) {
      try {
        onListenerError(error);
      } catch {
        // Debug listeners and their reporters are observational: neither may
        // interrupt Play or prevent the remaining subscribers from running.
      }
    }
  }

  function publish(next: InspectionState): void {
    state = next;
    for (const listener of [...listeners]) notifyOne(listener, next);
  }

  function subscribe(listener: InspectionListener): () => void {
    if (state.disposed) {
      notifyOne(listener, state);
      return () => {};
    }
    listeners.add(listener);
    notifyOne(listener, state);
    let subscribed = true;
    return () => {
      if (!subscribed) return;
      subscribed = false;
      listeners.delete(listener);
    };
  }

  const handle: InspectionHandle = Object.freeze({
    get state() {
      return state;
    },
    subscribe,
  });

  function installArtifacts(artifacts: InspectionArtifacts): boolean {
    if (state.disposed) return false;
    if (state.artifacts !== null) {
      throw new Error("inspection artifacts are already installed for this mount");
    }
    assertProgram(artifacts);
    const installed = deepFreeze(artifacts);
    publish(frozenState({ ...state, artifacts: installed }));
    return true;
  }

  function record(result: StepResult, inspection: InspectSnapshot): boolean {
    if (state.disposed) return false;
    const { artifacts } = state;
    if (artifacts === null) {
      throw new Error("inspection artifacts must be installed before recording steps");
    }
    assertCorrelated(artifacts, state.latest, result, inspection);

    const step = deepFreeze({ trace: result.t, inspection });
    const appended = [...state.history, step];
    const evicted = Math.max(0, appended.length - historyLimit);
    const history = evicted === 0 ? appended : appended.slice(evicted);
    publish(
      frozenState({
        ...state,
        latest: step,
        history,
        evictedSteps: state.evictedSteps + evicted,
      }),
    );
    return true;
  }

  function dispose(): void {
    if (state.disposed) return;
    publish(
      frozenState({
        disposed: true,
        historyLimit,
        artifacts: null,
        latest: null,
        history: [],
        evictedSteps: 0,
      }),
    );
    listeners.clear();
  }

  return Object.freeze({ handle, installArtifacts, record, dispose });
}
