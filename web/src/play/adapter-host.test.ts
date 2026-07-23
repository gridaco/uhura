import assert from "node:assert/strict";
import { test } from "vitest";

import type {
  ResolvedInput,
  Value,
} from "../protocol/machine.js";
import type { DeliveryQueue } from "./adapter-host.js";
import {
  APPLICATION_PROVIDER_ADAPTER,
  createAdapterHost,
  createDeliveryQueue,
  WEB_HISTORY_ADAPTER,
} from "./adapter-host.js";
import { hash } from "../protocol/machine.js";

const text = (value: string): Value => ({ $: "Text", value });

const textOf = (input: ResolvedInput): string => {
  assert.equal(input.value.$, "Text");
  return input.value.value;
};

test("adapter deliveries are deferred, FIFO, and drained from snapshots", () => {
  const tasks: (() => void)[] = [];
  const delivered: string[] = [];
  let queue!: DeliveryQueue;
  queue = createDeliveryQueue(
    (input) => {
      const value = textOf(input);
      delivered.push(value);
      if (value === "first") {
        queue.enqueue({ source: "port", port: "router", value: text("later") });
      }
    },
    (task) => { tasks.push(task); },
  );

  queue.enqueue({ source: "port", port: "router", value: text("first") });
  queue.enqueue({ source: "port", port: "router", value: text("second") });
  assert.deepEqual(delivered, []);
  assert.equal(tasks.length, 1);

  tasks.shift()?.();
  assert.deepEqual(delivered, ["first", "second"]);
  assert.equal(tasks.length, 1);

  tasks.shift()?.();
  assert.deepEqual(delivered, ["first", "second", "later"]);
});

test("an admitted adapter receives commands in order and reports later inputs", () => {
  const contractHash = hash("0".repeat(64));
  const contractInstanceHash = hash("1".repeat(64));
  const tasks: (() => void)[] = [];
  const accepted: string[] = [];
  const delivered: ResolvedInput[] = [];
  const host = createAdapterHost({
    requirements: [{
      port: "router",
      adapter: WEB_HISTORY_ADAPTER,
      contractHash,
      contractInstanceHash,
    }],
    adapters: [{
      port: "router",
      adapter: WEB_HISTORY_ADAPTER,
      contractHash,
      contractInstanceHash,
      accept(command, context): void {
        assert.equal(command.$, "Text");
        accepted.push(command.value);
        context.deliver(text(`changed:${command.value}`));
      },
    }],
    deliver(input) {
      delivered.push(input);
    },
    schedule(task) {
      tasks.push(task);
    },
  });

  host.publish([
    { target: "port", port: "router", value: text("/orders") },
    { target: "port", port: "router", value: text("/returns") },
  ]);

  assert.deepEqual(accepted, ["/orders", "/returns"]);
  assert.deepEqual(delivered, []);
  assert.equal(tasks.length, 1);

  tasks.shift()?.();
  assert.deepEqual(delivered.map(textOf), [
    "changed:/orders",
    "changed:/returns",
  ]);
  host.dispose();
});

test("adapter readiness waits for async starts and reports their failures", async () => {
  const contractHash = hash("0".repeat(64));
  const contractInstanceHash = hash("1".repeat(64));
  const failure = new Error("adapter start failed");
  const errors: Array<{ error: unknown; port: string }> = [];
  let release!: () => void;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  const host = createAdapterHost({
    requirements: [{
      port: "authority",
      adapter: APPLICATION_PROVIDER_ADAPTER,
      contractHash,
      contractInstanceHash,
    }],
    adapters: [{
      port: "authority",
      adapter: APPLICATION_PROVIDER_ADAPTER,
      contractHash,
      contractInstanceHash,
      async start(): Promise<void> {
        await gate;
        throw failure;
      },
      accept() {},
    }],
    deliver() {},
    adapterError(error, port) {
      errors.push({ error, port });
    },
  });

  const readiness = host.start();
  assert.equal(host.start(), readiness);
  let settled = false;
  void readiness.then(() => {
    settled = true;
  });
  await Promise.resolve();
  assert.equal(settled, false);

  release();
  await readiness;
  assert.equal(settled, true);
  assert.deepEqual(errors, [{ error: failure, port: "authority" }]);
  host.dispose();
});

test("adapter admission is complete and contract checked", () => {
  const expected = hash("1".repeat(64));
  const incompatible = hash("2".repeat(64));
  const instance = hash("3".repeat(64));
  const incompatibleInstance = hash("4".repeat(64));

  assert.throws(
    () => createAdapterHost({
      requirements: [{
        port: "router",
        adapter: WEB_HISTORY_ADAPTER,
        contractHash: expected,
        contractInstanceHash: instance,
      }],
      adapters: [],
      deliver() {},
    }),
    /missing Uhura adapter/u,
  );

  assert.throws(
    () => createAdapterHost({
      requirements: [{
        port: "router",
        adapter: WEB_HISTORY_ADAPTER,
        contractHash: expected,
        contractInstanceHash: instance,
      }],
      adapters: [{
        port: "router",
        adapter: WEB_HISTORY_ADAPTER,
        contractHash: incompatible,
        contractInstanceHash: instance,
        accept() {},
      }],
      deliver() {},
    }),
    /incompatible admitted identity/u,
  );

  assert.throws(
    () => createAdapterHost({
      requirements: [{
        port: "router",
        adapter: WEB_HISTORY_ADAPTER,
        contractHash: expected,
        contractInstanceHash: instance,
      }],
      adapters: [{
        port: "router",
        adapter: WEB_HISTORY_ADAPTER,
        contractHash: expected,
        contractInstanceHash: incompatibleInstance,
        accept() {},
      }],
      deliver() {},
    }),
    /incompatible admitted identity/u,
  );

  assert.throws(
    () => createAdapterHost({
      requirements: [{
        port: "router",
        adapter: WEB_HISTORY_ADAPTER,
        contractHash: expected,
        contractInstanceHash: instance,
      }],
      adapters: [{
        port: "router",
        adapter: APPLICATION_PROVIDER_ADAPTER,
        contractHash: expected,
        contractInstanceHash: instance,
        accept() {},
      }],
      deliver() {},
    }),
    /incompatible admitted identity/u,
  );

  const compatible = {
    port: "router",
    adapter: WEB_HISTORY_ADAPTER,
    contractHash: expected,
    contractInstanceHash: instance,
    accept() {},
  } as const;
  assert.throws(
    () => createAdapterHost({
      requirements: [],
      adapters: [compatible],
      deliver() {},
    }),
    /undeclared Uhura adapter/u,
  );
  assert.throws(
    () => createAdapterHost({
      requirements: [{
        port: "router",
        adapter: WEB_HISTORY_ADAPTER,
        contractHash: expected,
        contractInstanceHash: instance,
      }],
      adapters: [compatible, compatible],
      deliver() {},
    }),
    /duplicate Uhura adapter/u,
  );
});
