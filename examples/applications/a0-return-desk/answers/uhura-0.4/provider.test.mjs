import assert from "node:assert/strict";

import { test } from "vitest";

import { createUhuraAdapters } from "./provider.mjs";

const MODULE = "app.returndesk@1";
const MACHINE = `${MODULE}::ReturnDesk`;

test("browser-local provider observes the exact checked port value identities", () => {
  const controller = new AbortController();
  const requirements = {
    orders: {
      port: "orders",
      adapter: "app.provider",
      contractHash: "orders-contract",
      contractInstanceHash: "orders-instance",
    },
    returns: {
      port: "returns",
      adapter: "app.provider",
      contractHash: "returns-contract",
      contractInstanceHash: "returns-instance",
    },
  };
  const provider = createUhuraAdapters(
    { return_id: "return-900" },
    {
      signal: controller.signal,
      pickFile: async () => null,
      port(name) {
        return requirements[name];
      },
    },
  );
  const orders = provider.adapters.find((adapter) => adapter.port === "orders");
  const returns = provider.adapters.find((adapter) => adapter.port === "returns");
  assert.ok(orders?.start);
  assert.ok(returns);

  const observations = [];
  orders.start({
    signal: controller.signal,
    deliver(value) {
      observations.push(value);
    },
  });
  assert.equal(observations.length, 1);
  assert.equal(observations[0]?.type, `${MACHINE}::port.orders.Receive`);

  const settlements = [];
  returns.accept(
    {
      $: "variant",
      type: `${MACHINE}::port.returns.Send`,
      case: "request",
      fields: [
        {
          name: "id",
          value: {
            $: "key",
            type: `${MODULE}::RequestId`,
            value: { $: "PositiveInt", value: "1" },
          },
        },
        { name: "payload", value: { $: "record", fields: [] } },
      ],
    },
    {
      signal: controller.signal,
      deliver(value) {
        settlements.push(value);
      },
    },
  );
  assert.equal(settlements.length, 1);
  assert.equal(settlements[0]?.type, `${MACHINE}::port.returns.Receive`);
  assert.equal(settlements[0]?.case, "returns.settled");
  assert.match(JSON.stringify(settlements[0]), /"case":"Accepted"/u);
});
