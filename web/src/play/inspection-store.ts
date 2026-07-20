import type {
  RuntimeInspectedStep,
  RuntimeInspectionArtifacts,
  RuntimeInspectionHandle,
  RuntimeInspectionListener,
  RuntimeInspectionState,
} from "../protocol/types.js";
import type {
  Inspection,
  Receipt,
} from "../protocol/machine.js";

export const UHURA_INSPECTION_STATE_PROTOCOL =
  "uhura-runtime-inspection-state/0" as const;
export const DEFAULT_UHURA_INSPECTION_HISTORY_LIMIT = 128;

export interface InspectionStoreOptions {
  historyLimit?: number;
  onListenerError?: (error: unknown) => void;
}

export interface InspectionStore {
  readonly handle: RuntimeInspectionHandle;
  installArtifacts(artifacts: RuntimeInspectionArtifacts): boolean;
  record(inspection: Inspection, receipt: Receipt): boolean;
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
  state: Omit<RuntimeInspectionState, "protocol" | "history"> & {
    history: readonly RuntimeInspectedStep[];
  },
): RuntimeInspectionState {
  return Object.freeze({
    protocol: UHURA_INSPECTION_STATE_PROTOCOL,
    ...state,
    history: Object.freeze([...state.history]),
  });
}

function sameWire(left: unknown, right: unknown): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}

function assertArtifacts(artifacts: RuntimeInspectionArtifacts): void {
  if (!Number.isSafeInteger(artifacts.generation) || artifacts.generation < 0) {
    throw new TypeError(
      "Uhura machine inspection artifact generation must be a non-negative safe integer",
    );
  }
  if (artifacts.deployment.protocol !== "uhura-inspection/0") {
    throw new TypeError(
      "Uhura machine inspection artifacts must contain uhura-inspection/0 deployment metadata",
    );
  }
}

function assertCorrelated(
  artifacts: RuntimeInspectionArtifacts,
  previous: RuntimeInspectedStep | null,
  inspection: Inspection,
  receipt: Receipt,
): void {
  const deployment = artifacts.deployment;
  if (
    inspection.instance.length === 0
    || inspection.machineProgramHash !== deployment.machineProgramHash
    || inspection.presentation !== deployment.presentation
    || inspection.presentationHash !== deployment.presentationHash
  ) {
    throw new TypeError(
      "Uhura machine inspection does not match the admitted deployment identity",
    );
  }
  if (
    receipt.instance !== inspection.instance
    || receipt.machineProgramHash !== inspection.machineProgramHash
    || receipt.configurationHash !== inspection.configurationHash
  ) {
    throw new TypeError(
      "Uhura machine receipt does not match its runtime inspection identity",
    );
  }
  const retained = inspection.receipts.at(-1);
  if (!retained || !sameWire(retained, receipt)) {
    throw new TypeError(
      "Uhura machine inspection must retain the exact receipt being published",
    );
  }
  if (BigInt(inspection.nextSequence) !== BigInt(receipt.sequence) + 1n) {
    throw new TypeError(
      "Uhura machine inspection nextSequence must immediately follow its receipt",
    );
  }
  if (
    previous !== null
    && BigInt(receipt.sequence) !== BigInt(previous.receipt.sequence) + 1n
  ) {
    throw new TypeError(
      "Uhura machine inspection receipt sequences must increase contiguously",
    );
  }
}

export function createInspectionStore(
  options: InspectionStoreOptions = {},
): InspectionStore {
  const historyLimit =
    options.historyLimit ?? DEFAULT_UHURA_INSPECTION_HISTORY_LIMIT;
  if (!Number.isSafeInteger(historyLimit) || historyLimit < 1) {
    throw new RangeError(
      "Uhura machine inspection history limit must be a positive safe integer",
    );
  }
  const onListenerError =
    options.onListenerError
    ?? ((error: unknown) =>
      console.error("Uhura machine inspection listener failed", error));
  const listeners = new Set<RuntimeInspectionListener>();
  let state = frozenState({
    disposed: false,
    historyLimit,
    artifacts: null,
    latest: null,
    history: [],
    evictedSteps: 0,
  });

  function notify(
    listener: RuntimeInspectionListener,
    publication: RuntimeInspectionState,
  ): void {
    try {
      listener(publication);
    } catch (error) {
      try {
        onListenerError(error);
      } catch {
        // Inspection is observational; neither listeners nor reporters may
        // interrupt the machine or prevent the remaining listeners.
      }
    }
  }

  function publish(next: RuntimeInspectionState): void {
    state = next;
    for (const listener of [...listeners]) notify(listener, next);
  }

  const handle: RuntimeInspectionHandle = Object.freeze({
    get state() {
      return state;
    },
    subscribe(listener: RuntimeInspectionListener) {
      if (state.disposed) {
        notify(listener, state);
        return () => {};
      }
      listeners.add(listener);
      notify(listener, state);
      let subscribed = true;
      return () => {
        if (!subscribed) return;
        subscribed = false;
        listeners.delete(listener);
      };
    },
  });

  function installArtifacts(artifacts: RuntimeInspectionArtifacts): boolean {
    if (state.disposed) return false;
    if (state.artifacts !== null) {
      throw new Error(
        "Uhura machine inspection artifacts are already installed for this mount",
      );
    }
    assertArtifacts(artifacts);
    publish(frozenState({ ...state, artifacts: deepFreeze(artifacts) }));
    return true;
  }

  function record(
    inspection: Inspection,
    receipt: Receipt,
  ): boolean {
    if (state.disposed) return false;
    if (state.artifacts === null) {
      throw new Error(
        "Uhura machine inspection artifacts must be installed before runtime records",
      );
    }
    assertCorrelated(state.artifacts, state.latest, inspection, receipt);
    const step = deepFreeze({ inspection, receipt });
    const appended = [...state.history, step];
    const evicted = Math.max(0, appended.length - historyLimit);
    const history = evicted === 0 ? appended : appended.slice(evicted);
    publish(frozenState({
      ...state,
      latest: step,
      history,
      evictedSteps: state.evictedSteps + evicted,
    }));
    return true;
  }

  function dispose(): void {
    if (state.disposed) return;
    publish(frozenState({
      disposed: true,
      historyLimit,
      artifacts: null,
      latest: null,
      history: [],
      evictedSteps: 0,
    }));
    listeners.clear();
  }

  return Object.freeze({ handle, installArtifacts, record, dispose });
}
