import assert from "node:assert/strict";
import { test } from "vitest";

import {
  SYSTEM_ACTOR_STORAGE_KEY,
  SYSTEM_STATE_EVENT,
  createSystemControls,
  type SystemControls,
} from "../system-controls.js";
import type { SystemActor, SystemState } from "../../protocol/types.js";

interface RecordedEvent {
  type: string;
  detail: SystemState;
}

function recordedEvent(detail: SystemState): Event {
  return { type: SYSTEM_STATE_EVENT, detail } as unknown as Event;
}

function harness() {
  const events: RecordedEvent[] = [];
  const stored = new Map<string, string>();
  let reloads = 0;
  const controls = createSystemControls({
    target: {
      dispatchEvent(event: Event) {
        events.push(event as unknown as RecordedEvent);
        return true;
      },
    },
    storage: {
      setItem(key: string, value: string) {
        stored.set(key, value);
      },
    },
    reload: () => {
      reloads += 1;
    },
    eventFactory: recordedEvent,
  });
  return {
    controls,
    events,
    stored,
    reloads: () => reloads,
  };
}

const ACTORS: SystemActor[] = [
  { id: "user-lena", username: "lena.holt", label: "Lena Holt" },
  { id: "user-mira", username: "mira.santos", label: "Mira Santos" },
];

function readyApplication(
  controls: SystemControls,
  actor = "user-mira",
): void {
  controls.starting({
    hasProvider: true,
    actor,
    actors: ACTORS,
  });
  controls.ready();
}

test("publishes defensive state snapshots", () => {
  const { controls, events } = harness();
  readyApplication(controls);

  const snapshotActor = controls.state.actors[0];
  assert.ok(snapshotActor);
  snapshotActor.label = "mutated getter";
  const event = events.at(-1);
  assert.ok(event);
  const eventActor = event.detail.actors[0];
  assert.ok(eventActor);
  eventActor.label = "mutated event";

  assert.equal(controls.state.actors[0]?.label, "Lena Holt");
  assert.equal(event.type, SYSTEM_STATE_EVENT);
  assert.deepEqual(controls.state, {
    status: "ready",
    hasProvider: true,
    actor: "user-mira",
    actors: ACTORS,
    canSwitchActor: true,
  });
});

test("represents a local session without inventing a selectable provider", () => {
  const { controls } = harness();
  controls.starting({ hasProvider: false });
  controls.ready();

  assert.deepEqual(controls.state, {
    status: "ready",
    hasProvider: false,
    actor: null,
    actors: [],
    canSwitchActor: false,
  });
});

test("stores a validated actor tab-locally and reloads without rewriting the URL", () => {
  const { controls, stored, reloads } = harness();
  readyApplication(controls);

  controls.setActor("lena.holt");

  assert.equal(stored.get(SYSTEM_ACTOR_STORAGE_KEY), "user-lena");
  assert.equal(stored.size, 1);
  assert.equal(reloads(), 1);
  assert.equal(controls.state.status, "starting");
  assert.equal(controls.state.actor, "user-lena");
});

test("rejects an unknown actor without storing, reloading, or losing the active actor", () => {
  const { controls, stored, reloads } = harness();
  readyApplication(controls);

  assert.throws(
    () => controls.setActor("not-seeded"),
    /unknown application actor/,
  );

  assert.equal(stored.size, 0);
  assert.equal(reloads(), 0);
  assert.equal(controls.state.status, "error");
  assert.equal(controls.state.actor, "user-mira");
  assert.match(controls.state.error ?? "", /not-seeded/);
  assert.equal(controls.state.canSwitchActor, true);
});

test("a failed initial actor can recover from provider-owned metadata", () => {
  const { controls, stored, reloads } = harness();
  controls.starting({
    hasProvider: true,
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

test("restart uses the Session's clean reload boundary", () => {
  const { controls, reloads } = harness();
  readyApplication(controls);

  controls.restart();

  assert.equal(reloads(), 1);
  assert.equal(controls.state.status, "starting");
});

test("restores the prior state if tab storage refuses an actor selection", () => {
  const events: RecordedEvent[] = [];
  let reloads = 0;
  const controls = createSystemControls({
    target: {
      dispatchEvent(event: Event) {
        events.push(event as unknown as RecordedEvent);
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
    eventFactory: recordedEvent,
  });
  readyApplication(controls);

  assert.throws(() => controls.setActor("user-lena"), /storage unavailable/);
  assert.equal(reloads, 0);
  assert.equal(controls.state.status, "error");
  assert.equal(controls.state.actor, "user-mira");
  assert.match(controls.state.error ?? "", /storage unavailable/);
  assert.equal(events.at(-1)?.detail.actor, "user-mira");
});
