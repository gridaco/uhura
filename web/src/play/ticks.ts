// Wall time → driver ticks (§8.4): the fixture driver schedules in
// integer ticks (`after-ticks ≥ 1`, so optimistic states are always
// observable); the shell fires `driver.tick()` on one fixed cadence.
// Traces depend only on tick ordinals, never on wall time. Host mechanics do
// not consume the prototype's URL query namespace.

export const DEFAULT_TICK_MS = 250;

interface TickWiring {
  tick: () => string[];
  idle: () => boolean;
  enqueue: (eventJson: string) => void;
  toEvent: (msgJson: string) => string;
  intervalMs: number;
}

/**
 * @param {{
 *   tick: () => string[],
 *   idle: () => boolean,
 *   enqueue: (eventJson: string) => void,
 *   toEvent: (msgJson: string) => string,
 *   intervalMs: number,
 * }} wiring
 */
export function createTicks({ tick, idle, enqueue, toEvent, intervalMs }: TickWiring) {
  let timer: ReturnType<typeof setInterval> | undefined;

  function fire() {
    // An idle driver still ticks: [[deliver]] entries are scheduled on
    // absolute ticks, and `idle()` only reports the current schedule.
    for (const msgJson of tick()) enqueue(toEvent(msgJson));
  }

  return {
    start() {
      if (timer === undefined) timer = setInterval(fire, intervalMs);
    },
    stop() {
      if (timer !== undefined) clearInterval(timer);
      timer = undefined;
    },
    /** Test/debug hook: advance N ticks synchronously. */
    advance(n: number) {
      for (let i = 0; i < n; i += 1) fire();
    },
    idle,
  };
}
