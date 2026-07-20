export type Side = "left" | "right";
export type Passenger = "predator" | "dependent" | "cargo";
export type Violation =
  | "predator-with-dependent"
  | "dependent-with-cargo";

export interface Position {
  readonly operator: Side;
  readonly predator: Side;
  readonly dependent: Side;
  readonly cargo: Side;
}

export interface CrossingState {
  readonly position: Position;
}

export interface CrossInput {
  readonly type: "cross";
  readonly passenger: Passenger | null;
}

export type CrossingOutcome =
  | {
      readonly type: "accepted";
      readonly passenger: Passenger | null;
      readonly departure: Side;
      readonly arrival: Side;
    }
  | {
      readonly type: "passenger-not-with-operator";
      readonly passenger: Passenger;
    }
  | {
      readonly type: "unsafe";
      readonly violations: readonly [Violation, ...Violation[]];
    };

export interface CrossingStep {
  readonly state: CrossingState;
  readonly outcome: CrossingOutcome;
  readonly commands: readonly [];
}

export interface CrossingObservation {
  readonly position: Position;
  readonly status: "in-progress" | "solved";
}

export const INITIAL_POSITION: Position = {
  operator: "left",
  predator: "left",
  dependent: "left",
  cargo: "left",
};

export function opposite(side: Side): Side {
  // TRIAL-TODO: return the other closed side.
  throw new Error("TRIAL-TODO");
}

export function safetyViolations(
  position: Position,
): readonly Violation[] {
  // TRIAL-TODO: return every violation in canonical declaration order.
  throw new Error("TRIAL-TODO");
}

export function stepCrossing(
  state: CrossingState,
  input: CrossInput,
): CrossingStep {
  // TRIAL-TODO: implement precedence, tentative movement, refusal, and
  // accepted publication.
  throw new Error("TRIAL-TODO");
}

export function observeCrossing(
  state: CrossingState,
): CrossingObservation {
  const position = state.position;
  return {
    position,
    status:
      position.operator === "right" &&
      position.predator === "right" &&
      position.dependent === "right" &&
      position.cargo === "right"
        ? "solved"
        : "in-progress",
  };
}
