/**
 * Plain TypeScript answer sheet for Uhura's frozen L0-L2 program harnesses.
 *
 * There is no reducer framework, effect runtime, scheduler, clock, or global
 * store here. Each step is a pure synchronous function. Commands are returned
 * as data and can be interpreted only after the function returns.
 */

function assertNever(value: never): never {
  throw new Error("unexpected closed-union member: " + String(value));
}

// L0: bounded counter

export interface CounterConfig {
  readonly minimum: bigint;
  readonly maximum: bigint;
  readonly initial: bigint;
}

export interface CounterState {
  readonly count: bigint;
}

export type CounterInput = "increment" | "decrement" | "reset";

export interface CounterObservation {
  readonly count: bigint;
  readonly atMinimum: boolean;
  readonly atMaximum: boolean;
}

export function createCounter(config: CounterConfig): CounterState {
  if (config.minimum > config.initial || config.initial > config.maximum) {
    throw new RangeError(
      "counter requires minimum <= initial <= maximum",
    );
  }
  return { count: config.initial };
}

export function stepCounter(
  config: CounterConfig,
  state: CounterState,
  input: CounterInput,
): CounterState {
  switch (input) {
    case "increment":
      return {
        count:
          state.count < config.maximum
            ? state.count + 1n
            : config.maximum,
      };

    case "decrement":
      return {
        count:
          state.count > config.minimum
            ? state.count - 1n
            : config.minimum,
      };

    case "reset":
      return { count: config.initial };

    default:
      return assertNever(input);
  }
}

export function observeCounter(
  config: CounterConfig,
  state: CounterState,
): CounterObservation {
  return {
    count: state.count,
    atMinimum: state.count === config.minimum,
    atMaximum: state.count === config.maximum,
  };
}

// L1: river crossing

export type Side = "left" | "right";
export type Cargo = "wolf" | "goat" | "cabbage";
export type Violation = "wolf-with-goat" | "goat-with-cabbage";

export interface RiverPositions {
  readonly farmer: Side;
  readonly wolf: Side;
  readonly goat: Side;
  readonly cabbage: Side;
}

export interface RiverState {
  readonly positions: RiverPositions;
}

export interface RiverInput {
  readonly type: "cross";
  readonly passenger: Cargo | null;
}

export type RiverOutcome =
  | {
      readonly type: "accepted";
      readonly passenger: Cargo | null;
      readonly departure: Side;
      readonly arrival: Side;
    }
  | {
      readonly type: "refused";
      readonly reason: {
        readonly type: "passenger-not-with-farmer";
        readonly passenger: Cargo;
      };
    }
  | {
      readonly type: "refused";
      readonly reason: {
        readonly type: "unsafe";
        readonly violations: readonly [Violation, ...Violation[]];
      };
    };

export interface RiverStep {
  readonly state: RiverState;
  readonly outcome: RiverOutcome;
}

export interface RiverObservation {
  readonly positions: RiverPositions;
  readonly status: "in-progress" | "solved";
}

export const INITIAL_RIVER_POSITIONS: RiverPositions = {
  farmer: "left",
  wolf: "left",
  goat: "left",
  cabbage: "left",
};

function opposite(side: Side): Side {
  return side === "left" ? "right" : "left";
}

export function riverViolations(
  positions: RiverPositions,
): readonly Violation[] {
  const violations: Violation[] = [];

  if (
    positions.wolf === positions.goat &&
    positions.farmer !== positions.wolf
  ) {
    violations.push("wolf-with-goat");
  }

  if (
    positions.goat === positions.cabbage &&
    positions.farmer !== positions.goat
  ) {
    violations.push("goat-with-cabbage");
  }

  return violations;
}

export function createRiverState(
  positions: RiverPositions = INITIAL_RIVER_POSITIONS,
): RiverState {
  if (riverViolations(positions).length !== 0) {
    throw new RangeError("unsafe positions are outside the river state domain");
  }
  return { positions: { ...positions } };
}

export function stepRiver(
  state: RiverState,
  input: RiverInput,
): RiverStep {
  const departure = state.positions.farmer;
  const passenger = input.passenger;

  if (
    passenger !== null &&
    state.positions[passenger] !== departure
  ) {
    return {
      state,
      outcome: {
        type: "refused",
        reason: {
          type: "passenger-not-with-farmer",
          passenger,
        },
      },
    };
  }

  const arrival = opposite(departure);
  let candidate: RiverPositions = {
    ...state.positions,
    farmer: arrival,
  };

  if (passenger !== null) {
    candidate = {
      ...candidate,
      [passenger]: arrival,
    };
  }

  const violations = riverViolations(candidate);
  const [firstViolation, ...remainingViolations] = violations;
  if (firstViolation !== undefined) {
    return {
      state,
      outcome: {
        type: "refused",
        reason: {
          type: "unsafe",
          violations: [firstViolation, ...remainingViolations],
        },
      },
    };
  }

  return {
    state: { positions: candidate },
    outcome: {
      type: "accepted",
      passenger,
      departure,
      arrival,
    },
  };
}

export function observeRiver(state: RiverState): RiverObservation {
  const positions = state.positions;
  return {
    positions,
    status:
      positions.farmer === "right" &&
      positions.wolf === "right" &&
      positions.goat === "right" &&
      positions.cabbage === "right"
        ? "solved"
        : "in-progress",
  };
}

// L2: keyed task supervisor

export type TaskId = string;
export type Attempt = bigint;

export type TaskPhase =
  | { readonly type: "queued" }
  | {
      readonly type: "running";
      readonly attempt: Attempt;
      readonly progress: number;
    }
  | { readonly type: "succeeded" }
  | { readonly type: "failed" }
  | { readonly type: "cancelled" };

export interface Task {
  readonly phase: TaskPhase;
  readonly started: bigint;
}

export interface SupervisorState {
  readonly tasks: ReadonlyMap<TaskId, Task>;
  readonly queue: readonly TaskId[];
}

export type SupervisorInput =
  | { readonly type: "submit"; readonly task: TaskId }
  | { readonly type: "cancel"; readonly task: TaskId }
  | { readonly type: "retry"; readonly task: TaskId }
  | {
      readonly type: "progress";
      readonly task: TaskId;
      readonly attempt: Attempt;
      readonly value: number;
    }
  | {
      readonly type: "succeed";
      readonly task: TaskId;
      readonly attempt: Attempt;
    }
  | {
      readonly type: "fail";
      readonly task: TaskId;
      readonly attempt: Attempt;
    };

export type WorkerCommand =
  | {
      readonly type: "start";
      readonly task: TaskId;
      readonly attempt: Attempt;
    }
  | {
      readonly type: "cancel";
      readonly task: TaskId;
      readonly attempt: Attempt;
    };

export type SupervisorClassification =
  | "accepted"
  | "duplicate"
  | "stale"
  | "invalid";

export interface SupervisorStep {
  readonly state: SupervisorState;
  readonly classification: SupervisorClassification;
  readonly commands: readonly WorkerCommand[];
}

export interface RunningObservation {
  readonly attempt: Attempt;
  readonly progress: number;
}

export interface SupervisorObservation {
  readonly tasks: ReadonlyMap<TaskId, Task>;
  readonly queue: readonly TaskId[];
  readonly running: ReadonlyMap<TaskId, RunningObservation>;
  readonly availableCapacity: number;
}

const CONCURRENCY_LIMIT = 2;

const QUEUED: TaskPhase = { type: "queued" };
const SUCCEEDED: TaskPhase = { type: "succeeded" };
const FAILED: TaskPhase = { type: "failed" };
const CANCELLED: TaskPhase = { type: "cancelled" };

export function createSupervisor(): SupervisorState {
  return {
    tasks: new Map(),
    queue: [],
  };
}

function ignored(
  state: SupervisorState,
  classification: Exclude<SupervisorClassification, "accepted">,
): SupervisorStep {
  return {
    state,
    classification,
    commands: [],
  };
}

function replaceTask(
  state: SupervisorState,
  id: TaskId,
  task: Task,
  queue: readonly TaskId[] = state.queue,
): SupervisorState {
  const tasks = new Map(state.tasks);
  tasks.set(id, task);
  return { tasks, queue };
}

function countRunning(tasks: ReadonlyMap<TaskId, Task>): number {
  let count = 0;
  for (const task of tasks.values()) {
    if (task.phase.type === "running") {
      count += 1;
    }
  }
  return count;
}

function acceptAndSchedule(
  directState: SupervisorState,
  directCommands: readonly WorkerCommand[] = [],
): SupervisorStep {
  const tasks = new Map(directState.tasks);
  const queue = [...directState.queue];
  const commands = [...directCommands];
  let runningCount = countRunning(tasks);

  while (runningCount < CONCURRENCY_LIMIT && queue.length !== 0) {
    const id = queue.shift();
    if (id === undefined) {
      throw new Error("queue length and shift result disagree");
    }

    const task = tasks.get(id);
    if (task === undefined || task.phase.type !== "queued") {
      throw new Error("supervisor invariant: queue head must be queued");
    }

    const attempt = task.started + 1n;
    tasks.set(id, {
      started: attempt,
      phase: {
        type: "running",
        attempt,
        progress: 0,
      },
    });
    commands.push({ type: "start", task: id, attempt });
    runningCount += 1;
  }

  return {
    state: { tasks, queue },
    classification: "accepted",
    commands,
  };
}

function settle(
  state: SupervisorState,
  id: TaskId,
  attempt: Attempt,
  terminal: "succeed" | "fail",
): SupervisorStep {
  if (attempt <= 0n) {
    return ignored(state, "invalid");
  }

  const task = state.tasks.get(id);
  if (task === undefined) {
    return ignored(state, "invalid");
  }

  if (attempt > task.started) {
    return ignored(state, "invalid");
  }
  if (attempt < task.started) {
    return ignored(state, "stale");
  }

  switch (task.phase.type) {
    case "running": {
      if (task.phase.attempt !== attempt) {
        throw new Error(
          "supervisor invariant: running attempt must equal started ledger",
        );
      }
      const phase = terminal === "succeed" ? SUCCEEDED : FAILED;
      return acceptAndSchedule(
        replaceTask(state, id, { ...task, phase }),
      );
    }

    case "succeeded":
      return ignored(
        state,
        terminal === "succeed" ? "duplicate" : "stale",
      );

    case "failed":
      return ignored(
        state,
        terminal === "fail" ? "duplicate" : "stale",
      );

    case "queued":
    case "cancelled":
      return ignored(state, "stale");

    default:
      return assertNever(task.phase);
  }
}

export function stepSupervisor(
  state: SupervisorState,
  input: SupervisorInput,
): SupervisorStep {
  switch (input.type) {
    case "submit": {
      if (state.tasks.has(input.task)) {
        return ignored(state, "invalid");
      }

      return acceptAndSchedule(
        replaceTask(
          state,
          input.task,
          { phase: QUEUED, started: 0n },
          [...state.queue, input.task],
        ),
      );
    }

    case "cancel": {
      const task = state.tasks.get(input.task);
      if (task === undefined) {
        return ignored(state, "invalid");
      }

      switch (task.phase.type) {
        case "queued":
          return acceptAndSchedule(
            replaceTask(
              state,
              input.task,
              { ...task, phase: CANCELLED },
              state.queue.filter((id) => id !== input.task),
            ),
          );

        case "running":
          return acceptAndSchedule(
            replaceTask(state, input.task, {
              ...task,
              phase: CANCELLED,
            }),
            [
              {
                type: "cancel",
                task: input.task,
                attempt: task.phase.attempt,
              },
            ],
          );

        case "cancelled":
          return ignored(state, "duplicate");

        case "succeeded":
        case "failed":
          return ignored(state, "invalid");

        default:
          return assertNever(task.phase);
      }
    }

    case "retry": {
      const task = state.tasks.get(input.task);
      if (task === undefined) {
        return ignored(state, "invalid");
      }

      switch (task.phase.type) {
        case "failed":
        case "cancelled":
          return acceptAndSchedule(
            replaceTask(
              state,
              input.task,
              { ...task, phase: QUEUED },
              [...state.queue, input.task],
            ),
          );

        case "queued":
        case "running":
        case "succeeded":
          return ignored(state, "invalid");

        default:
          return assertNever(task.phase);
      }
    }

    case "progress": {
      if (
        input.attempt <= 0n ||
        !Number.isFinite(input.value) ||
        input.value < 0 ||
        input.value > 1
      ) {
        return ignored(state, "invalid");
      }

      const task = state.tasks.get(input.task);
      if (task === undefined) {
        return ignored(state, "invalid");
      }

      if (input.attempt > task.started) {
        return ignored(state, "invalid");
      }
      if (input.attempt < task.started) {
        return ignored(state, "stale");
      }
      if (task.phase.type !== "running") {
        return ignored(state, "stale");
      }
      if (task.phase.attempt !== input.attempt) {
        throw new Error(
          "supervisor invariant: running attempt must equal started ledger",
        );
      }
      if (input.value < task.phase.progress) {
        return ignored(state, "stale");
      }
      if (input.value === task.phase.progress) {
        return ignored(state, "duplicate");
      }

      return acceptAndSchedule(
        replaceTask(state, input.task, {
          ...task,
          phase: {
            type: "running",
            attempt: input.attempt,
            progress: input.value,
          },
        }),
      );
    }

    case "succeed":
      return settle(state, input.task, input.attempt, "succeed");

    case "fail":
      return settle(state, input.task, input.attempt, "fail");

    default:
      return assertNever(input);
  }
}

export function observeSupervisor(
  state: SupervisorState,
): SupervisorObservation {
  const running = new Map<TaskId, RunningObservation>();
  for (const [id, task] of state.tasks) {
    if (task.phase.type === "running") {
      running.set(id, {
        attempt: task.phase.attempt,
        progress: task.phase.progress,
      });
    }
  }

  return {
    tasks: state.tasks,
    queue: state.queue,
    running,
    availableCapacity: CONCURRENCY_LIMIT - running.size,
  };
}
