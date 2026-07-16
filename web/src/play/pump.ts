// The event pump (§8.4, normative): renderer emissions always ENQUEUE;
// a `pumping` flag makes nested pumps no-ops — the wasm Session is
// single-borrow, so re-entering `dispatch` from inside `onStep` would
// panic. Post-drain observation checks run in a microtask.

import type { ProviderMsg, StepResult } from "../protocol/types.js";

interface PumpWiring {
  dispatch: (eventJson: string) => string;
  deliver: (cmdJson: string) => void;
  onStep: (result: StepResult) => void;
  onError: (error: unknown, eventJson: string) => void;
  onDrained?: () => void;
}

interface QueuedEvent {
  eventJson: string;
  onApplied?: () => void;
}

export function createPump({ dispatch, deliver, onStep, onError, onDrained }: PumpWiring) {
  const queue: QueuedEvent[] = [];
  let pumping = false;

  /**
   * The ONLY entry point — everything (user input, driver ticks, Init)
   * goes through the queue, so step order is arrival order.
   * @param {string} eventJson
   * @param {() => void} [onApplied] runs right after this event's step
   *   lands (the textfield in-flight accounting hangs off this)
   */
  function enqueue(eventJson: string, onApplied?: () => void): void {
    queue.push(onApplied ? { eventJson, onApplied } : { eventJson });
    pump();
  }

  function pump() {
    if (pumping) return;
    pumping = true;
    try {
      while (queue.length > 0) {
        const item = queue.shift();
        if (!item) break;
        let resultJson: string;
        try {
          resultJson = dispatch(item.eventJson);
        } catch (error) {
          item.onApplied?.();
          onError(error, item.eventJson);
          continue;
        }
        const result = JSON.parse(resultJson) as StepResult;
        // Emitted commands go to the provider as they appear (§7.2). A
        // provider refusal to ACCEPT one (unscripted command, §9.5) is
        // reported but must not skip the render below: the machine
        // stepped — the DOM tracks the session, never the provider.
        for (const c of result.c) {
          const cmdJson = JSON.stringify(c);
          try {
            deliver(cmdJson);
          } catch (error) {
            onError(error, cmdJson);
          }
        }
        // onStep reconciles the DOM; anything it provokes (focus events,
        // observation flips) re-enters via enqueue and drains here.
        onStep(result);
        item.onApplied?.();
      }
    } finally {
      pumping = false;
    }
    if (queue.length > 0) {
      pump(); // an emission slipped in during the finally window
    } else if (onDrained) {
      queueMicrotask(() => {
        if (!pumping && queue.length === 0) onDrained();
      });
    }
  }

  return { enqueue };
}

/**
 * Maps one provider wire message to its external event (§7.2): a
 * standalone projection update wraps into an `updates` list; `outcome`
 * and `projection-failed` are shape-identical pass-throughs.
 * @param {string} msgJson
 * @returns {string} event JSON for `Session.dispatch`
 */
export function providerMsgToEvent(msgJson: string): string {
  const msg = JSON.parse(msgJson) as ProviderMsg;
  switch (msg.kind) {
    case "projection":
      return JSON.stringify({ kind: "projection", updates: [msg] });
    case "outcome":
    case "projection-failed":
      return msgJson;
    default:
      throw new Error(`the driver emitted a \`${msg.kind}\` message`);
  }
}
