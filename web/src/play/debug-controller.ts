// Framework-neutral lifecycle for the optional Play debugger. DOM ownership,
// layout, and graph rendering stay in the browser adapter; this controller only
// owns the inspection subscription and coalesces its publications to frames.

import type {
  InspectionHandle,
  InspectionState,
} from "../protocol/types.js";

export type DebugControllerUpdate =
  | {
      readonly kind: "inspection";
      readonly state: InspectionState;
    }
  | {
      readonly kind: "unavailable";
    };

export interface DebugControllerOptions {
  /** Resolved on each closed -> open transition, never while closed. */
  resolveInspection(): InspectionHandle | null | undefined;
  requestFrame(callback: () => void): number;
  cancelFrame(handle: number): void;
  render(update: DebugControllerUpdate): void;
}

export interface DebugController {
  readonly isOpen: boolean;
  readonly isDisposed: boolean;
  /** Returns true only when this call transitions the controller to open. */
  open(): boolean;
  /** Returns true only when this call transitions the controller to closed. */
  close(): boolean;
  /** Permanently retires the controller. Idempotent. */
  dispose(): boolean;
}

interface SubscriptionSlot {
  readonly generation: number;
  handle: InspectionHandle | null;
  stop: (() => void) | null;
  terminal: boolean;
}

interface FrameSlot {
  readonly generation: number;
  handle: number | undefined;
}

const UNAVAILABLE_UPDATE: DebugControllerUpdate = Object.freeze({
  kind: "unavailable",
});

export function createDebugController(
  options: DebugControllerOptions,
): DebugController {
  let open = false;
  let disposed = false;
  let generation = 0;
  let active: SubscriptionSlot | null = null;
  let pending: DebugControllerUpdate | null = null;
  let frame: FrameSlot | null = null;

  function release(slot: SubscriptionSlot): void {
    if (active === slot) active = null;
    slot.handle = null;
    const stop = slot.stop;
    slot.stop = null;
    if (stop === null) return;
    try {
      stop();
    } catch {
      // Debug subscriptions are observational. Cleanup must still retire every
      // other reference when a third-party handle has a faulty unsubscribe.
    }
  }

  function cancelPendingFrame(): void {
    pending = null;
    const canceled = frame;
    frame = null;
    if (canceled === null) return;
    const { handle } = canceled;
    if (handle === undefined) return;
    try {
      options.cancelFrame(handle);
    } catch {
      // The generation/open checks below also reject a callback from a frame
      // scheduler that failed to cancel its work.
    }
  }

  function queue(update: DebugControllerUpdate, owner: number): void {
    if (disposed || !open || owner !== generation) return;
    pending = update;
    if (frame !== null) return;

    const scheduled: FrameSlot = { generation: owner, handle: undefined };
    frame = scheduled;
    let ranSynchronously = false;
    let requested: number;
    try {
      requested = options.requestFrame(() => {
        ranSynchronously = true;
        // A scheduler may invoke a callback even after cancelFrame. An older
        // callback must not consume a newer activation's pending state.
        if (frame !== scheduled) return;
        frame = null;
        const latest = pending;
        pending = null;
        if (
          latest !== null
          && !disposed
          && open
          && scheduled.generation === generation
        ) {
          options.render(latest);
        }
      });
    } catch (error) {
      if (frame === scheduled) frame = null;
      pending = null;
      throw error;
    }

    // Browser rAF is asynchronous, but accepting a synchronous injected
    // scheduler makes the lifecycle deterministic in small hosts and tests.
    if (!ranSynchronously && frame === scheduled) scheduled.handle = requested;
  }

  function deactivate(): void {
    open = false;
    generation += 1;
    cancelPendingFrame();
    const slot = active;
    if (slot !== null) release(slot);
  }

  function openController(): boolean {
    if (disposed || open) return false;
    open = true;
    const owner = ++generation;

    let handle: InspectionHandle | null | undefined;
    try {
      handle = options.resolveInspection();
    } catch {
      queue(UNAVAILABLE_UPDATE, owner);
      return true;
    }
    if (handle == null) {
      queue(UNAVAILABLE_UPDATE, owner);
      return true;
    }

    const slot: SubscriptionSlot = {
      generation: owner,
      handle,
      stop: null,
      terminal: false,
    };
    active = slot;

    let stop: () => void;
    try {
      stop = handle.subscribe((state) => {
        if (
          disposed
          || !open
          || generation !== slot.generation
          || active !== slot
        ) {
          return;
        }
        queue(Object.freeze({ kind: "inspection", state }), slot.generation);
        if (state.disposed) {
          slot.terminal = true;
          if (slot.stop !== null) release(slot);
        }
      });
    } catch {
      release(slot);
      queue(UNAVAILABLE_UPDATE, owner);
      return true;
    }

    slot.stop = stop;
    // InspectionHandle.subscribe replays synchronously. The replay, or a
    // custom handle around it, may close/dispose this controller before the
    // unsubscribe function is returned. Never retain that late function.
    if (
      disposed
      || !open
      || generation !== slot.generation
      || active !== slot
      || slot.terminal
    ) {
      release(slot);
    }
    return true;
  }

  function closeController(): boolean {
    if (disposed || !open) return false;
    deactivate();
    return true;
  }

  const controller: DebugController = {
    get isOpen() {
      return open;
    },
    get isDisposed() {
      return disposed;
    },
    open: openController,
    close: closeController,
    dispose(): boolean {
      if (disposed) return false;
      if (open) deactivate();
      else {
        generation += 1;
        cancelPendingFrame();
        const slot = active;
        if (slot !== null) release(slot);
      }
      disposed = true;
      return true;
    },
  };
  return Object.freeze(controller);
}
