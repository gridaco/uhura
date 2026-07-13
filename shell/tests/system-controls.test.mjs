import assert from "node:assert/strict";
import test from "node:test";

import {
  SYSTEM_ACTOR_STORAGE_KEY,
  SYSTEM_PROVIDER_STORAGE_KEY,
  SYSTEM_STATE_EVENT,
  createSystemControls,
} from "../system-controls.js";

function harness() {
  const events = [];
  const stored = new Map();
  let reloads = 0;
  const controls = createSystemControls({
    target: {
      dispatchEvent(event) {
        events.push(event);
        return true;
      },
    },
    storage: {
      setItem(key, value) {
        stored.set(key, value);
      },
    },
    reload: () => {
      reloads += 1;
    },
    eventFactory: (detail) => ({ type: SYSTEM_STATE_EVENT, detail }),
  });
  return {
    controls,
    events,
    stored,
    reloads: () => reloads,
  };
}

const ACTORS = [
  { id: "user-lena", username: "lena.holt", label: "Lena Holt" },
  { id: "user-mira", username: "mira.santos", label: "Mira Santos" },
];

function readyRemote(controls, actor = "user-mira") {
  controls.starting({
    provider: "remote",
    providers: ["remote", "fixture"],
    actor,
    actors: ACTORS,
  });
  controls.ready();
}

test("publishes defensive state snapshots", () => {
  const { controls, events } = harness();
  readyRemote(controls);

  const snapshot = controls.state;
  snapshot.actors[0].label = "mutated getter";
  const event = events.at(-1);
  event.detail.actors[0].label = "mutated event";

  assert.equal(controls.state.actors[0].label, "Lena Holt");
  assert.equal(event.type, SYSTEM_STATE_EVENT);
  assert.deepEqual(controls.state, {
    status: "ready",
    provider: "remote",
    providers: ["remote", "fixture"],
    actor: "user-mira",
    actors: ACTORS,
    canSwitchActor: true,
  });
});

test("stores a validated actor tab-locally and reloads without rewriting the URL", () => {
  const { controls, stored, reloads } = harness();
  readyRemote(controls);

  controls.setActor("lena.holt");

  assert.equal(stored.get(SYSTEM_ACTOR_STORAGE_KEY), "user-lena");
  assert.equal(stored.has(SYSTEM_PROVIDER_STORAGE_KEY), false);
  assert.equal(reloads(), 1);
  assert.equal(controls.state.status, "starting");
  assert.equal(controls.state.actor, "user-lena");
});

test("rejects an unknown actor without storing, reloading, or losing the active actor", () => {
  const { controls, stored, reloads } = harness();
  readyRemote(controls);

  assert.throws(() => controls.setActor("not-seeded"), /unknown auth actor/);

  assert.equal(stored.size, 0);
  assert.equal(reloads(), 0);
  assert.equal(controls.state.status, "error");
  assert.equal(controls.state.actor, "user-mira");
  assert.match(controls.state.error, /not-seeded/);
  assert.equal(controls.state.canSwitchActor, true);
});

test("a failed initial actor can recover from provider-owned metadata", () => {
  const { controls, stored, reloads } = harness();
  controls.starting({
    provider: "remote",
    providers: ["remote", "fixture"],
    actor: "typo",
  });
  controls.failed(new Error("actor `typo` is not a seeded user"), {
    actor: "typo",
    actors: ACTORS,
  });

  assert.equal(controls.state.canSwitchActor, true);
  controls.setActor("user-mira");
  assert.equal(stored.get(SYSTEM_ACTOR_STORAGE_KEY), "user-mira");
  assert.equal(reloads(), 1);
});

test("switches provider and restarts through clean reload boundaries", () => {
  const switched = harness();
  readyRemote(switched.controls);
  switched.controls.setProvider("fixture");
  assert.equal(switched.stored.get(SYSTEM_PROVIDER_STORAGE_KEY), "fixture");
  assert.equal(switched.stored.has(SYSTEM_ACTOR_STORAGE_KEY), false);
  assert.equal(switched.reloads(), 1);

  const restarted = harness();
  readyRemote(restarted.controls);
  restarted.controls.restart();
  assert.equal(restarted.reloads(), 1);
  assert.equal(restarted.controls.state.status, "starting");
});

test("restores the prior state if tab storage refuses a selection", () => {
  const events = [];
  let reloads = 0;
  const controls = createSystemControls({
    target: {
      dispatchEvent(event) {
        events.push(event);
        return true;
      },
    },
    storage: {
      setItem() {
        throw new Error("storage unavailable");
      },
    },
    reload: () => {
      reloads += 1;
    },
    eventFactory: (detail) => ({ type: SYSTEM_STATE_EVENT, detail }),
  });
  readyRemote(controls);

  assert.throws(() => controls.setActor("user-lena"), /storage unavailable/);
  assert.equal(reloads, 0);
  assert.equal(controls.state.status, "error");
  assert.equal(controls.state.actor, "user-mira");
  assert.match(controls.state.error, /storage unavailable/);
  assert.equal(events.at(-1).detail.actor, "user-mira");
});
