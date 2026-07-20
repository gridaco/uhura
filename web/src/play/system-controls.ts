// Host-owned controls for a running `uhura play` prototype. Changing the
// selected application actor intentionally reloads instead of trying to
// transplant a live Session. The selection is tab-local host state, not part
// of the application's URL or deterministic machine state.

import type {
  SystemActor,
  SystemInfo,
  SystemState,
  SystemStatus,
} from "../protocol/types.js";

export type { SystemActor, SystemInfo, SystemState, SystemStatus };

export const SYSTEM_STATE_EVENT = "uhura:system-state";
export const SYSTEM_ACTOR_STORAGE_KEY = "uhura:play:actor";

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function systemActors(value: unknown): SystemActor[] {
  if (!Array.isArray(value)) return [];
  const actors: SystemActor[] = [];
  const seen = new Set<string>();
  for (const candidate of value) {
    if (typeof candidate !== "object" || candidate === null) continue;
    const row = candidate as Record<string, unknown>;
    const id = typeof row["id"] === "string" ? row["id"].trim() : "";
    const username =
      typeof row["username"] === "string" ? row["username"].trim() : "";
    if (id.length === 0 || username.length === 0 || seen.has(id)) continue;
    const authoredLabel =
      typeof row["label"] === "string" ? row["label"].trim() : "";
    actors.push({
      id,
      username,
      label: authoredLabel || `@${username}`,
    });
    seen.add(id);
  }
  return actors;
}

function copyState(state: SystemState): SystemState {
  const copy = {
    status: state.status,
    hasProvider: state.hasProvider,
    actor: state.actor,
    actors: state.actors.map((actor) => ({ ...actor })),
    canSwitchActor: state.canSwitchActor,
  };
  return state.error === undefined ? copy : { ...copy, error: state.error };
}

function mergeState(
  previous: SystemState,
  status: SystemStatus,
  info: SystemInfo,
  error?: string,
): SystemState {
  const hasProvider = info.hasProvider ?? previous.hasProvider;
  const actors = info.actors === undefined ? previous.actors : systemActors(info.actors);
  let actor = info.actor === undefined ? previous.actor : info.actor;
  if (typeof actor === "string") actor = actor.trim() || null;

  // An adapter provider may be configured with a username while its metadata
  // names the normalized authority ID. Expose one stable ID to the shell.
  const current = actors.find(
    (candidate) => candidate.id === actor || candidate.username === actor,
  );
  if (current) actor = current.id;

  const canSwitchActor = actors.some((candidate) => candidate.id !== actor);
  const next = {
    status,
    hasProvider,
    actor,
    actors,
    canSwitchActor,
  };
  return error === undefined ? next : { ...next, error };
}

interface SystemControlsWiring {
  target: Pick<Window, "dispatchEvent">;
  storage: Pick<Storage, "setItem">;
  reload: () => void;
  eventFactory?: (detail: SystemState) => Event;
}

export interface SystemControls {
  readonly state: SystemState;
  starting(info: SystemInfo): void;
  ready(info?: SystemInfo): void;
  failed(error: unknown, info?: SystemInfo): void;
  restart(): void;
  setActor(requested: string): void;
}

export function createSystemControls({
  target,
  storage,
  reload,
  eventFactory = (detail) =>
    new CustomEvent(SYSTEM_STATE_EVENT, { detail: copyState(detail) }),
}: SystemControlsWiring): SystemControls {
  let state: SystemState = {
    status: "starting",
    hasProvider: false,
    actor: null,
    actors: [],
    canSwitchActor: false,
  };

  function publish(next: SystemState): void {
    state = copyState(next);
    target.dispatchEvent(eventFactory(copyState(state)));
  }

  function transition(
    status: SystemStatus,
    info: SystemInfo = {},
    error?: string,
  ): void {
    publish(mergeState(state, status, info, error));
  }

  function rejectSelection(message: string): never {
    transition("error", {}, message);
    throw new Error(message);
  }

  function persistActorAndReload(actor: string): void {
    const previous = copyState(state);
    transition("starting", { actor });
    try {
      storage.setItem(SYSTEM_ACTOR_STORAGE_KEY, actor);
      reload();
    } catch (error) {
      publish(mergeState(previous, "error", {}, errorMessage(error)));
      throw error;
    }
  }

  return {
    get state() {
      return copyState(state);
    },

    starting(info: SystemInfo) {
      transition("starting", info);
    },

    ready(info: SystemInfo = {}) {
      transition("ready", info);
    },

    failed(error: unknown, info: SystemInfo = {}) {
      transition("error", info, errorMessage(error));
    },

    restart() {
      const previous = copyState(state);
      transition("starting");
      try {
        reload();
      } catch (error) {
        publish(mergeState(previous, "error", {}, errorMessage(error)));
        throw error;
      }
    },

    setActor(requested: string) {
      const value = String(requested).trim();
      const actor = state.actors.find(
        (candidate) => candidate.id === value || candidate.username === value,
      );
      if (!actor) rejectSelection(`unknown application actor \`${value}\``);
      if (actor.id === state.actor && state.status !== "error") return;
      persistActorAndReload(actor.id);
    },
  };
}
