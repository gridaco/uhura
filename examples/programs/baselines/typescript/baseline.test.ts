import {
  INITIAL_RIVER_POSITIONS,
  createCounter,
  createRiverState,
  createSupervisor,
  observeCounter,
  observeRiver,
  observeSupervisor,
  riverViolations,
  stepCounter,
  stepRiver,
  stepSupervisor,
  type Cargo,
  type RiverInput,
  type RiverPositions,
  type RiverState,
  type RiverStep,
  type Side,
  type SupervisorInput,
  type SupervisorState,
  type Task,
  type WorkerCommand,
} from "./baseline.ts";

function normalized(value: unknown): unknown {
  if (typeof value === "bigint") {
    return { $bigint: value.toString() };
  }
  if (Array.isArray(value)) {
    return value.map(normalized);
  }
  if (value instanceof Map) {
    return {
      $map: [...value.entries()]
        .sort(([left], [right]) => String(left).localeCompare(String(right)))
        .map(([key, item]) => [normalized(key), normalized(item)]),
    };
  }
  if (value !== null && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value)
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([key, item]) => [key, normalized(item)]),
    );
  }
  return value;
}

function expectEqual(
  actual: unknown,
  expected: unknown,
  label: string,
): void {
  const actualText = JSON.stringify(normalized(actual));
  const expectedText = JSON.stringify(normalized(expected));
  if (actualText !== expectedText) {
    throw new Error(
      `${label}\nexpected: ${expectedText}\nactual:   ${actualText}`,
    );
  }
}

function expectSame(
  actual: unknown,
  expected: unknown,
  label: string,
): void {
  if (actual !== expected) {
    throw new Error(`${label}: expected the original value`);
  }
}

function expectThrows(operation: () => void, label: string): void {
  try {
    operation();
  } catch {
    return;
  }
  throw new Error(`${label}: expected operation to throw`);
}

// L0: canonical and adversarial traces

{
  const config = { minimum: 0n, maximum: 2n, initial: 0n };
  let state = createCounter(config);
  const observed = [observeCounter(config, state)];

  for (const input of [
    "increment",
    "increment",
    "increment",
    "decrement",
    "reset",
    "decrement",
  ] as const) {
    state = stepCounter(config, state, input);
    observed.push(observeCounter(config, state));
  }

  expectEqual(
    observed,
    [
      { count: 0n, atMinimum: true, atMaximum: false },
      { count: 1n, atMinimum: false, atMaximum: false },
      { count: 2n, atMinimum: false, atMaximum: true },
      { count: 2n, atMinimum: false, atMaximum: true },
      { count: 1n, atMinimum: false, atMaximum: false },
      { count: 0n, atMinimum: true, atMaximum: false },
      { count: 0n, atMinimum: true, atMaximum: false },
    ],
    "L0 canonical trace",
  );

  for (const test of [
    {
      config: { minimum: 7n, maximum: 7n, initial: 7n },
      inputs: ["increment", "decrement", "reset"] as const,
      expected: [7n, 7n, 7n],
    },
    {
      config: { minimum: -2n, maximum: 1n, initial: -1n },
      inputs: ["decrement", "decrement", "increment", "reset"] as const,
      expected: [-2n, -2n, -1n, -1n],
    },
    {
      config: { minimum: 0n, maximum: 2n, initial: 1n },
      inputs: ["increment", "reset", "decrement", "reset"] as const,
      expected: [2n, 1n, 0n, 1n],
    },
  ]) {
    let current = createCounter(test.config);
    const counts: bigint[] = [];
    for (const input of test.inputs) {
      current = stepCounter(test.config, current, input);
      counts.push(current.count);
    }
    expectEqual(counts, test.expected, "L0 adversarial trace");
  }

  const degenerateConfig = { minimum: 7n, maximum: 7n, initial: 7n };
  expectEqual(
    observeCounter(degenerateConfig, createCounter(degenerateConfig)),
    { count: 7n, atMinimum: true, atMaximum: true },
    "L0 degenerate observation",
  );

  expectThrows(
    () =>
      void createCounter({ minimum: 2n, maximum: 1n, initial: 1n }),
    "L0 rejects reordered bounds",
  );
  expectThrows(
    () =>
      void createCounter({ minimum: 0n, maximum: 2n, initial: 3n }),
    "L0 rejects out-of-range initial value",
  );
}

// L1: canonical trace, adversarial cases, and complete 40-case oracle

const CROSSINGS: readonly RiverInput[] = [
  { type: "cross", passenger: null },
  { type: "cross", passenger: "wolf" },
  { type: "cross", passenger: "goat" },
  { type: "cross", passenger: "cabbage" },
];

function positionsKey(positions: RiverPositions): string {
  return [
    positions.farmer,
    positions.wolf,
    positions.goat,
    positions.cabbage,
  ]
    .map((side) => (side === "left" ? "0" : "1"))
    .join("");
}

function positionsFromBits(bits: number): RiverPositions {
  const side = (shift: number): Side =>
    (bits & (1 << shift)) === 0 ? "left" : "right";
  return {
    farmer: side(3),
    wolf: side(2),
    goat: side(1),
    cabbage: side(0),
  };
}

const RIVER_ORACLE = {
  "0000": ["U:WG+GC", "U:GC", "A:1010:goat:01", "U:WG"],
  "0001": ["U:WG", "A:1101:wolf:01", "A:1011:goat:01", "P:cabbage"],
  "0010": [
    "A:1010:none:01",
    "A:1110:wolf:01",
    "P:goat",
    "A:1011:cabbage:01",
  ],
  "0100": [
    "U:GC",
    "P:wolf",
    "A:1110:goat:01",
    "A:1101:cabbage:01",
  ],
  "0101": [
    "A:1101:none:01",
    "P:wolf",
    "A:1111:goat:01",
    "P:cabbage",
  ],
  "1010": [
    "A:0010:none:10",
    "P:wolf",
    "A:0000:goat:10",
    "P:cabbage",
  ],
  "1011": [
    "U:GC",
    "P:wolf",
    "A:0001:goat:10",
    "A:0010:cabbage:10",
  ],
  "1101": [
    "A:0101:none:10",
    "A:0001:wolf:10",
    "P:goat",
    "A:0100:cabbage:10",
  ],
  "1110": ["U:WG", "A:0010:wolf:10", "A:0100:goat:10", "P:cabbage"],
  "1111": ["U:WG+GC", "U:GC", "A:0101:goat:10", "U:WG"],
} as const;

function sideBit(side: Side): "0" | "1" {
  return side === "left" ? "0" : "1";
}

function mirrorSide(side: Side): Side {
  return side === "left" ? "right" : "left";
}

function riverStepKey(step: RiverStep): string {
  const outcome = step.outcome;
  if (outcome.type === "accepted") {
    return [
      "A",
      positionsKey(step.state.positions),
      outcome.passenger ?? "none",
      `${sideBit(outcome.departure)}${sideBit(outcome.arrival)}`,
    ].join(":");
  }
  if (outcome.reason.type === "passenger-not-with-farmer") {
    return `P:${outcome.reason.passenger}`;
  }
  const violations = outcome.reason.violations.map((violation) =>
    violation === "wolf-with-goat" ? "WG" : "GC"
  );
  return `U:${violations.join("+")}`;
}

function mirrorPositions(positions: RiverPositions): RiverPositions {
  return {
    farmer: mirrorSide(positions.farmer),
    wolf: mirrorSide(positions.wolf),
    goat: mirrorSide(positions.goat),
    cabbage: mirrorSide(positions.cabbage),
  };
}

{
  let state = createRiverState();
  const canonicalPassengers: readonly (Cargo | null)[] = [
    "goat",
    null,
    "wolf",
    "goat",
    "cabbage",
    null,
    "goat",
  ];
  const expectedStates = [
    "1010",
    "0010",
    "1110",
    "0100",
    "1101",
    "0101",
    "1111",
  ];
  const expectedOutcomes = [
    "A:1010:goat:01",
    "A:0010:none:10",
    "A:1110:wolf:01",
    "A:0100:goat:10",
    "A:1101:cabbage:01",
    "A:0101:none:10",
    "A:1111:goat:01",
  ];
  const canonicalResults: string[] = [];
  const canonicalObservations = [observeRiver(state)];
  expectEqual(
    canonicalObservations[0]?.status,
    "in-progress",
    "L1 initial status",
  );

  canonicalPassengers.forEach((passenger, index) => {
    const step = stepRiver(state, { type: "cross", passenger });
    const result = riverStepKey(step);
    expectEqual(
      result,
      expectedOutcomes[index],
      `L1 canonical outcome ${index + 1}`,
    );
    canonicalResults.push(result);
    state = step.state;
    canonicalObservations.push(observeRiver(state));
    expectEqual(
      positionsKey(state.positions),
      expectedStates[index],
      `L1 canonical state ${index + 1}`,
    );
  });
  expectEqual(observeRiver(state).status, "solved", "L1 canonical goal");
  const leftGoal = stepRiver(state, { type: "cross", passenger: "goat" });
  expectEqual(
    riverStepKey(leftGoal),
    "A:0101:goat:10",
    "L1 goal remains nonabsorbing",
  );
  expectEqual(
    observeRiver(leftGoal.state).status,
    "in-progress",
    "L1 leaving goal changes derived status",
  );

  let replayState = createRiverState();
  const replayResults: string[] = [];
  const replayObservations = [observeRiver(replayState)];
  for (const passenger of canonicalPassengers) {
    const step = stepRiver(replayState, { type: "cross", passenger });
    replayResults.push(riverStepKey(step));
    replayState = step.state;
    replayObservations.push(observeRiver(replayState));
  }
  expectEqual(replayResults, canonicalResults, "L1 outcome replay");
  expectEqual(
    replayObservations,
    canonicalObservations,
    "L1 observation replay",
  );

  const initial = createRiverState();
  const alone = stepRiver(initial, { type: "cross", passenger: null });
  expectEqual(
    alone.outcome,
    {
      type: "refused",
      reason: {
        type: "unsafe",
        violations: ["wolf-with-goat", "goat-with-cabbage"],
      },
    },
    "L1 crossing alone lists both harms in canonical order",
  );
  expectSame(alone.state, initial, "L1 unsafe crossing stutters");

  expectEqual(
    stepRiver(initial, { type: "cross", passenger: "wolf" }).outcome,
    {
      type: "refused",
      reason: {
        type: "unsafe",
        violations: ["goat-with-cabbage"],
      },
    },
    "L1 wolf refusal",
  );
  expectEqual(
    stepRiver(initial, { type: "cross", passenger: "cabbage" }).outcome,
    {
      type: "refused",
      reason: {
        type: "unsafe",
        violations: ["wolf-with-goat"],
      },
    },
    "L1 cabbage refusal",
  );

  const afterGoat = stepRiver(initial, {
    type: "cross",
    passenger: "goat",
  }).state;
  const misplacedWolf = stepRiver(afterGoat, {
    type: "cross",
    passenger: "wolf",
  });
  expectEqual(
    misplacedWolf.outcome,
    {
      type: "refused",
      reason: {
        type: "passenger-not-with-farmer",
        passenger: "wolf",
      },
    },
    "L1 passenger-location precedence",
  );
  expectSame(
    misplacedWolf.state,
    afterGoat,
    "L1 passenger refusal stutters",
  );
  expectEqual(
    stepRiver(afterGoat, { type: "cross", passenger: "goat" }).state
      .positions,
    INITIAL_RIVER_POSITIONS,
    "L1 accepted crossings are reversible",
  );

  const safeKeys = new Set(Object.keys(RIVER_ORACLE));
  for (let bits = 0; bits < 16; bits += 1) {
    const positions = positionsFromBits(bits);
    const key = positionsKey(positions);
    if (safeKeys.has(key)) {
      expectEqual(
        createRiverState(positions).positions,
        positions,
        `L1 admits frozen safe state ${key}`,
      );
    } else {
      expectThrows(
        () => void createRiverState(positions),
        `L1 rejects frozen unsafe state ${key}`,
      );
    }
  }
  expectEqual(safeKeys.size, 10, "L1 has ten frozen safe assignments");

  let accepted = 0;
  let passengerRefusals = 0;
  let unsafeRefusals = 0;
  const unsafeProfiles = new Map<string, number>();

  for (const [bits, expectedResults] of Object.entries(RIVER_ORACLE)) {
    const current = createRiverState(
      positionsFromBits(Number.parseInt(bits, 2)),
    );
    expectedResults.forEach((expected, inputIndex) => {
      const input = CROSSINGS[inputIndex];
      if (input === undefined) {
        throw new Error("L1 frozen oracle has more results than inputs");
      }
      const step = stepRiver(current, input);
      expectEqual(
        riverStepKey(step),
        expected,
        `L1 frozen oracle ${bits}/${input.passenger ?? "alone"}`,
      );

      if (step.outcome.type === "accepted") {
        accepted += 1;
        expectEqual(
          riverViolations(step.state.positions),
          [],
          "L1 accepted transition remains safe",
        );
        expectEqual(
          step.state.positions.farmer,
          mirrorSide(current.positions.farmer),
          "L1 accepted transition moves the farmer",
        );
        for (const cargo of ["wolf", "goat", "cabbage"] as const) {
          expectEqual(
            step.state.positions[cargo],
            cargo === input.passenger
              ? mirrorSide(current.positions[cargo])
              : current.positions[cargo],
            `L1 accepted transition locality for ${cargo}`,
          );
        }

        const reversed = stepRiver(step.state, input);
        expectEqual(
          reversed.state.positions,
          current.positions,
          "L1 reverse restores positions",
        );
        expectEqual(
          reversed.outcome,
          {
            type: "accepted",
            passenger: input.passenger,
            departure: step.outcome.arrival,
            arrival: step.outcome.departure,
          },
          "L1 reverse carries the exact accepted payload",
        );
      } else if (
        step.outcome.reason.type === "passenger-not-with-farmer"
      ) {
        passengerRefusals += 1;
        expectSame(step.state, current, "L1 passenger refusal stutters");
      } else {
        unsafeRefusals += 1;
        const key = step.outcome.reason.violations.join(",");
        unsafeProfiles.set(key, (unsafeProfiles.get(key) ?? 0) + 1);
        expectSame(step.state, current, "L1 unsafe refusal stutters");
      }

      if (step.outcome.type === "refused") {
        const repeated = stepRiver(current, input);
        expectEqual(
          repeated.outcome,
          step.outcome,
          "L1 repeated refusal has the same payload",
        );
        expectSame(
          repeated.state,
          current,
          "L1 repeated refusal does not accumulate state",
        );
      }

      const mirroredState = createRiverState(
        mirrorPositions(current.positions),
      );
      const mirroredStep = stepRiver(mirroredState, input);
      if (step.outcome.type === "accepted") {
        expectEqual(
          mirroredStep,
          {
            state: {
              positions: mirrorPositions(step.state.positions),
            },
            outcome: {
              type: "accepted",
              passenger: step.outcome.passenger,
              departure: mirrorSide(step.outcome.departure),
              arrival: mirrorSide(step.outcome.arrival),
            },
          },
          "L1 accepted transition mirrors",
        );
      } else {
        expectEqual(
          mirroredStep.outcome,
          step.outcome,
          "L1 refused outcome mirrors",
        );
        expectSame(
          mirroredStep.state,
          mirroredState,
          "L1 mirrored refusal stutters",
        );
      }
    });
  }

  expectEqual(accepted, 20, "L1 exhaustive accepted count");
  expectEqual(passengerRefusals, 10, "L1 exhaustive passenger refusal count");
  expectEqual(unsafeRefusals, 10, "L1 exhaustive unsafe refusal count");
  expectEqual(
    unsafeProfiles,
    new Map([
      ["wolf-with-goat", 4],
      ["goat-with-cabbage", 4],
      ["wolf-with-goat,goat-with-cabbage", 2],
    ]),
    "L1 exhaustive unsafe profiles",
  );

  function countSolvedAcceptedTraces(
    current: RiverState,
    remaining: number,
  ): number {
    if (remaining === 0) {
      return observeRiver(current).status === "solved" ? 1 : 0;
    }
    let count = 0;
    for (const input of CROSSINGS) {
      const step = stepRiver(current, input);
      if (step.outcome.type === "accepted") {
        count += countSolvedAcceptedTraces(step.state, remaining - 1);
      }
    }
    return count;
  }

  for (let crossings = 0; crossings < 7; crossings += 1) {
    expectEqual(
      countSolvedAcceptedTraces(initial, crossings),
      0,
      `L1 goal unreachable in ${crossings} accepted crossings`,
    );
  }
  expectEqual(
    countSolvedAcceptedTraces(initial, 7),
    2,
    "L1 exactly two seven-crossing solutions",
  );

  expectThrows(
    () =>
      void createRiverState({
        farmer: "right",
        wolf: "left",
        goat: "left",
        cabbage: "left",
      }),
    "L1 rejects unsafe state construction",
  );
}

// L2: canonical trace, invariants, replay, and additional required cases

const start = (task: string, attempt: bigint): WorkerCommand => ({
  type: "start",
  task,
  attempt,
});
const cancel = (task: string, attempt: bigint): WorkerCommand => ({
  type: "cancel",
  task,
  attempt,
});

const CANONICAL_SUPERVISOR_INPUTS: readonly SupervisorInput[] = [
  { type: "submit", task: "A" },
  { type: "submit", task: "B" },
  { type: "submit", task: "C" },
  { type: "submit", task: "D" },
  { type: "submit", task: "A" },
  { type: "progress", task: "A", attempt: 2n, value: 0.5 },
  { type: "cancel", task: "C" },
  { type: "retry", task: "C" },
  { type: "cancel", task: "B" },
  { type: "succeed", task: "B", attempt: 1n },
  { type: "fail", task: "A", attempt: 1n },
  { type: "retry", task: "A" },
  { type: "progress", task: "D", attempt: 1n, value: 0.75 },
  { type: "progress", task: "D", attempt: 1n, value: 0.5 },
  { type: "succeed", task: "D", attempt: 1n },
  { type: "progress", task: "A", attempt: 1n, value: 0.9 },
  { type: "progress", task: "A", attempt: 2n, value: 0.6 },
  { type: "progress", task: "A", attempt: 2n, value: 0.6 },
  { type: "fail", task: "A", attempt: 2n },
  { type: "retry", task: "A" },
  { type: "cancel", task: "A" },
  { type: "succeed", task: "A", attempt: 3n },
  { type: "progress", task: "C", attempt: 1n, value: 1 },
  { type: "succeed", task: "C", attempt: 1n },
  { type: "succeed", task: "C", attempt: 1n },
  { type: "retry", task: "C" },
];

const CANONICAL_SUPERVISOR_CLASSIFICATIONS = [
  "accepted",
  "accepted",
  "accepted",
  "accepted",
  "invalid",
  "invalid",
  "accepted",
  "accepted",
  "accepted",
  "stale",
  "accepted",
  "accepted",
  "accepted",
  "stale",
  "accepted",
  "stale",
  "accepted",
  "duplicate",
  "accepted",
  "accepted",
  "accepted",
  "stale",
  "accepted",
  "accepted",
  "duplicate",
  "invalid",
] as const;

const CANONICAL_SUPERVISOR_COMMANDS: readonly (readonly WorkerCommand[])[] = [
  [start("A", 1n)],
  [start("B", 1n)],
  [],
  [],
  [],
  [],
  [],
  [],
  [cancel("B", 1n), start("D", 1n)],
  [],
  [start("C", 1n)],
  [],
  [],
  [],
  [start("A", 2n)],
  [],
  [],
  [],
  [],
  [start("A", 3n)],
  [cancel("A", 3n)],
  [],
  [],
  [],
  [],
  [],
];

const CANONICAL_SUPERVISOR_STATES = [
  "A=r1@0|Q=",
  "A=r1@0,B=r1@0|Q=",
  "A=r1@0,B=r1@0,C=q0|Q=C",
  "A=r1@0,B=r1@0,C=q0,D=q0|Q=C,D",
  "A=r1@0,B=r1@0,C=q0,D=q0|Q=C,D",
  "A=r1@0,B=r1@0,C=q0,D=q0|Q=C,D",
  "A=r1@0,B=r1@0,C=c0,D=q0|Q=D",
  "A=r1@0,B=r1@0,C=q0,D=q0|Q=D,C",
  "A=r1@0,B=c1,C=q0,D=r1@0|Q=C",
  "A=r1@0,B=c1,C=q0,D=r1@0|Q=C",
  "A=f1,B=c1,C=r1@0,D=r1@0|Q=",
  "A=q1,B=c1,C=r1@0,D=r1@0|Q=A",
  "A=q1,B=c1,C=r1@0,D=r1@0.75|Q=A",
  "A=q1,B=c1,C=r1@0,D=r1@0.75|Q=A",
  "A=r2@0,B=c1,C=r1@0,D=s1|Q=",
  "A=r2@0,B=c1,C=r1@0,D=s1|Q=",
  "A=r2@0.6,B=c1,C=r1@0,D=s1|Q=",
  "A=r2@0.6,B=c1,C=r1@0,D=s1|Q=",
  "A=f2,B=c1,C=r1@0,D=s1|Q=",
  "A=r3@0,B=c1,C=r1@0,D=s1|Q=",
  "A=c3,B=c1,C=r1@0,D=s1|Q=",
  "A=c3,B=c1,C=r1@0,D=s1|Q=",
  "A=c3,B=c1,C=r1@1,D=s1|Q=",
  "A=c3,B=c1,C=s1,D=s1|Q=",
  "A=c3,B=c1,C=s1,D=s1|Q=",
  "A=c3,B=c1,C=s1,D=s1|Q=",
] as const;

function countRunning(tasks: ReadonlyMap<string, Task>): number {
  let count = 0;
  for (const task of tasks.values()) {
    if (task.phase.type === "running") {
      count += 1;
    }
  }
  return count;
}

function supervisorInvariantErrors(
  state: SupervisorState,
): readonly string[] {
  const errors: string[] = [];
  const runningCount = countRunning(state.tasks);
  const queueCounts = new Map<string, number>();

  if (runningCount > 2) {
    errors.push("more than two tasks are running");
  }

  for (const id of state.queue) {
    queueCounts.set(id, (queueCounts.get(id) ?? 0) + 1);
    const task = state.tasks.get(id);
    if (task === undefined || task.phase.type !== "queued") {
      errors.push(`queue contains non-queued task ${id}`);
    }
  }

  for (const [id, count] of queueCounts) {
    if (count !== 1) {
      errors.push(`queue contains ${id} ${count} times`);
    }
  }

  for (const [id, task] of state.tasks) {
    const queuedCount = queueCounts.get(id) ?? 0;
    if ((task.phase.type === "queued") !== (queuedCount === 1)) {
      errors.push(`task/queue membership disagrees for ${id}`);
    }
    if (task.started < 0n) {
      errors.push(`negative started ledger for ${id}`);
    }
    if (task.phase.type === "running") {
      if (task.phase.attempt <= 0n || task.phase.attempt !== task.started) {
        errors.push(`running attempt/ledger disagrees for ${id}`);
      }
      if (
        !Number.isFinite(task.phase.progress) ||
        task.phase.progress < 0 ||
        task.phase.progress > 1
      ) {
        errors.push(`invalid running progress for ${id}`);
      }
    }
  }

  if (state.queue.length !== 0 && runningCount !== 2) {
    errors.push("nonempty queue without full concurrency");
  }

  return errors;
}

function supervisorStateKey(state: SupervisorState): string {
  const tasks = [...state.tasks.entries()]
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([id, task]) => {
      switch (task.phase.type) {
        case "queued":
          return `${id}=q${task.started}`;
        case "running":
          return `${id}=r${task.phase.attempt}@${task.phase.progress}`;
        case "succeeded":
          return `${id}=s${task.started}`;
        case "failed":
          return `${id}=f${task.started}`;
        case "cancelled":
          return `${id}=c${task.started}`;
      }
    });
  return `${tasks.join(",")}|Q=${state.queue.join(",")}`;
}

function commandKey(command: WorkerCommand): string {
  return `${command.task}:${command.attempt}`;
}

function assertSupervisorObservation(
  state: SupervisorState,
  label: string,
): void {
  const before = supervisorStateKey(state);
  const expectedRunning = new Map<
    string,
    { readonly attempt: bigint; readonly progress: number }
  >();
  for (const [id, task] of state.tasks) {
    if (task.phase.type === "running") {
      expectedRunning.set(id, {
        attempt: task.phase.attempt,
        progress: task.phase.progress,
      });
    }
  }

  const observed = observeSupervisor(state);
  expectSame(observed.tasks, state.tasks, `${label}: observed task state`);
  expectSame(observed.queue, state.queue, `${label}: observed queue`);
  expectEqual(
    observed.running,
    expectedRunning,
    `${label}: running correlations`,
  );
  expectEqual(
    observed.availableCapacity,
    2 - expectedRunning.size,
    `${label}: available capacity`,
  );
  expectEqual(
    supervisorStateKey(state),
    before,
    `${label}: observation leaves state unchanged`,
  );
  expectEqual(
    observeSupervisor(state),
    observed,
    `${label}: observation is deterministic`,
  );
}

function expectIgnoredSupervisorInput(
  state: SupervisorState,
  input: SupervisorInput,
  classification: "duplicate" | "stale" | "invalid",
  label: string,
): void {
  const step = stepSupervisor(state, input);
  expectEqual(step.classification, classification, `${label}: classification`);
  expectSame(step.state, state, `${label}: state stutters`);
  expectEqual(step.commands, [], `${label}: no commands`);
}

function runSupervisorTrace(
  inputs: readonly SupervisorInput[],
): SupervisorState {
  let state = createSupervisor();
  const started = new Set<string>();
  const cancelled = new Set<string>();
  const commandTrace: WorkerCommand[] = [];
  assertSupervisorObservation(state, "L2 initial observation");

  inputs.forEach((input, index) => {
    const previousState = state;
    const step = stepSupervisor(state, input);
    expectEqual(
      step.classification,
      CANONICAL_SUPERVISOR_CLASSIFICATIONS[index],
      `L2 canonical classification ${index + 1}`,
    );
    expectEqual(
      step.commands,
      CANONICAL_SUPERVISOR_COMMANDS[index],
      `L2 canonical commands ${index + 1}`,
    );

    if (
      step.classification === "accepted" &&
      (input.type === "progress" ||
        input.type === "succeed" ||
        input.type === "fail")
    ) {
      const reportedTask = previousState.tasks.get(input.task);
      expectEqual(
        reportedTask?.phase.type,
        "running",
        `L2 accepted report addresses running task ${index + 1}`,
      );
      if (reportedTask?.phase.type === "running") {
        expectEqual(
          reportedTask.phase.attempt,
          input.attempt,
          `L2 accepted report matches correlation ${index + 1}`,
        );
      }
    }

    const startsThisStep = new Set<string>();
    for (const command of step.commands) {
      const key = commandKey(command);
      commandTrace.push(command);
      if (command.type === "start") {
        expectEqual(
          command.attempt > 0n,
          true,
          `L2 start correlation is positive ${key}`,
        );
        expectEqual(
          started.has(key),
          false,
          `L2 start correlation is globally unique ${key}`,
        );
        started.add(key);
        startsThisStep.add(key);
        const startedTask = step.state.tasks.get(command.task);
        expectEqual(
          startedTask?.started,
          command.attempt,
          `L2 start publishes its attempt ledger ${key}`,
        );
        expectEqual(
          startedTask?.phase,
          {
            type: "running",
            attempt: command.attempt,
            progress: 0,
          },
          `L2 start publishes its running correlation ${key}`,
        );
      } else {
        expectEqual(
          started.has(key),
          true,
          `L2 cancellation refers to a prior start ${key}`,
        );
        expectEqual(
          cancelled.has(key),
          false,
          `L2 cancellation is emitted at most once ${key}`,
        );
        cancelled.add(key);
        expectEqual(
          step.state.tasks.get(command.task)?.phase.type,
          "cancelled",
          `L2 cancellation publishes cancelled state ${key}`,
        );
      }
    }

    if (step.classification !== "accepted") {
      expectSame(step.state, state, `L2 canonical stutter ${index + 1}`);
    }
    state = step.state;

    for (const [id, task] of state.tasks) {
      const previousStarted = previousState.tasks.get(id)?.started ?? 0n;
      expectEqual(
        task.started >= previousStarted,
        true,
        `L2 attempt ledger never decreases for ${id}`,
      );
      const startKey = `${id}:${task.started}`;
      if (task.started > previousStarted) {
        expectEqual(
          task.started,
          previousStarted + 1n,
          `L2 attempt ledger increments once for ${id}`,
        );
        expectEqual(
          startsThisStep.has(startKey),
          true,
          `L2 ledger increments only with start ${startKey}`,
        );
      } else {
        expectEqual(
          startsThisStep.has(startKey),
          false,
          `L2 no start without ledger increment ${startKey}`,
        );
      }
    }

    expectEqual(
      supervisorStateKey(state),
      CANONICAL_SUPERVISOR_STATES[index],
      `L2 canonical state ${index + 1}`,
    );
    expectEqual(
      supervisorInvariantErrors(state),
      [],
      `L2 invariants after canonical step ${index + 1}`,
    );
    assertSupervisorObservation(
      state,
      `L2 observation after canonical step ${index + 1}`,
    );
  });

  expectEqual(
    [...started],
    ["A:1", "B:1", "D:1", "C:1", "A:2", "A:3"],
    "L2 globally unique start correlations",
  );
  expectEqual(
    [...cancelled],
    ["B:1", "A:3"],
    "L2 cancellation correlations",
  );
  expectEqual(
    commandTrace,
    CANONICAL_SUPERVISOR_COMMANDS.flat(),
    "L2 flattened command order",
  );
  return state;
}

{
  const finalState = runSupervisorTrace(CANONICAL_SUPERVISOR_INPUTS);
  expectEqual(finalState.queue, [], "L2 canonical final queue");
  expectEqual(
    [...finalState.tasks.entries()],
    [
      ["A", { phase: { type: "cancelled" }, started: 3n }],
      ["B", { phase: { type: "cancelled" }, started: 1n }],
      ["C", { phase: { type: "succeeded" }, started: 1n }],
      ["D", { phase: { type: "succeeded" }, started: 1n }],
    ],
    "L2 canonical final tasks",
  );
  expectEqual(
    observeSupervisor(finalState),
    {
      tasks: finalState.tasks,
      queue: [],
      running: new Map(),
      availableCapacity: 2,
    },
    "L2 canonical final observation",
  );

  const replayed = runSupervisorTrace(CANONICAL_SUPERVISOR_INPUTS);
  expectEqual(replayed, finalState, "L2 deterministic replay");

  const empty = createSupervisor();
  for (const [input, label] of [
    [{ type: "cancel", task: "unknown" }, "unknown cancellation"],
    [{ type: "retry", task: "unknown" }, "unknown retry"],
    [
      { type: "progress", task: "unknown", attempt: 1n, value: 0.5 },
      "unknown progress",
    ],
    [
      { type: "succeed", task: "unknown", attempt: 1n },
      "unknown success",
    ],
    [{ type: "fail", task: "unknown", attempt: 1n }, "unknown failure"],
  ] as const) {
    expectIgnoredSupervisorInput(empty, input, "invalid", `L2 ${label}`);
  }

  let state = createSupervisor();
  state = stepSupervisor(state, { type: "submit", task: "X" }).state;

  let step = stepSupervisor(state, {
    type: "progress",
    task: "X",
    attempt: 1n,
    value: 0,
  });
  expectEqual(
    step.classification,
    "duplicate",
    "L2 zero progress is duplicate",
  );
  expectSame(step.state, state, "L2 duplicate progress stutters");
  expectIgnoredSupervisorInput(
    state,
    { type: "progress", task: "X", attempt: 1n, value: -0 },
    "duplicate",
    "L2 signed zero is normalized zero",
  );

  for (const value of [
    -0.1,
    1.1,
    Number.NaN,
    Number.POSITIVE_INFINITY,
    Number.NEGATIVE_INFINITY,
  ]) {
    step = stepSupervisor(state, {
      type: "progress",
      task: "X",
      attempt: 1n,
      value,
    });
    expectEqual(
      step.classification,
      "invalid",
      `L2 rejects progress ${String(value)}`,
    );
    expectSame(step.state, state, "L2 invalid progress stutters");
    expectEqual(step.commands, [], "L2 invalid progress emits nothing");
  }

  step = stepSupervisor(state, {
    type: "progress",
    task: "X",
    attempt: 0n,
    value: 0.5,
  });
  expectEqual(step.classification, "invalid", "L2 rejects attempt zero");
  expectIgnoredSupervisorInput(
    state,
    { type: "succeed", task: "X", attempt: 0n },
    "invalid",
    "L2 rejects non-positive terminal attempt",
  );
  expectIgnoredSupervisorInput(
    state,
    { type: "fail", task: "X", attempt: 2n },
    "invalid",
    "L2 rejects future terminal attempt",
  );

  step = stepSupervisor(state, {
    type: "succeed",
    task: "X",
    attempt: 1n,
  });
  expectEqual(
    step.classification,
    "accepted",
    "L2 success below progress one",
  );
  state = step.state;
  expectEqual(
    stepSupervisor(state, {
      type: "fail",
      task: "X",
      attempt: 1n,
    }).classification,
    "stale",
    "L2 conflicting terminal report is stale",
  );
  expectEqual(
    stepSupervisor(state, {
      type: "succeed",
      task: "X",
      attempt: 1n,
    }).classification,
    "duplicate",
    "L2 repeated terminal report is duplicate",
  );
  expectEqual(
    stepSupervisor(state, { type: "cancel", task: "X" }).classification,
    "invalid",
    "L2 cancel after success is invalid",
  );
  expectIgnoredSupervisorInput(
    state,
    { type: "progress", task: "X", attempt: 1n, value: 0.5 },
    "stale",
    "L2 progress after settlement is stale",
  );
  expectIgnoredSupervisorInput(
    state,
    { type: "progress", task: "X", attempt: 2n, value: 0.5 },
    "invalid",
    "L2 future progress after settlement is invalid",
  );

  let failureAtOne = createSupervisor();
  failureAtOne = stepSupervisor(failureAtOne, {
    type: "submit",
    task: "Y",
  }).state;
  const progressAtOne = stepSupervisor(failureAtOne, {
    type: "progress",
    task: "Y",
    attempt: 1n,
    value: 1,
  });
  expectEqual(
    progressAtOne.state.tasks.get("Y")?.phase,
    { type: "running", attempt: 1n, progress: 1 },
    "L2 progress one does not complete",
  );
  failureAtOne = progressAtOne.state;
  const failed = stepSupervisor(failureAtOne, {
    type: "fail",
    task: "Y",
    attempt: 1n,
  });
  expectEqual(
    failed.classification,
    "accepted",
    "L2 failure at progress one is valid",
  );
  expectEqual(
    stepSupervisor(failed.state, {
      type: "fail",
      task: "Y",
      attempt: 1n,
    }).classification,
    "duplicate",
    "L2 repeated failure is duplicate",
  );
  expectEqual(
    stepSupervisor(failed.state, { type: "cancel", task: "Y" })
      .classification,
    "invalid",
    "L2 cancel after failure is invalid",
  );

  const retried = stepSupervisor(failed.state, { type: "retry", task: "Y" });
  expectEqual(retried.classification, "accepted", "L2 failed task retries");
  expectEqual(
    stepSupervisor(retried.state, {
      type: "fail",
      task: "Y",
      attempt: 1n,
    }).classification,
    "stale",
    "L2 prior duplicate becomes stale after retry",
  );
  expectEqual(
    stepSupervisor(retried.state, { type: "retry", task: "Y" })
      .classification,
    "invalid",
    "L2 retry while running is invalid",
  );

  let cancelled = createSupervisor();
  cancelled = stepSupervisor(cancelled, {
    type: "submit",
    task: "Z",
  }).state;
  cancelled = stepSupervisor(cancelled, {
    type: "cancel",
    task: "Z",
  }).state;
  step = stepSupervisor(cancelled, { type: "cancel", task: "Z" });
  expectEqual(
    step.classification,
    "duplicate",
    "L2 repeated cancellation",
  );
  expectSame(step.state, cancelled, "L2 duplicate cancellation stutters");
  expectIgnoredSupervisorInput(
    cancelled,
    { type: "progress", task: "Z", attempt: 1n, value: 0.5 },
    "stale",
    "L2 progress after cancellation is stale",
  );
  expectIgnoredSupervisorInput(
    cancelled,
    { type: "succeed", task: "Z", attempt: 1n },
    "stale",
    "L2 success after cancellation is stale",
  );
  expectIgnoredSupervisorInput(
    cancelled,
    { type: "fail", task: "Z", attempt: 2n },
    "invalid",
    "L2 future failure after cancellation is invalid",
  );

  expectEqual(
    stepSupervisor(createSupervisor(), {
      type: "progress",
      task: "unknown",
      attempt: 0n,
      value: Number.NaN,
    }).classification,
    "invalid",
    "L2 malformed unknown progress is invalid",
  );

  let precedence = createSupervisor();
  precedence = stepSupervisor(precedence, {
    type: "submit",
    task: "P",
  }).state;
  precedence = stepSupervisor(precedence, {
    type: "fail",
    task: "P",
    attempt: 1n,
  }).state;
  precedence = stepSupervisor(precedence, {
    type: "retry",
    task: "P",
  }).state;
  expectEqual(
    stepSupervisor(precedence, {
      type: "progress",
      task: "P",
      attempt: 1n,
      value: Number.NaN,
    }).classification,
    "invalid",
    "L2 invalid progress value precedes stale attempt",
  );

  let full = createSupervisor();
  full = stepSupervisor(full, { type: "submit", task: "R1" }).state;
  full = stepSupervisor(full, { type: "submit", task: "R2" }).state;
  full = stepSupervisor(full, { type: "submit", task: "Q" }).state;
  expectEqual(
    stepSupervisor(full, { type: "retry", task: "Q" }).classification,
    "invalid",
    "L2 retry while queued is invalid",
  );

  let tail = createSupervisor();
  tail = stepSupervisor(tail, { type: "submit", task: "R1" }).state;
  tail = stepSupervisor(tail, { type: "submit", task: "R2" }).state;
  tail = stepSupervisor(tail, { type: "submit", task: "C0" }).state;
  tail = stepSupervisor(tail, { type: "cancel", task: "C0" }).state;
  tail = stepSupervisor(tail, { type: "submit", task: "earlier" }).state;
  tail = stepSupervisor(tail, { type: "retry", task: "C0" }).state;
  expectEqual(
    tail.queue,
    ["earlier", "C0"],
    "L2 retry of never-started cancellation enters at FIFO tail",
  );
  expectEqual(
    tail.tasks.get("C0")?.started,
    0n,
    "L2 queueing and retry do not allocate attempts",
  );

  step = stepSupervisor(tail, { type: "cancel", task: "R1" });
  expectEqual(
    step.commands,
    [cancel("R1", 1n), start("earlier", 1n)],
    "L2 oldest queued task starts first",
  );
  tail = step.state;
  step = stepSupervisor(tail, { type: "cancel", task: "R2" });
  expectEqual(
    step.commands,
    [cancel("R2", 1n), start("C0", 1n)],
    "L2 retried never-started task keeps attempt one",
  );
}
