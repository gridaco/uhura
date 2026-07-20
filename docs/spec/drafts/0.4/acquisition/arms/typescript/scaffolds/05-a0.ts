export type Reason =
  | { readonly type: "damaged" }
  | { readonly type: "not-needed" };
// TRIAL-TODO: add the controlled-change variant.

export interface ReasonState {
  readonly reason: Reason | null;
}

export type ReasonInput =
  | { readonly type: "choose-reason"; readonly reason: Reason }
  | { readonly type: "submit" };

export type ReasonCommand = {
  readonly type: "send";
  readonly reason: Reason;
};

export type ReasonClassification = "applied" | "submitted" | "blocked";

export interface ReasonStep {
  readonly state: ReasonState;
  readonly classification: ReasonClassification;
  readonly commands: readonly ReasonCommand[];
}

export interface ReasonObservation {
  readonly reason: Reason | null;
  readonly reasonComplete: boolean;
}

export function reasonComplete(reason: Reason | null): boolean {
  // TRIAL-TODO: after adding other, exact empty text is incomplete.
  return reason !== null;
}

export function stepReason(
  state: ReasonState,
  input: ReasonInput,
): ReasonStep {
  switch (input.type) {
    case "choose-reason":
      return {
        state: { reason: input.reason },
        classification: "applied",
        commands: [],
      };

    case "submit":
      if (state.reason === null || !reasonComplete(state.reason)) {
        return { state, classification: "blocked", commands: [] };
      }
      return {
        state,
        classification: "submitted",
        commands: [{ type: "send", reason: state.reason }],
      };
  }
}

export function observeReason(state: ReasonState): ReasonObservation {
  return {
    reason: state.reason,
    reasonComplete: reasonComplete(state.reason),
  };
}
