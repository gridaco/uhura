const MODULE = "app.return_desk.machine@1";
const MACHINE = `${MODULE}::ReturnDesk`;

const TYPES = Object.freeze({
  lineId: `${MODULE}::LineId`,
  orderId: `${MODULE}::OrderId`,
  ordersReceive: `${MACHINE}::port.orders.Receive`,
  returnsReceive: `${MACHINE}::port.returns.Receive`,
  returnsSend: `${MACHINE}::port.returns.Send`,
  settlement: `${MODULE}::Settlement`,
});

const text = (value) => ({ $: "Text", value });
const integer = (value) => ({ $: "Int", value: String(value) });
const key = (type, value) => ({ $: "key", type, value });
const field = (name, value) => ({ name, value });
const record = (fields) => ({ $: "record", fields });
const variant = (type, caseName, fields = []) => ({
  $: "variant",
  type,
  case: caseName,
  fields,
});

const orderLine = (
  id,
  title,
  purchasedQuantity,
  returnableQuantity,
  policySummary,
) =>
  record([
    field("id", key(TYPES.lineId, text(id))),
    field("title", text(title)),
    field("purchased_quantity", integer(purchasedQuantity)),
    field("returnable_quantity", integer(returnableQuantity)),
    field("policy_summary", text(policySummary)),
  ]);

const initialOrder = () =>
  variant(TYPES.ordersReceive, "orders.observed", [
    field(
      "value",
      record([
        field("id", key(TYPES.orderId, text("order-100"))),
        field("revision", integer(7)),
        field("lines", {
          $: "seq",
          items: [
            orderLine(
              "lamp",
              "Desk lamp",
              2,
              2,
              "Return the lamp in protective packaging.",
            ),
            orderLine(
              "mug",
              "Stoneware mug",
              1,
              1,
              "Wrap the mug to prevent breakage in transit.",
            ),
          ],
        }),
        field("allowed_methods", {
          $: "seq",
          items: [text("drop-off"), text("pickup")],
        }),
      ]),
    ),
  ]);

const exactTextConfig = (config, name, fallback) => {
  const value = config[name] ?? fallback;
  if (typeof value !== "string" || value.length === 0) {
    throw new TypeError(`Return Desk provider config.${name} must be nonempty text`);
  }
  return value;
};

const requiredVariant = (value, type, caseName, context) => {
  if (
    value?.$ !== "variant"
    || value.type !== type
    || value.case !== caseName
    || !Array.isArray(value.fields)
  ) {
    throw new TypeError(`${context} has an unexpected Uhura value`);
  }
  return value;
};

const requiredField = (value, name, context) => {
  const entry = value.fields.find((candidate) => candidate?.name === name);
  if (!entry || typeof entry !== "object" || !("value" in entry)) {
    throw new TypeError(`${context} is missing field \`${name}\``);
  }
  return entry.value;
};

const createOrdersAdapter = (host) => {
  const identity = host.port("orders");
  return {
    ...identity,
    start(context) {
      context.deliver(initialOrder());
    },
    accept() {
      throw new TypeError("Return Desk orders is an observation-only port");
    },
  };
};

const createReturnsAdapter = (host, returnId) => {
  const identity = host.port("returns");
  return {
    ...identity,
    accept(command, context) {
      const request = requiredVariant(
        command,
        TYPES.returnsSend,
        "returns.request",
        "Return Desk return command",
      );
      const requestId = requiredField(
        request,
        "id",
        "Return Desk return command",
      );
      requiredField(request, "payload", "Return Desk return command");
      context.deliver(
        variant(TYPES.returnsReceive, "returns.settled", [
          field("id", requestId),
          field(
            "result",
            variant(TYPES.settlement, "accepted", [
              field("return_id", text(returnId)),
            ]),
          ),
        ]),
      );
    },
  };
};

/**
 * A0's application-owned adapter assembly.
 *
 * `host.port()` supplies the exact checked identities. `context.deliver()`
 * crosses the adapter host's deferred FIFO queue, so neither startup
 * observations nor settlements can synchronously reenter a machine reaction.
 */
export function createUhuraAdapters(config, host) {
  const returnId = exactTextConfig(config, "return_id", "return-900");

  return {
    adapters: [
      createOrdersAdapter(host),
      createReturnsAdapter(host, returnId),
    ],
  };
}
