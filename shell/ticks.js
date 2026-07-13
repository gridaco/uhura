// Wall time → driver ticks (§8.4): the fixture driver schedules in
// integer ticks (`after-ticks ≥ 1`, so optimistic states are always
// observable); the shell fires `driver.tick()` on a fixed cadence —
// 250 ms default, `?tick=<ms>` override (plan micro-decision #18).
// Traces depend only on tick ordinals, never on wall time.

export const DEFAULT_TICK_MS = 250;

/** @param {string} search e.g. `location.search` */
export function tickMillis(search) {
  const raw = new URLSearchParams(search).get("tick");
  const ms = raw === null ? Number.NaN : Number(raw);
  return Number.isInteger(ms) && ms > 0 ? ms : DEFAULT_TICK_MS;
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
export function createTicks({ tick, idle, enqueue, toEvent, intervalMs }) {
  /** @type {number | undefined} */
  let timer;

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
    advance(/** @type {number} */ n) {
      for (let i = 0; i < n; i += 1) fire();
    },
    idle,
  };
}
