import type { DevEvent } from "../protocol/types.js";

export type GenerationAction =
  | { kind: "none" }
  | { kind: "reload" }
  | { kind: "hide-diagnostics" }
  | { kind: "show-diagnostics"; diagnostics: Record<string, unknown> };

/**
 * Relates the Play artifacts that will actually boot to the live compiler
 * stream. Events can arrive before the artifact fetch completes, so the
 * newest one is retained until the artifact generation is known.
 */
export class PlayGenerationGate {
  #artifactGeneration: number | null = null;
  #artifactsUnavailable = false;
  #latestEvent: DevEvent | null = null;

  artifacts(generation: number): GenerationAction {
    this.#artifactGeneration = generation;
    this.#artifactsUnavailable = false;
    return this.#decide(this.#latestEvent);
  }

  /** The host had no last-good Play build (artifact HTTP 503). */
  unavailable(): GenerationAction {
    this.#artifactGeneration = null;
    this.#artifactsUnavailable = true;
    return this.#decide(this.#latestEvent);
  }

  event(event: DevEvent): GenerationAction {
    if (
      this.#latestEvent === null
      || event.generation >= this.#latestEvent.generation
    ) {
      this.#latestEvent = event;
    }
    return this.#decide(this.#latestEvent);
  }

  #decide(event: DevEvent | null): GenerationAction {
    if (event === null) {
      return { kind: "none" };
    }
    if (this.#artifactsUnavailable) {
      if (event.ok) return { kind: "reload" };
      if (event.diagnostics) {
        return {
          kind: "show-diagnostics",
          diagnostics: event.diagnostics,
        };
      }
      return { kind: "none" };
    }
    if (this.#artifactGeneration === null) return { kind: "none" };
    if (!event.ok && event.generation >= this.#artifactGeneration) {
      if (event.diagnostics) {
        return {
          kind: "show-diagnostics",
          diagnostics: event.diagnostics,
        };
      }
      return { kind: "none" };
    }
    if (event.generation > this.#artifactGeneration) {
      return { kind: "reload" };
    }
    return event.ok ? { kind: "hide-diagnostics" } : { kind: "none" };
  }
}
