// Host-owned controls for a running `uhura play` prototype. Changing a
// system setting intentionally reloads instead of trying to transplant a
// live Session: a hard reload is the lifecycle boundary that retires every
// timer, DOM listener, picker, and in-flight provider queue together. The
// selections are tab-local host state, not part of the app's URL contract.

export const SYSTEM_STATE_EVENT = "uhura:system-state";
export const SYSTEM_PROVIDER_STORAGE_KEY = "uhura:play:provider";
export const SYSTEM_ACTOR_STORAGE_KEY = "uhura:play:actor";

/** @typedef {"starting" | "ready" | "error"} SystemStatus */
/** @typedef {"remote" | "fixture"} ProviderMode */

/**
 * @typedef {Object} SystemActor
 * @property {string} id
 * @property {string} username
 * @property {string} label
 */

/**
 * @typedef {Object} SystemState
 * @property {SystemStatus} status
 * @property {ProviderMode | null} provider
 * @property {ProviderMode[]} providers
 * @property {string | null} actor
 * @property {SystemActor[]} actors
 * @property {boolean} canSwitchActor
 * @property {string} [error]
 */

/**
 * @typedef {Object} SystemInfo
 * @property {ProviderMode | null} [provider]
 * @property {ProviderMode[]} [providers]
 * @property {string | null} [actor]
 * @property {SystemActor[]} [actors]
 */

/** @param {unknown} error */
function errorMessage(error) {
  return error instanceof Error ? error.message : String(error);
}

/** @param {unknown} value @returns {ProviderMode[]} */
function providerModes(value) {
  if (!Array.isArray(value)) return [];
  /** @type {ProviderMode[]} */
  const modes = [];
  for (const candidate of value) {
    if (
      (candidate === "remote" || candidate === "fixture") &&
      !modes.includes(candidate)
    ) {
      modes.push(candidate);
    }
  }
  return modes;
}

/** @param {unknown} value @returns {SystemActor[]} */
function systemActors(value) {
  if (!Array.isArray(value)) return [];
  /** @type {SystemActor[]} */
  const actors = [];
  const seen = new Set();
  for (const candidate of value) {
    if (typeof candidate !== "object" || candidate === null) continue;
    const row = /** @type {Record<string, unknown>} */ (candidate);
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

/** @param {SystemState} state @returns {SystemState} */
function copyState(state) {
  const copy = {
    status: state.status,
    provider: state.provider,
    providers: [...state.providers],
    actor: state.actor,
    actors: state.actors.map((actor) => ({ ...actor })),
    canSwitchActor: state.canSwitchActor,
  };
  return state.error === undefined ? copy : { ...copy, error: state.error };
}

/**
 * @param {SystemState} previous
 * @param {SystemStatus} status
 * @param {SystemInfo} info
 * @param {string | undefined} error
 * @returns {SystemState}
 */
function mergeState(previous, status, info, error) {
  const provider = info.provider === undefined ? previous.provider : info.provider;
  const providers =
    info.providers === undefined ? [...previous.providers] : providerModes(info.providers);
  const actors = info.actors === undefined ? previous.actors : systemActors(info.actors);
  let actor = info.actor === undefined ? previous.actor : info.actor;
  if (typeof actor === "string") actor = actor.trim() || null;

  // A provider may be configured with a username, while its live metadata
  // names the normalized auth-table id. Expose one stable id to the shell.
  const current = actors.find(
    (candidate) => candidate.id === actor || candidate.username === actor,
  );
  if (current) actor = current.id;

  const remoteActors = provider === "remote" ? actors : [];
  const remoteActor = provider === "remote" ? actor : null;
  const canSwitchActor =
    provider === "remote" &&
    remoteActors.some((candidate) => candidate.id !== remoteActor);
  const next = {
    status,
    provider,
    providers,
    actor: remoteActor,
    actors: remoteActors,
    canSwitchActor,
  };
  return error === undefined ? next : { ...next, error };
}

/**
 * @param {{
 *   target: Pick<Window, "dispatchEvent">,
 *   storage: Pick<Storage, "setItem">,
 *   reload: () => void,
 *   eventFactory?: (detail: SystemState) => Event,
 * }} wiring
 */
export function createSystemControls({
  target,
  storage,
  reload,
  eventFactory = (detail) =>
    new CustomEvent(SYSTEM_STATE_EVENT, { detail: copyState(detail) }),
}) {
  /** @type {SystemState} */
  let state = {
    status: "starting",
    provider: null,
    providers: [],
    actor: null,
    actors: [],
    canSwitchActor: false,
  };

  /** @param {SystemState} next */
  function publish(next) {
    state = copyState(next);
    target.dispatchEvent(eventFactory(copyState(state)));
  }

  /**
   * @param {SystemStatus} status
   * @param {SystemInfo} [info]
   * @param {string} [error]
   */
  function transition(status, info = {}, error) {
    publish(mergeState(state, status, info, error));
  }

  /** @param {string} message @returns {never} */
  function rejectSelection(message) {
    transition("error", {}, message);
    throw new Error(message);
  }

  /**
   * Persist and reload after publishing the pending system state. If storage
   * or reload fails, restore the previous selection and surface the failure
   * instead of leaving the chrome on a phantom actor.
   * @param {SystemInfo} pending
   * @param {() => void} persist
   */
  function persistAndReload(pending, persist) {
    const previous = copyState(state);
    transition("starting", pending);
    try {
      persist();
      reload();
    } catch (error) {
      publish(mergeState(previous, "error", {}, errorMessage(error)));
      throw error;
    }
  }

  return {
    /** @returns {SystemState} */
    get state() {
      return copyState(state);
    },

    /** @param {SystemInfo} info */
    starting(info) {
      transition("starting", info);
    },

    /** @param {SystemInfo} [info] */
    ready(info = {}) {
      transition("ready", info);
    },

    /** @param {unknown} error @param {SystemInfo} [info] */
    failed(error, info = {}) {
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

    /** @param {string} requested */
    setActor(requested) {
      if (state.provider !== "remote") {
        rejectSelection("the fixture provider does not support auth actors");
      }
      const value = String(requested).trim();
      const actor = state.actors.find(
        (candidate) => candidate.id === value || candidate.username === value,
      );
      if (!actor) rejectSelection(`unknown auth actor \`${value}\``);
      if (actor.id === state.actor && state.status !== "error") return;
      persistAndReload({ actor: actor.id }, () => {
        storage.setItem(SYSTEM_ACTOR_STORAGE_KEY, actor.id);
      });
    },

    /** @param {ProviderMode} requested */
    setProvider(requested) {
      if (!state.providers.includes(requested)) {
        rejectSelection(`provider \`${requested}\` is not available`);
      }
      if (requested === state.provider && state.status !== "error") return;
      persistAndReload({ provider: requested }, () => {
        storage.setItem(SYSTEM_PROVIDER_STORAGE_KEY, requested);
      });
    },
  };
}
