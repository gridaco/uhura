import assert from "node:assert/strict";
import { createHash } from "node:crypto";

const ORACLE_PROGRAM_HASH = "a0-return-desk-reference-oracle-v2";

const ORDER_ID = "order-100";
const FLOW_PREFIX = `/orders/${ORDER_ID}/return`;
const ORDER_URL = `/orders/${ORDER_ID}`;
const STEPS = new Set(["items", "method", "review"]);
const METHODS = new Set(["drop-off", "pickup"]);
const BASE_REASONS = new Set(["damaged", "not-needed"]);

export const order7 = Object.freeze({
  id: ORDER_ID,
  revision: 7,
  lines: Object.freeze([
    Object.freeze({
      id: "lamp",
      title: "Desk lamp",
      purchasedQuantity: 2,
      returnableQuantity: 2,
      policySummary: "Return the lamp in protective packaging.",
    }),
    Object.freeze({
      id: "mug",
      title: "Stoneware mug",
      purchasedQuantity: 1,
      returnableQuantity: 1,
      policySummary: "Wrap the mug to prevent breakage in transit.",
    }),
  ]),
  allowedMethods: Object.freeze(["drop-off", "pickup"]),
});

export const order8 = Object.freeze({
  id: ORDER_ID,
  revision: 8,
  lines: Object.freeze([
    Object.freeze({
      id: "lamp",
      title: "Desk lamp",
      purchasedQuantity: 2,
      returnableQuantity: 0,
      policySummary: "Return the lamp in protective packaging.",
    }),
    Object.freeze({
      id: "mug",
      title: "Stoneware mug",
      purchasedQuantity: 1,
      returnableQuantity: 1,
      policySummary: "Wrap the mug to prevent breakage in transit.",
    }),
  ]),
  allowedMethods: Object.freeze(["drop-off", "pickup"]),
});

export const order9WithoutLamp = Object.freeze({
  id: ORDER_ID,
  revision: 9,
  lines: Object.freeze([
    Object.freeze({
      id: "mug",
      title: "Stoneware mug",
      purchasedQuantity: 1,
      returnableQuantity: 1,
      policySummary: "Wrap the mug to prevent breakage in transit.",
    }),
  ]),
  allowedMethods: Object.freeze(["drop-off", "pickup"]),
});

function encodeOpaquePathSegment(value) {
  const encoded = encodeURIComponent(value);
  if (
    encoded === "" ||
    encoded === "." ||
    encoded === ".." ||
    encoded.startsWith("~")
  ) {
    return `~${Buffer.from(value, "utf8").toString("base64url")}`;
  }
  return encoded;
}

function decodeOpaquePathSegment(segment) {
  try {
    const value = segment.startsWith("~")
      ? Buffer.from(segment.slice(1), "base64url").toString("utf8")
      : decodeURIComponent(segment);
    return encodeOpaquePathSegment(value) === segment ? value : null;
  } catch {
    return null;
  }
}

export const urls = Object.freeze({
  items: `${FLOW_PREFIX}?step=items`,
  method: `${FLOW_PREFIX}?step=method`,
  review: `${FLOW_PREFIX}?step=review`,
  missingStep: FLOW_PREFIX,
  unknownStep: `${FLOW_PREFIX}?step=unknown`,
  order: ORDER_URL,
  receipt: (returnId) => `/returns/${encodeOpaquePathSegment(returnId)}`,
});

export function other(note) {
  return { kind: "other", note };
}

function clone(value) {
  return structuredClone(value);
}

function ownEnumerableDataKeys(value) {
  try {
    if (
      value === null ||
      typeof value !== "object" ||
      Array.isArray(value) ||
      ![Object.prototype, null].includes(Object.getPrototypeOf(value))
    ) {
      return null;
    }

    const keys = Reflect.ownKeys(value);
    if (keys.some((key) => typeof key !== "string")) return null;
    for (const key of keys) {
      const descriptor = Object.getOwnPropertyDescriptor(value, key);
      if (
        descriptor === undefined ||
        !Object.hasOwn(descriptor, "value") ||
        descriptor.enumerable !== true
      ) {
        return null;
      }
    }
    return keys;
  } catch {
    return null;
  }
}

function hasExactOwnKeys(value, keys) {
  const actual = ownEnumerableDataKeys(value);
  if (actual === null) return false;
  const sortedActual = [...actual].sort();
  const expected = [...keys].sort();
  return (
    sortedActual.length === expected.length &&
    sortedActual.every((key, index) => key === expected[index])
  );
}

function isDataRecord(value) {
  return ownEnumerableDataKeys(value) !== null;
}

function isDenseDataArray(value) {
  try {
    if (!Array.isArray(value)) return false;
    const keys = Reflect.ownKeys(value);
    if (keys.some((key) => typeof key !== "string")) return false;
    if (keys.length !== value.length + 1 || !keys.includes("length")) {
      return false;
    }
    const indexKeys = keys.filter((key) => key !== "length");
    for (const key of indexKeys) {
      if (!/^(0|[1-9]\d*)$/.test(key)) return false;
      const index = Number(key);
      if (
        !Number.isSafeInteger(index) ||
        index < 0 ||
        index >= value.length ||
        String(index) !== key
      ) {
        return false;
      }
      const descriptor = Object.getOwnPropertyDescriptor(value, key);
      if (
        descriptor === undefined ||
        !Object.hasOwn(descriptor, "value") ||
        descriptor.enumerable !== true
      ) {
        return false;
      }
    }
    const length = Object.getOwnPropertyDescriptor(value, "length");
    return (
      length !== undefined &&
      Object.hasOwn(length, "value") &&
      length.enumerable === false
    );
  } catch {
    return false;
  }
}

function isClosedDataTree(value, seen = new Set()) {
  if (
    value === null ||
    typeof value === "string" ||
    typeof value === "boolean"
  ) {
    return true;
  }
  if (typeof value === "number") {
    return Number.isFinite(value) &&
      (!Number.isInteger(value) || Number.isSafeInteger(value));
  }
  if (typeof value === "bigint") return true;
  if (typeof value !== "object" || seen.has(value)) return false;
  seen.add(value);

  if (Array.isArray(value)) {
    if (!isDenseDataArray(value)) return false;
    return value.every((item) => isClosedDataTree(item, seen));
  }

  const keys = ownEnumerableDataKeys(value);
  if (keys === null) return false;
  return keys.every((key) => isClosedDataTree(value[key], seen));
}

function isSafePositiveInteger(value) {
  return Number.isSafeInteger(value) && value > 0;
}

function isSafeNonNegativeInteger(value) {
  return Number.isSafeInteger(value) && value >= 0;
}

function isPositiveCounter(value) {
  return (
    isSafePositiveInteger(value) ||
    (typeof value === "bigint" && value > 0n)
  );
}

function counterBigInt(value) {
  assert.equal(isPositiveCounter(value), true);
  return typeof value === "bigint" ? value : BigInt(value);
}

function incrementCounter(value) {
  const next = counterBigInt(value) + 1n;
  return next <= BigInt(Number.MAX_SAFE_INTEGER)
    ? Number(next)
    : next;
}

function sameCounter(left, right) {
  return (
    isPositiveCounter(left) &&
    isPositiveCounter(right) &&
    counterBigInt(left) === counterBigInt(right)
  );
}

function isUnicodeScalarText(value) {
  if (typeof value !== "string") return false;

  for (let index = 0; index < value.length; index += 1) {
    const unit = value.charCodeAt(index);
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const next = value.charCodeAt(index + 1);
      if (!(next >= 0xdc00 && next <= 0xdfff)) return false;
      index += 1;
    } else if (unit >= 0xdc00 && unit <= 0xdfff) {
      return false;
    }
  }
  return true;
}

function compareText(left, right) {
  return Buffer.compare(
    Buffer.from(left, "utf8"),
    Buffer.from(right, "utf8"),
  );
}

function canonical(value) {
  if (value === null) return ["null"];
  if (typeof value === "boolean") return ["boolean", value];
  if (typeof value === "string") return ["string", value];
  if (typeof value === "number") return ["number", value];
  if (typeof value === "bigint") return ["bigint", value.toString()];
  if (Array.isArray(value)) {
    return ["array", value.map(canonical)];
  }
  if (value && typeof value === "object") {
    return [
      "record",
      Object.keys(value)
        .sort(compareText)
        .map((key) => [key, canonical(value[key])]),
    ];
  }
  throw new TypeError("unsupported canonical value");
}

function same(a, b) {
  return JSON.stringify(canonical(a)) === JSON.stringify(canonical(b));
}

function digest(value) {
  return createHash("sha256")
    .update(JSON.stringify(canonical(value)))
    .digest("hex");
}

function result(kind, reason = null) {
  return reason === null ? kind : `${kind}(${reason})`;
}

function accepted() {
  return "accepted";
}

function blocked(reason) {
  return result("blocked", reason);
}

function invalid(reason) {
  return result("invalid", reason);
}

function routeFor(url) {
  if (!isUnicodeScalarText(url)) return null;
  if (url === ORDER_URL) return { kind: "order" };

  const receipt = /^\/returns\/([^/?#]+)$/.exec(url);
  if (receipt) {
    const returnId = decodeOpaquePathSegment(receipt[1]);
    if (!isUnicodeScalarText(returnId) || returnId.length === 0) return null;
    return { kind: "receipt", returnId };
  }

  const flow = /^\/orders\/order-100\/return(?:\?([^#]*))?$/.exec(url);
  if (!flow) return null;

  const params = new URLSearchParams(flow[1] ?? "");
  const step = params.has("step") ? params.get("step") : null;
  return {
    kind: "flow",
    step: STEPS.has(step) ? step : null,
    rawStep: step,
  };
}

function canonicalOrder(order) {
  return {
    id: order.id,
    revision: order.revision,
    lines: [...order.lines]
      .map((line) => ({
        id: line.id,
        title: line.title,
        purchasedQuantity: line.purchasedQuantity,
        returnableQuantity: line.returnableQuantity,
        policySummary: line.policySummary,
      }))
      .sort((a, b) => compareText(a.id, b.id)),
    allowedMethods: [...order.allowedMethods].sort(compareText),
  };
}

function validOrder(order) {
  try {
    if (
      !hasExactOwnKeys(order, [
        "id",
        "revision",
        "lines",
        "allowedMethods",
      ])
    ) {
      return false;
    }
    if (order.id !== ORDER_ID) return false;
    if (!isSafePositiveInteger(order.revision)) return false;
    if (
      !isDenseDataArray(order.lines) ||
      !isDenseDataArray(order.allowedMethods)
    ) {
      return false;
    }

    const lineIds = new Set();
    for (const line of order.lines) {
      if (
        !hasExactOwnKeys(line, [
          "id",
          "title",
          "purchasedQuantity",
          "returnableQuantity",
          "policySummary",
        ])
      ) {
        return false;
      }
      if (!isUnicodeScalarText(line.id)) return false;
      if (lineIds.has(line.id)) return false;
      lineIds.add(line.id);
      if (!isUnicodeScalarText(line.title)) return false;
      if (!isUnicodeScalarText(line.policySummary)) return false;
      if (!isSafeNonNegativeInteger(line.purchasedQuantity)) return false;
      if (!isSafeNonNegativeInteger(line.returnableQuantity)) return false;
      if (line.returnableQuantity > line.purchasedQuantity) return false;
    }

    const methods = new Set();
    for (const method of order.allowedMethods) {
      if (!METHODS.has(method) || methods.has(method)) return false;
      methods.add(method);
    }
    return true;
  } catch {
    return false;
  }
}

function reasonValid(reason) {
  if (BASE_REASONS.has(reason)) return true;
  return (
    hasExactOwnKeys(reason, ["kind", "note"]) &&
    reason.kind === "other" &&
    isUnicodeScalarText(reason.note)
  );
}

function reasonComplete(reason) {
  if (BASE_REASONS.has(reason)) return true;
  return reasonValid(reason) && reason.kind === "other" && reason.note.length > 0;
}

function lineOf(order, lineId) {
  return order?.lines.find((line) => line.id === lineId) ?? null;
}

function blankDraft(revision) {
  return {
    orderId: ORDER_ID,
    baseRevision: revision,
    selections: {},
    method: null,
  };
}

function isBlankDraft(draft) {
  return (
    draft !== null &&
    Object.keys(draft.selections).length === 0 &&
    draft.method === null
  );
}

function selectionOf(draft, lineId) {
  if (
    draft === null ||
    !Object.hasOwn(draft.selections, lineId)
  ) {
    return null;
  }
  return draft.selections[lineId];
}

function putSelection(draft, lineId, selection) {
  Object.defineProperty(draft.selections, lineId, {
    value: selection,
    enumerable: true,
    configurable: true,
    writable: true,
  });
}

function putOwn(record, key, value) {
  Object.defineProperty(record, key, {
    value,
    enumerable: true,
    configurable: true,
    writable: true,
  });
}

function snapshotFromDraft(draft, requestId) {
  return {
    requestId,
    orderId: draft.orderId,
    expectedRevision: draft.baseRevision,
    selections: Object.entries(draft.selections)
      .map(([lineId, selection]) => ({
        lineId,
        quantity: selection.quantity,
        reason: clone(selection.reason),
      }))
      .sort((a, b) => compareText(a.lineId, b.lineId)),
    method: draft.method,
  };
}

function requestSnapshotValid(snapshot) {
  if (
    !hasExactOwnKeys(snapshot, [
      "requestId",
      "orderId",
      "expectedRevision",
      "selections",
      "method",
    ])
  ) {
    return false;
  }
  const number = requestNumber(snapshot.requestId);
  if (number === null || snapshot.requestId !== `request-${number}`) {
    return false;
  }
  if (snapshot.orderId !== ORDER_ID) return false;
  if (
    !Number.isSafeInteger(snapshot.expectedRevision) ||
    snapshot.expectedRevision <= 0 ||
    !isDenseDataArray(snapshot.selections) ||
    snapshot.selections.length === 0 ||
    !METHODS.has(snapshot.method)
  ) {
    return false;
  }

  const ids = [];
  for (const selection of snapshot.selections) {
    if (
      !hasExactOwnKeys(selection, [
        "lineId",
        "quantity",
        "reason",
      ]) ||
      !isUnicodeScalarText(selection.lineId) ||
      !Number.isSafeInteger(selection.quantity) ||
      selection.quantity <= 0 ||
      !reasonComplete(selection.reason)
    ) {
      return false;
    }
    ids.push(selection.lineId);
  }
  if (new Set(ids).size !== ids.length) return false;
  return same(ids, [...ids].sort(compareText));
}

function requestNumber(id) {
  if (typeof id !== "string") return null;
  const match = /^request-(0|[1-9]\d*)$/.exec(id);
  if (match === null) return null;
  const value = BigInt(match[1]);
  return value > 0n ? value : null;
}

function surfaceNumber(id) {
  if (typeof id !== "string") return null;
  const match = /^surface-(0|[1-9]\d*)$/.exec(id);
  if (match === null) return null;
  const value = BigInt(match[1]);
  return value > 0n ? value : null;
}

function routeScopeNumber(id) {
  if (typeof id !== "string") return null;
  const match = /^scope-(0|[1-9]\d*)$/.exec(id);
  if (match === null) return null;
  const value = BigInt(match[1]);
  return value > 0n ? value : null;
}

export class ReturnDeskOracle {
  constructor(state = null) {
    this.state =
      state === null
        ? {
            location: null,
            order: null,
            draft: null,
            phase: { kind: "idle" },
            revisionFence: null,
            completedReturn: null,
            receiptAccess: null,
            routeScope: null,
            nextRouteScope: 1,
            surface: null,
            nextSurface: 1,
            navigationIntent: null,
            nextRequest: 1,
            settledRequests: [],
          }
        : clone(state);
    this._assertInvariants();
  }

  snapshot() {
    return clone(this.state);
  }

  checkpoint(provenance) {
    assert.equal(
      isClosedDataTree(provenance),
      true,
      "checkpoint provenance is closed serializable data",
    );
    const state = this.snapshot();
    return {
      format: 1,
      programHash: ORACLE_PROGRAM_HASH,
      stateHash: digest(state),
      state,
      provenance: clone(provenance),
    };
  }

  static restore(checkpoint) {
    assert.equal(
      hasExactOwnKeys(checkpoint, [
        "format",
        "programHash",
        "stateHash",
        "state",
        "provenance",
      ]),
      true,
      "checkpoint envelope",
    );
    assert.equal(
      isClosedDataTree(checkpoint),
      true,
      "checkpoint is closed serializable data",
    );
    assert.equal(checkpoint?.format, 1, "checkpoint format");
    assert.equal(
      checkpoint?.programHash,
      ORACLE_PROGRAM_HASH,
      "checkpoint program",
    );
    assert.equal(
      checkpoint?.stateHash,
      digest(checkpoint?.state),
      "checkpoint state hash",
    );
    return new ReturnDeskOracle(checkpoint.state);
  }

  presentation() {
    const normalization = this._normalizationTarget();
    const route = this.state.location?.route ?? null;
    const offeredReturn =
      this.state.receiptAccess === "offered"
        ? this.state.completedReturn
        : null;
    const activeStep =
      route?.kind === "flow" && normalization === null ? route.step : null;
    const conflict = this._conflict();
    const surfaceLine =
      this.state.surface === null
        ? null
        : lineOf(this.state.order, this.state.surface.lineId);

    return {
      deliveredLocation: this.state.location?.url ?? null,
      activeStep,
      normalizing: normalization !== null,
      orderStatus:
        this.state.order === null
          ? { kind: "waiting" }
          : { kind: "available", order: clone(this.state.order) },
      draft: clone(this.state.draft),
      conflict,
      itemsComplete: this._itemsComplete(),
      methodComplete: this._methodComplete(),
      submission: clone(this.state.phase),
      policySurface:
        this.state.surface === null || surfaceLine === null
          ? null
          : {
              lineId: surfaceLine.id,
              title: surfaceLine.title,
              policySummary: surfaceLine.policySummary,
            },
      completionNotice:
        offeredReturn === null
          ? null
          : {
              returnId: offeredReturn,
              receipt: urls.receipt(offeredReturn),
            },
      receipt:
        route?.kind === "receipt" &&
        route.returnId === this.state.completedReturn
          ? { returnId: route.returnId, status: "completed" }
          : null,
      actions: this._actionAvailability(),
    };
  }

  inspect() {
    return {
      ...this.snapshot(),
      presentation: this.presentation(),
    };
  }

  deliverLocation(url) {
    return this._atomic("location", (ctx) => {
      const route = routeFor(url);
      if (route === null) return invalid("location");
      if (
        route.kind === "receipt" &&
        route.returnId !== this.state.completedReturn
      ) {
        return invalid("location");
      }
      if (this.state.location?.url === url) return "duplicate";

      this.state.location = { url, route };
      this.state.navigationIntent = null;
      this.state.routeScope = null;
      this.state.surface = null;
      ctx.locationChanged = true;

      if (
        route.kind === "flow" &&
        this.state.order !== null &&
        this.state.draft === null &&
        this.state.completedReturn === null
      ) {
        this.state.draft = blankDraft(this.state.order.revision);
      }

      if (
        route.kind === "receipt" &&
        route.returnId === this.state.completedReturn
      ) {
        this.state.receiptAccess = "acknowledged";
      } else if (
        this.state.completedReturn !== null &&
        route.kind === "flow" &&
        this.state.receiptAccess !== "acknowledged"
      ) {
        this.state.receiptAccess = "redirecting";
      } else if (
        this.state.completedReturn !== null &&
        this.state.receiptAccess === "redirecting" &&
        route.kind !== "flow"
      ) {
        this.state.receiptAccess = "offered";
      }

      return accepted();
    });
  }

  deliverOrder(order) {
    return this._atomic("order", () => {
      if (!validOrder(order)) return invalid("order");
      const normalized = canonicalOrder(order);

      if (this.state.order !== null) {
        if (normalized.revision < this.state.order.revision) return "stale";
        if (normalized.revision === this.state.order.revision) {
          return same(normalized, this.state.order)
            ? "duplicate"
            : invalid("equal-revision-order");
        }
      }

      this.state.order = normalized;

      if (
        this.state.surface !== null &&
        lineOf(normalized, this.state.surface.lineId) === null
      ) {
        this.state.surface = null;
      }

      if (
        this.state.location?.route.kind === "flow" &&
        this.state.draft === null &&
        this.state.completedReturn === null
      ) {
        this.state.draft = blankDraft(normalized.revision);
      }

      return accepted();
    });
  }

  chooseQuantity(lineId, quantity) {
    return this._atomic("choose-quantity", () => {
      if (!Number.isSafeInteger(quantity)) return invalid("quantity");

      const orderLine = lineOf(this.state.order, lineId);
      if (orderLine === null) {
        return invalid("unknown-line");
      }
      if (quantity < 0 || quantity > orderLine.returnableQuantity) {
        return invalid("quantity");
      }

      const current = selectionOf(this.state.draft, lineId);
      if (
        (quantity === 0 && current === null) ||
        (quantity > 0 && current?.quantity === quantity)
      ) {
        return "duplicate";
      }

      const editBlock = this._editBlock("items");
      if (editBlock !== null) return blocked(editBlock);

      if (quantity === 0) {
        delete this.state.draft.selections[lineId];
      } else {
        putSelection(this.state.draft, lineId, {
          quantity,
          reason: current?.reason ?? null,
        });
      }
      this._clearRetryablePhaseAfterEdit();
      return accepted();
    });
  }

  chooseReason(lineId, reason) {
    return this._atomic("choose-reason", () => {
      if (!reasonValid(reason)) return invalid("reason");

      const orderLine = lineOf(this.state.order, lineId);
      if (orderLine === null) {
        return invalid("unknown-line");
      }

      const selection = selectionOf(this.state.draft, lineId);
      if (selection === null) return invalid("unselected-line");
      if (same(selection.reason, reason)) return "duplicate";

      const editBlock = this._editBlock("items");
      if (editBlock !== null) return blocked(editBlock);

      selection.reason = clone(reason);
      this._clearRetryablePhaseAfterEdit();
      return accepted();
    });
  }

  chooseMethod(method) {
    return this._atomic("choose-method", () => {
      if (!METHODS.has(method)) return invalid("method");
      if (
        this.state.order !== null &&
        !this.state.order.allowedMethods.includes(method)
      ) {
        return invalid("disallowed-method");
      }
      if (this.state.draft?.method === method) return "duplicate";

      const editBlock = this._editBlock("method");
      if (editBlock !== null) return blocked(editBlock);
      if (!this._itemsComplete()) return blocked("items-incomplete");

      this.state.draft.method = method;
      this._clearRetryablePhaseAfterEdit();
      return accepted();
    });
  }

  goToStep(step) {
    return this._atomic("go-to-step", (ctx) => {
      if (!STEPS.has(step)) return invalid("step");
      const target = urls[step];
      const navigationCheck = this._checkUserNavigation(target);
      if (navigationCheck !== null) return navigationCheck;

      const current = this._activeStep();
      if (current === null) return blocked("wrong-location");
      if (!this._stepAdmissible(step)) return blocked("incomplete-prerequisite");
      if (this._conflict() !== null && this._rank(step) > this._rank(current)) {
        return blocked("source-changed");
      }
      this._emitNavigation(ctx, "push", target);
      return accepted();
    });
  }

  openPolicy(lineId) {
    return this._atomic("open-policy", () => {
      const line = lineOf(this.state.order, lineId);
      if (line === null) return invalid("unknown-line");
      if (
        this.state.surface !== null &&
        this.state.surface.lineId === lineId
      ) {
        return "duplicate";
      }
      if (this._activeStep() !== "items" || this.state.routeScope === null) {
        return blocked(
          this._normalizationTarget() === null
            ? "wrong-location"
            : "normalizing",
        );
      }
      const id = `surface-${this.state.nextSurface}`;
      this.state.nextSurface = incrementCounter(this.state.nextSurface);
      this.state.surface = {
        id,
        lineId,
        ownerScope: this.state.routeScope,
      };
      return accepted();
    });
  }

  dismissPolicy(surfaceId) {
    return this._atomic("dismiss-policy", () => {
      if (
        this.state.surface === null ||
        this.state.surface.id !== surfaceId ||
        this.state.surface.ownerScope !== this.state.routeScope
      ) {
        return "stale";
      }
      this.state.surface = null;
      return accepted();
    });
  }

  restartFromCurrentOrder() {
    return this._atomic("restart", () => {
      if (this.state.phase.kind === "pending") return blocked("pending");
      if (this._activeStep() !== "items") {
        return blocked(
          this._normalizationTarget() === null
            ? "wrong-location"
            : "normalizing",
        );
      }
      if (this.state.order === null) return blocked("order-waiting");
      if (
        this.state.revisionFence !== null &&
        this.state.order.revision < this.state.revisionFence
      ) {
        return blocked("revision-fence");
      }
      if (
        this.state.draft !== null &&
        this.state.draft.baseRevision === this.state.order.revision &&
        isBlankDraft(this.state.draft) &&
        this.state.revisionFence === null &&
        this.state.phase.kind === "idle"
      ) {
        return "duplicate";
      }
      if (this.state.completedReturn !== null) return blocked("completed");

      this.state.draft = blankDraft(this.state.order.revision);
      this.state.revisionFence = null;
      this.state.phase = { kind: "idle" };
      return accepted();
    });
  }

  submit() {
    return this._atomic("submit", (ctx) => {
      if (this.state.phase.kind === "pending") return "duplicate";
      if (this.state.completedReturn !== null) return blocked("completed");
      if (this._activeStep() !== "review") {
        return blocked(
          this._normalizationTarget() === null
            ? "wrong-location"
            : "normalizing",
        );
      }
      if (this._conflict() !== null) return blocked("source-changed");
      if (!this._methodComplete()) return blocked("incomplete");
      if (
        this.state.phase.kind !== "idle" &&
        this.state.phase.kind !== "unavailable"
      ) {
        return blocked("submission-phase");
      }
      const requestId = `request-${this.state.nextRequest}`;
      this.state.nextRequest = incrementCounter(this.state.nextRequest);
      const snapshot = snapshotFromDraft(this.state.draft, requestId);
      this.state.phase = { kind: "pending", snapshot: clone(snapshot) };
      ctx.consequences.push({ kind: "submit-return", snapshot: clone(snapshot) });
      return accepted();
    });
  }

  settle(requestId, settlement) {
    return this._atomic("settlement", (ctx) => {
      if (
        !hasExactOwnKeys(settlement, ["kind"]) &&
        !hasExactOwnKeys(settlement, ["kind", "returnId"]) &&
        !hasExactOwnKeys(settlement, ["kind", "currentRevision"])
      ) {
        return invalid("settlement");
      }

      if (
        !["accepted", "refused", "unavailable"].includes(settlement.kind)
      ) {
        return invalid("settlement");
      }

      if (
        (settlement.kind === "accepted" &&
          !hasExactOwnKeys(settlement, ["kind", "returnId"])) ||
        (settlement.kind === "refused" &&
          !hasExactOwnKeys(settlement, ["kind", "currentRevision"])) ||
        (settlement.kind === "unavailable" &&
          !hasExactOwnKeys(settlement, ["kind"]))
      ) {
        return invalid("settlement");
      }

      if (
        settlement.kind === "accepted" &&
        (!isUnicodeScalarText(settlement.returnId) ||
          settlement.returnId.length === 0)
      ) {
        return invalid("return-id");
      }

      if (
        settlement.kind === "refused" &&
        !isSafePositiveInteger(settlement.currentRevision)
      ) {
        return invalid("refusal-revision");
      }

      if (requestNumber(requestId) === null) {
        return invalid("request-id");
      }

      if (
        this.state.phase.kind !== "pending" ||
        this.state.phase.snapshot.requestId !== requestId
      ) {
        return "stale";
      }

      const pending = this.state.phase.snapshot;
      if (settlement.kind === "accepted") {
        this.state.settledRequests.push(requestId);
        this.state.completedReturn = settlement.returnId;
        this.state.draft = null;
        this.state.revisionFence = null;
        this.state.phase = {
          kind: "completed",
          requestId,
          returnId: settlement.returnId,
        };

        const route = this.state.location?.route ?? null;
        if (
          route?.kind === "receipt" &&
          route.returnId === settlement.returnId
        ) {
          this.state.receiptAccess = "acknowledged";
        } else if (route?.kind === "flow") {
          this.state.receiptAccess = "redirecting";
        } else {
          this.state.receiptAccess = "offered";
        }
        return accepted();
      }

      if (settlement.kind === "unavailable") {
        this.state.settledRequests.push(requestId);
        this.state.phase = {
          kind: "unavailable",
          request: clone(pending),
        };
        return accepted();
      }

      if (
        settlement.currentRevision <= pending.expectedRevision
      ) {
        return invalid("refusal-revision");
      }
      this.state.settledRequests.push(requestId);
      this.state.revisionFence = settlement.currentRevision;
      this.state.phase = {
        kind: "refused",
        request: clone(pending),
        currentRevision: settlement.currentRevision,
      };
      return accepted();
    });
  }

  followOrderLink() {
    return this._atomic("follow-order-link", (ctx) => {
      const route = this.state.location?.route ?? null;
      const admittedOwner =
        route?.kind === "flow" ||
        (route?.kind === "receipt" &&
          route.returnId === this.state.completedReturn);
      if (!admittedOwner) return blocked("wrong-location");

      const navigationCheck = this._checkUserNavigation(urls.order);
      if (navigationCheck !== null) return navigationCheck;

      this._emitNavigation(ctx, "push", urls.order);
      return accepted();
    });
  }

  followReceiptLink() {
    return this._atomic("follow-receipt-link", (ctx) => {
      if (
        this.state.receiptAccess !== "offered" ||
        this.state.completedReturn === null
      ) {
        return blocked("no-completion-notice");
      }
      const route = this.state.location?.route ?? null;
      if (route?.kind !== "order") return blocked("wrong-location");

      const target = urls.receipt(this.state.completedReturn);
      const navigationCheck = this._checkUserNavigation(target);
      if (navigationCheck !== null) return navigationCheck;

      this._emitNavigation(ctx, "push", target);
      return accepted();
    });
  }

  _actionAvailability() {
    const activeStep = this._activeStep();
    const itemEditable = this._editBlock("items") === null;
    const methodEditable =
      this._editBlock("method") === null && this._itemsComplete();
    const canGo = (step) => {
      const target = urls[step];
      if (this._checkUserNavigation(target) !== null) return false;
      const current = this._activeStep();
      if (current === null || !this._stepAdmissible(step)) return false;
      if (
        this._conflict() !== null &&
        this._rank(step) > this._rank(current)
      ) {
        return false;
      }
      return true;
    };

    const chooseQuantity = {};
    const chooseReason = {};
    const openPolicy = {};
    for (const line of this.state.order?.lines ?? []) {
      putOwn(
        chooseQuantity,
        line.id,
        itemEditable && line.returnableQuantity > 0,
      );
      putOwn(
        chooseReason,
        line.id,
        itemEditable &&
          selectionOf(this.state.draft, line.id) !== null,
      );
      putOwn(
        openPolicy,
        line.id,
        activeStep === "items" &&
          this.state.routeScope !== null &&
          this.state.surface?.lineId !== line.id,
      );
    }

    const chooseMethod = {};
    for (const method of METHODS) {
      chooseMethod[method] =
        methodEditable &&
        this.state.order?.allowedMethods.includes(method) === true &&
        this.state.draft?.method !== method;
    }

    const route = this.state.location?.route ?? null;
    const followOrderOwner =
      route?.kind === "flow" ||
      (route?.kind === "receipt" &&
        route.returnId === this.state.completedReturn);
    const noticeTarget =
      this.state.receiptAccess === "offered" &&
      this.state.completedReturn !== null
        ? urls.receipt(this.state.completedReturn)
        : null;
    const fenceSatisfied =
      this.state.revisionFence === null ||
      (this.state.order !== null &&
        this.state.order.revision >= this.state.revisionFence);
    const restartWouldBeDuplicate =
      this.state.draft !== null &&
      this.state.order !== null &&
      this.state.draft.baseRevision === this.state.order.revision &&
      isBlankDraft(this.state.draft) &&
      this.state.revisionFence === null &&
      this.state.phase.kind === "idle";

    return {
      chooseQuantity,
      chooseReason,
      chooseMethod,
      goToStep: {
        items: canGo("items"),
        method: canGo("method"),
        review: canGo("review"),
      },
      openPolicy,
      dismissPolicy:
        this.state.surface === null
          ? {}
          : { [this.state.surface.id]: true },
      restart:
        activeStep === "items" &&
        this.state.order !== null &&
        this.state.phase.kind !== "pending" &&
        this.state.completedReturn === null &&
        fenceSatisfied &&
        !restartWouldBeDuplicate,
      submit:
        activeStep === "review" &&
        this._conflict() === null &&
        this._methodComplete() &&
        ["idle", "unavailable"].includes(this.state.phase.kind),
      followOrder:
        followOrderOwner &&
        this._checkUserNavigation(urls.order) === null,
      followReceipt:
        noticeTarget !== null &&
        route?.kind !== "flow" &&
        this._checkUserNavigation(noticeTarget) === null,
    };
  }

  _atomic(label, handler) {
    const before = this.snapshot();
    const ctx = { consequences: [], locationChanged: false };
    try {
      const stepResult = handler(ctx);

      if (stepResult === "accepted") {
        this._reconcile(ctx);
      } else {
        this.state = before;
        ctx.consequences = [];
        assert.deepEqual(
          this.state,
          before,
          `${label}: non-accepted step must roll back`,
        );
      }

      this._assertInvariants();
      return {
        result: stepResult,
        consequences: clone(ctx.consequences),
        state: this.snapshot(),
        presentation: this.presentation(),
      };
    } catch (error) {
      this.state = before;
      ctx.consequences = [];
      throw error;
    }
  }

  _reconcile(ctx) {
    const target = this._normalizationTarget();
    const route = this.state.location?.route ?? null;

    if (target !== null) {
      this.state.routeScope = null;
      this.state.surface = null;
      if (
        this.state.navigationIntent?.kind !== "required-replace" ||
        this.state.navigationIntent?.target !== target
      ) {
        this._emitNavigation(ctx, "replace", target, true);
      }
      return;
    }

    if (route?.kind === "flow") {
      if (ctx.locationChanged && this.state.routeScope === null) {
        this.state.routeScope = `scope-${this.state.nextRouteScope}`;
        this.state.nextRouteScope =
          incrementCounter(this.state.nextRouteScope);
      }
      return;
    }

    this.state.routeScope = null;
    this.state.surface = null;
  }

  _emitNavigation(ctx, mode, target, supersede = false) {
    const kind =
      mode === "push" ? "user-push" : "required-replace";
    if (
      !supersede &&
      this.state.navigationIntent !== null &&
      (this.state.navigationIntent.kind !== kind ||
        this.state.navigationIntent.target !== target)
    ) {
      throw new Error("user navigation cannot overwrite an outstanding intent");
    }
    this.state.navigationIntent = { kind, target };
    ctx.consequences.push({ kind: "navigate", mode, target });
  }

  _checkUserNavigation(target) {
    if (this.state.location?.url === target) return "duplicate";
    if (this.state.navigationIntent?.target === target) return "duplicate";
    if (this.state.navigationIntent !== null) {
      return blocked("navigation-pending");
    }
    if (this._normalizationTarget() !== null) return blocked("normalizing");
    return null;
  }

  _normalizationTarget() {
    const route = this.state.location?.route ?? null;
    if (route === null) return null;

    if (this.state.completedReturn !== null) {
      if (route.kind === "flow") {
        return urls.receipt(this.state.completedReturn);
      }
      return null;
    }

    if (route.kind !== "flow") return null;
    if (route.step === null) return urls.items;
    if (route.step === "items") return null;
    if (!this._itemsComplete()) return urls.items;
    if (route.step === "review" && !this._methodComplete()) return urls.method;
    return null;
  }

  _activeStep() {
    const route = this.state.location?.route ?? null;
    if (
      route?.kind !== "flow" ||
      route.step === null ||
      this._normalizationTarget() !== null
    ) {
      return null;
    }
    return route.step;
  }

  _stepAdmissible(step) {
    if (step === "items") return true;
    if (step === "method") return this._itemsComplete();
    return this._methodComplete();
  }

  _rank(step) {
    return { items: 0, method: 1, review: 2 }[step];
  }

  _itemsComplete() {
    const { order, draft } = this.state;
    if (order === null || draft === null) return false;
    if (draft.baseRevision !== order.revision) return false;
    const selections = Object.entries(draft.selections);
    if (selections.length === 0) return false;

    for (const [lineId, selection] of selections) {
      const line = lineOf(order, lineId);
      if (line === null) return false;
      if (
        !isSafePositiveInteger(selection.quantity) ||
        selection.quantity > line.returnableQuantity
      ) {
        return false;
      }
      if (!reasonComplete(selection.reason)) return false;
    }
    return true;
  }

  _methodComplete() {
    return (
      this._itemsComplete() &&
      this.state.draft.method !== null &&
      this.state.order.allowedMethods.includes(this.state.draft.method)
    );
  }

  _conflict() {
    if (
      this.state.draft !== null &&
      this.state.order !== null &&
      this.state.draft.baseRevision !== this.state.order.revision
    ) {
      return {
        kind: "source-changed",
        draftRevision: this.state.draft.baseRevision,
        observedRevision: this.state.order.revision,
        fence: this.state.revisionFence,
      };
    }
    if (this.state.revisionFence !== null) {
      return {
        kind: "source-changed",
        draftRevision: this.state.draft?.baseRevision ?? null,
        observedRevision: this.state.order?.revision ?? null,
        fence: this.state.revisionFence,
      };
    }
    return null;
  }

  _editBlock(requiredStep) {
    if (this._normalizationTarget() !== null) return "normalizing";
    if (this._activeStep() !== requiredStep) return "wrong-location";
    if (this.state.order === null || this.state.draft === null) {
      return "order-waiting";
    }
    if (this.state.phase.kind === "pending") return "pending";
    if (
      this.state.phase.kind === "completed" ||
      this.state.completedReturn !== null
    ) {
      return "completed";
    }
    if (this._conflict() !== null) return "source-changed";
    return null;
  }

  _clearRetryablePhaseAfterEdit() {
    if (
      this.state.phase.kind === "unavailable" ||
      this.state.phase.kind === "refused"
    ) {
      this.state.phase = { kind: "idle" };
    }
  }

  _assertInvariants() {
    const s = this.state;

    assert.equal(
      hasExactOwnKeys(s, [
        "location",
        "order",
        "draft",
        "phase",
        "revisionFence",
        "completedReturn",
        "receiptAccess",
        "routeScope",
        "nextRouteScope",
        "surface",
        "nextSurface",
        "navigationIntent",
        "nextRequest",
        "settledRequests",
      ]),
      true,
      "coordinator state has its exact closed shape",
    );
    assert.equal(isDenseDataArray(s.settledRequests), true);
    assert.equal(
      isPositiveCounter(s.nextRequest),
      true,
      "request allocator is an exact positive counter",
    );
    assert.equal(
      isPositiveCounter(s.nextSurface),
      true,
      "surface allocator is an exact positive counter",
    );
    assert.equal(
      isPositiveCounter(s.nextRouteScope),
      true,
      "route-scope allocator is an exact positive counter",
    );

    assert.equal(
      ["idle", "pending", "refused", "unavailable", "completed"].includes(
        s.phase?.kind,
      ),
      true,
      "submission phase is closed",
    );
    const phaseKeys = {
      idle: ["kind"],
      pending: ["kind", "snapshot"],
      refused: ["kind", "request", "currentRevision"],
      unavailable: ["kind", "request"],
      completed: ["kind", "requestId", "returnId"],
    };
    assert.equal(
      hasExactOwnKeys(s.phase, phaseKeys[s.phase.kind]),
      true,
      "submission phase has its exact closed shape",
    );

    if (s.location !== null) {
      assert.equal(hasExactOwnKeys(s.location, ["url", "route"]), true);
      assert.equal(typeof s.location.url, "string");
      const decoded = routeFor(s.location.url);
      assert.notEqual(decoded, null);
      assert.equal(
        hasExactOwnKeys(s.location.route, Object.keys(decoded)),
        true,
      );
      assert.deepEqual(
        s.location.route,
        decoded,
        "decoded route is derived from the delivered URL",
      );
      if (s.location.route.kind === "receipt") {
        assert.equal(
          s.location.route.returnId,
          s.completedReturn,
          "only the retained completed receipt is admitted",
        );
      }
    }

    if (s.order !== null) {
      assert.equal(validOrder(s.order), true);
      assert.deepEqual(
        s.order,
        canonicalOrder(s.order),
        "stored order is canonical",
      );
    }
    if (s.draft !== null) {
      assert.equal(
        hasExactOwnKeys(s.draft, [
          "orderId",
          "baseRevision",
          "selections",
          "method",
        ]),
        true,
      );
      assert.equal(isDataRecord(s.draft.selections), true);
      assert.notEqual(s.order, null, "a draft requires an accepted order copy");
      assert.equal(s.draft.orderId, ORDER_ID);
      assert.equal(
        isSafePositiveInteger(s.draft.baseRevision),
        true,
      );
      assert.equal(
        s.draft.baseRevision <= s.order.revision,
        true,
        "a draft cannot be based on an unobserved future revision",
      );
      assert.equal(
        s.draft.method === null || METHODS.has(s.draft.method),
        true,
        "draft method is closed",
      );
      for (const [lineId, selection] of Object.entries(s.draft.selections)) {
        assert.equal(
          hasExactOwnKeys(selection, ["quantity", "reason"]),
          true,
        );
        assert.equal(
          isUnicodeScalarText(lineId),
          true,
        );
        assert.equal(
          isSafePositiveInteger(selection.quantity),
          true,
        );
        assert.equal(
          selection.reason === null || reasonValid(selection.reason),
          true,
        );
        if (s.draft.baseRevision === s.order.revision) {
          const currentLine = lineOf(s.order, lineId);
          assert.notEqual(
            currentLine,
            null,
            "a current-revision draft cannot select an unknown line",
          );
          assert.equal(
            selection.quantity <= currentLine.returnableQuantity,
            true,
            "a current-revision selection respects current quantity",
          );
        }
      }
      if (
        s.draft.baseRevision === s.order.revision &&
        s.draft.method !== null
      ) {
        assert.equal(
          s.order.allowedMethods.includes(s.draft.method),
          true,
          "a current-revision method is currently allowed",
        );
      }
    }
    if (
      s.location?.route.kind === "flow" &&
      s.order !== null &&
      s.completedReturn === null
    ) {
      assert.notEqual(
        s.draft,
        null,
        "an observed order in an incomplete flow owns a draft",
      );
    }

    if (s.phase.kind === "pending") {
      assert.notEqual(s.draft, null);
      assert.equal(requestSnapshotValid(s.phase.snapshot), true);
      assert.deepEqual(
        s.phase.snapshot,
        snapshotFromDraft(s.draft, s.phase.snapshot.requestId),
        "pending snapshot is the canonical retained draft snapshot",
      );
    }

    if (s.phase.kind === "refused") {
      assert.equal(requestSnapshotValid(s.phase.request), true);
      assert.equal(
        isSafePositiveInteger(s.phase.currentRevision),
        true,
      );
      assert.equal(
        s.phase.currentRevision > s.phase.request.expectedRevision,
        true,
        "refusal fence is newer than the submitted revision",
      );
      assert.notEqual(s.draft, null);
      assert.deepEqual(
        s.phase.request,
        snapshotFromDraft(s.draft, s.phase.request.requestId),
        "refusal preserves the submitted draft snapshot",
      );
      assert.equal(s.revisionFence, s.phase.currentRevision);
      assert.equal(
        s.settledRequests.includes(s.phase.request.requestId),
        true,
      );
    } else {
      assert.equal(s.revisionFence, null);
    }

    if (s.phase.kind === "unavailable") {
      assert.equal(requestSnapshotValid(s.phase.request), true);
      assert.notEqual(s.draft, null);
      assert.deepEqual(
        s.phase.request,
        snapshotFromDraft(s.draft, s.phase.request.requestId),
        "unavailability preserves the submitted draft snapshot",
      );
      assert.equal(
        s.settledRequests.includes(s.phase.request.requestId),
        true,
      );
    }

    if (s.phase.kind === "completed") {
      assert.equal(
        isUnicodeScalarText(s.phase.returnId) &&
          s.phase.returnId.length > 0,
        true,
      );
      assert.equal(s.completedReturn, s.phase.returnId);
      assert.equal(s.settledRequests.includes(s.phase.requestId), true);
      assert.equal(s.draft, null);
      assert.notEqual(
        s.location,
        null,
        "completion retains the location from which its request was submitted",
      );
      assert.equal(
        ["redirecting", "offered", "acknowledged"].includes(
          s.receiptAccess,
        ),
        true,
        "completed receipt access has a closed lifecycle",
      );
    } else {
      assert.equal(s.completedReturn, null);
      assert.equal(s.receiptAccess, null);
    }

    if (s.phase.kind === "completed") {
      const route = s.location.route;
      if (s.receiptAccess === "redirecting") {
        assert.equal(route.kind, "flow");
      } else if (s.receiptAccess === "offered") {
        assert.equal(
          route.kind === "receipt" &&
            route.returnId === s.completedReturn,
          false,
        );
      }
      if (
        route.kind === "receipt" &&
        route.returnId === s.completedReturn
      ) {
        assert.equal(s.receiptAccess, "acknowledged");
      }
    }

    if (s.surface !== null) {
      assert.equal(
        hasExactOwnKeys(s.surface, ["id", "lineId", "ownerScope"]),
        true,
      );
      assert.equal(s.routeScope !== null, true);
      assert.equal(s.surface.ownerScope, s.routeScope);
      assert.notEqual(lineOf(s.order, s.surface.lineId), null);
      assert.equal(this._activeStep(), "items");
      const surfaceId = surfaceNumber(s.surface.id);
      assert.notEqual(surfaceId, null);
      assert.equal(
        sameCounter(incrementCounter(surfaceId), s.nextSurface),
        true,
        "the open surface is the latest allocated surface",
      );
    }

    if (s.routeScope !== null) {
      const scopeId = routeScopeNumber(s.routeScope);
      assert.notEqual(scopeId, null);
      assert.equal(
        sameCounter(incrementCounter(scopeId), s.nextRouteScope),
        true,
        "the current scope is the latest allocated route scope",
      );
      assert.equal(s.location?.route.kind, "flow");
      assert.equal(this._normalizationTarget(), null);
    }

    const normalizationTarget = this._normalizationTarget();
    if (normalizationTarget !== null) {
      assert.equal(s.routeScope, null);
      assert.equal(s.surface, null);
      assert.deepEqual(
        s.navigationIntent,
        {
          kind: "required-replace",
          target: normalizationTarget,
        },
        "normalizing state retains its required replace intent",
      );
    } else if (s.location?.route.kind === "flow") {
      assert.notEqual(
        s.routeScope,
        null,
        "an admissible flow location owns a route scope",
      );
    }

    for (const [index, requestId] of s.settledRequests.entries()) {
      const number = requestNumber(requestId);
      assert.notEqual(number, null);
      assert.equal(
        requestId,
        `request-${index + 1}`,
        "the settled ledger is complete and allocation-ordered",
      );
    }

    const lastSettled = s.settledRequests.length;
    if (s.phase.kind === "pending") {
      assert.equal(
        s.phase.snapshot.requestId,
        `request-${lastSettled + 1}`,
        "the pending request is the next allocated request",
      );
      assert.equal(
        sameCounter(s.nextRequest, lastSettled + 2),
        true,
      );
    } else {
      if (["refused", "unavailable", "completed"].includes(s.phase.kind)) {
        const currentId =
          s.phase.kind === "completed"
            ? s.phase.requestId
            : s.phase.request.requestId;
        assert.equal(lastSettled > 0, true);
        assert.equal(
          currentId,
          `request-${lastSettled}`,
          "the visible settlement belongs to the latest request",
        );
      }
      assert.equal(
        sameCounter(s.nextRequest, lastSettled + 1),
        true,
        "the request allocator follows the complete settled ledger",
      );
    }

    assert.equal(
      s.navigationIntent === null ||
        (hasExactOwnKeys(s.navigationIntent, ["kind", "target"]) &&
          ["user-push", "required-replace"].includes(
            s.navigationIntent.kind,
          ) &&
          typeof s.navigationIntent.target === "string" &&
          routeFor(s.navigationIntent.target) !== null),
      true,
    );
    if (s.navigationIntent?.kind === "required-replace") {
      assert.notEqual(normalizationTarget, null);
      assert.equal(s.navigationIntent.target, normalizationTarget);
    }
    if (s.navigationIntent?.kind === "user-push") {
      assert.equal(normalizationTarget, null);
      const route = s.location?.route ?? null;
      let ownedTarget = false;
      if (route?.kind === "flow") {
        ownedTarget = [
          urls.items,
          urls.method,
          urls.review,
          urls.order,
        ].includes(s.navigationIntent.target);
      } else if (route?.kind === "receipt") {
        ownedTarget = s.navigationIntent.target === urls.order;
      } else if (
        route?.kind === "order" &&
        s.receiptAccess === "offered"
      ) {
        ownedTarget =
          s.navigationIntent.target === urls.receipt(s.completedReturn);
      }
      assert.equal(
        ownedTarget,
        true,
        "a user navigation intent has an admitted current owner",
      );
      assert.notEqual(s.navigationIntent.target, s.location.url);
    }
  }
}

export function expectStep(actual, expectedResult, expectedConsequences = []) {
  assert.equal(actual.result, expectedResult);
  assert.deepEqual(actual.consequences, expectedConsequences);
  return actual;
}

export function nav(mode, target) {
  return { kind: "navigate", mode, target };
}

export function submitCommand(snapshot) {
  return { kind: "submit-return", snapshot };
}

export function refusal(currentRevision) {
  return { kind: "refused", currentRevision };
}

export function acceptance(returnId) {
  return { kind: "accepted", returnId };
}

export const unavailable = Object.freeze({ kind: "unavailable" });

export function assertSame(actual, expected, message = undefined) {
  assert.deepEqual(actual, expected, message);
}
