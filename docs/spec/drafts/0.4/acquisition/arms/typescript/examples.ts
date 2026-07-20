// Non-task example: one finite access registry.

export interface RegistryConfig {
  readonly capacity: bigint;
}

export interface RegistryState {
  readonly present: ReadonlySet<string>;
}

export type RegistryInput =
  | { readonly type: "enter"; readonly member: string }
  | { readonly type: "leave"; readonly member: string };

export type RegistryClassification = "applied" | "duplicate" | "invalid";

export interface RegistryStep {
  readonly state: RegistryState;
  readonly classification: RegistryClassification;
  readonly commands: readonly [];
}

export interface RegistryObservation {
  readonly presentCount: bigint;
  readonly full: boolean;
}

export function createRegistry(config: RegistryConfig): RegistryState {
  if (config.capacity <= 0n) {
    throw new RangeError("capacity must be positive");
  }
  return { present: new Set() };
}

export function stepRegistry(
  config: RegistryConfig,
  state: RegistryState,
  input: RegistryInput,
): RegistryStep {
  switch (input.type) {
    case "enter": {
      if (state.present.has(input.member)) {
        return { state, classification: "duplicate", commands: [] };
      }
      if (BigInt(state.present.size) === config.capacity) {
        return { state, classification: "invalid", commands: [] };
      }
      const present = new Set(state.present);
      present.add(input.member);
      return {
        state: { present },
        classification: "applied",
        commands: [],
      };
    }

    case "leave": {
      if (!state.present.has(input.member)) {
        return { state, classification: "invalid", commands: [] };
      }
      const present = new Set(state.present);
      present.delete(input.member);
      return {
        state: { present },
        classification: "applied",
        commands: [],
      };
    }
  }
}

export function observeRegistry(
  config: RegistryConfig,
  state: RegistryState,
): RegistryObservation {
  const presentCount = BigInt(state.present.size);
  return {
    presentCount,
    full: presentCount === config.capacity,
  };
}
