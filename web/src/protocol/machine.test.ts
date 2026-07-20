import assert from "node:assert/strict";
import { test } from "vitest";

import {
  UHURA_MACHINE_PROGRAM_ID_PROTOCOL,
  UHURA_SEMANTIC_IR_HASH_PROTOCOL,
  decodeInspection,
  decodeReactionReceipt,
  decodeValue,
  decimal,
  integer,
  natural,
  positive,
  ratio,
} from "./machine.js";

test("exact numeric text rejects lossy or non-canonical forms", () => {
  assert.equal(integer("9007199254740993"), "9007199254740993");
  assert.equal(natural("0"), "0");
  assert.equal(positive("1"), "1");
  assert.equal(decimal("-0.001"), "-0.001");
  assert.equal(ratio("0.125"), "0.125");

  assert.throws(() => integer("01"), /canonical exact text/u);
  assert.throws(() => natural("-1"), /canonical exact text/u);
  assert.throws(() => positive("0"), /canonical exact text/u);
  assert.throws(() => decimal("1.0"), /canonical exact text/u);
  assert.throws(() => decimal("-0"), /canonical exact text/u);
  assert.throws(() => ratio("1.01"), /within 0\.\.1/u);
});

test("the value decoder preserves tags, nominal identity, and large integers", () => {
  const value = decodeValue({
    $: "variant",
    type: "example.counter@1::Message",
    case: "changed",
    fields: [
      {
        name: "count",
        value: { $: "Int", value: "9007199254740993" },
      },
    ],
  });

  assert.deepEqual(value, {
    $: "variant",
    type: "example.counter@1::Message",
    case: "changed",
    fields: [
      {
        name: "count",
        value: { $: "Int", value: "9007199254740993" },
      },
    ],
  });
});

test("record fields retain declared order and reject duplicate names", () => {
  const value = decodeValue({
    $: "record",
    fields: [
      { name: "zebra", value: { $: "Text", value: "first" } },
      { name: "alpha", value: { $: "Text", value: "second" } },
    ],
  });

  assert.deepEqual(value, {
    $: "record",
    fields: [
      { name: "zebra", value: { $: "Text", value: "first" } },
      { name: "alpha", value: { $: "Text", value: "second" } },
    ],
  });
  assert.throws(
    () => decodeValue({
      $: "record",
      fields: [
        { name: "same", value: { $: "unit" } },
        { name: "same", value: { $: "unit" } },
      ],
    }),
    /duplicate field/u,
  );
});

test("the value decoder rejects unknown fields on every value form", () => {
  const forms: readonly (readonly [string, unknown])[] = [
    ["unit", { $: "unit", ambient: true }],
    ["bool", { $: "bool", value: true, ambient: true }],
    ["Int", { $: "Int", value: "1", ambient: true }],
    ["Nat", { $: "Nat", value: "1", ambient: true }],
    ["PositiveInt", { $: "PositiveInt", value: "1", ambient: true }],
    ["Decimal", { $: "Decimal", value: "1", ambient: true }],
    ["Ratio", { $: "Ratio", value: "1", ambient: true }],
    [
      "finite BoundaryNumber",
      { $: "BoundaryNumber", case: "finite", value: "1", ambient: true },
    ],
    [
      "non-finite BoundaryNumber",
      { $: "BoundaryNumber", case: "nan", value: "0" },
    ],
    ["Text", { $: "Text", value: "x", ambient: true }],
    [
      "key",
      {
        $: "key",
        type: "example@1::Id",
        value: { $: "Text", value: "x" },
        ambient: true,
      },
    ],
    ["tuple", { $: "tuple", items: [], ambient: true }],
    ["record", { $: "record", fields: [], ambient: true }],
    [
      "variant",
      {
        $: "variant",
        type: "example@1::Choice",
        case: "ready",
        fields: [],
        ambient: true,
      },
    ],
    ["seq", { $: "seq", items: [], ambient: true }],
    ["nonempty", { $: "nonempty", items: [{ $: "unit" }], ambient: true }],
    ["set", { $: "set", items: [], ambient: true }],
    ["map", { $: "map", entries: [], ambient: true }],
    [
      "table",
      {
        $: "table",
        keyType: "example@1::Id",
        entries: [],
        ambient: true,
      },
    ],
  ];

  for (const [name, value] of forms) {
    assert.throws(
      () => decodeValue(value, name),
      /wrong fields/u,
      `${name} must be an exact object`,
    );
  }
});

test("the value decoder rejects nested field and collection drift", () => {
  assert.throws(
    () => decodeValue({
      $: "record",
      fields: [{
        name: "count",
        value: { $: "Int", value: "1" },
        ambient: true,
      }],
    }),
    /fields\[0\].*wrong fields/u,
  );
  assert.throws(
    () => decodeValue({
      $: "record",
      fields: [{
        name: "count",
        value: { $: "Int", value: "1", ambient: true },
      }],
    }),
    /fields\[0\]\.value.*wrong fields/u,
  );
  assert.throws(
    () => decodeValue({
      $: "variant",
      type: "example@1::Choice",
      case: "ready",
      fields: [{ value: { $: "unit" } }],
    }),
    /fields\[0\].*wrong fields/u,
  );
  assert.throws(
    () => decodeValue({
      $: "map",
      entries: [[
        { $: "Text", value: "key" },
        { $: "unit", ambient: true },
      ]],
    }),
    /entries\[0\]\[1\].*wrong fields/u,
  );
  assert.throws(
    () => decodeValue({
      $: "table",
      keyType: "example@1::Id",
      entries: [["key", { $: "unit" }, { $: "unit" }]],
    }),
    /text\/value pair/u,
  );
});

const hash = (digit: string): string => digit.repeat(64);
const observation = (count: string) => ({
  $: "record",
  fields: [{ name: "count", value: { $: "Int", value: count } }],
});
const genesis = {
  protocol: "uhura-genesis-receipt/0",
  kind: "genesis",
  instance: "entry/counter",
  machineProgramHash: hash("a"),
  configurationHash: hash("b"),
  sequence: "0",
  initialObservation: observation("0"),
  initialStateHash: hash("c"),
};
const reaction = {
  protocol: "uhura-reaction-receipt/0",
  kind: "reaction",
  instance: "entry/counter",
  machineProgramHash: hash("a"),
  configurationHash: hash("b"),
  sequence: "1",
  input: {
    source: "local",
    value: {
      $: "variant",
      type: "example.counter@1::Counter::Input",
      case: "increment",
      fields: [],
    },
  },
  resolution: {
    kind: "completed",
    outcome: {
      $: "variant",
      type: "example.counter@1::Counter::Outcome",
      case: "accepted",
      fields: [],
    },
    disposition: "commit",
  },
  orderedCommands: [],
  postObservation: observation("1"),
  preStateHash: hash("c"),
  postStateHash: hash("d"),
};
const inspection = {
  protocol: "uhura-browser/2",
  identityProtocol: UHURA_SEMANTIC_IR_HASH_PROTOCOL,
  instance: "entry/counter",
  machineProgramHash: hash("a"),
  presentation: "example.counter_web@1::CounterWeb",
  presentationHash: hash("e"),
  configurationHash: hash("b"),
  configuration: { $: "unit" },
  state: observation("1"),
  observation: observation("1"),
  inbox: [],
  lifecycle: "running",
  nextSequence: "2",
  tracePrefixHash: hash("f"),
  receipts: [genesis, reaction],
  ingressPrefixHash: hash("0"),
  nextIngressOrdinal: "1",
  ingressRecords: [],
};

test("receipt and inspection decoders validate the complete exact protocol", () => {
  assert.deepEqual(decodeReactionReceipt(reaction), reaction);
  assert.deepEqual(decodeInspection(inspection), inspection);
  assert.equal(
    decodeInspection({
      ...inspection,
      identityProtocol: UHURA_MACHINE_PROGRAM_ID_PROTOCOL,
    }).identityProtocol,
    UHURA_MACHINE_PROGRAM_ID_PROTOCOL,
  );
  assert.throws(
    () =>
      decodeInspection({
        ...inspection,
        identityProtocol: "uhura-unrecognized-identity/9",
      }),
    /identityProtocol must be/u,
  );
});

test("receipt and inspection decoders reject drift and incoherent retained state", () => {
  assert.throws(
    () => decodeReactionReceipt({
      ...reaction,
      machineProgramHash: "A".repeat(64),
    }),
    /canonical exact text/u,
  );
  assert.throws(
    () => decodeReactionReceipt({
      ...reaction,
      sequence: 9_007_199_254_740_992,
    }),
    /sequence must be text/u,
  );
  assert.throws(
    () => decodeReactionReceipt({
      ...reaction,
      unexpected: true,
    }),
    /wrong fields/u,
  );
  assert.throws(
    () => decodeInspection({
      ...inspection,
      observation: observation("2"),
    }),
    /latest receipt/u,
  );
  assert.throws(
    () => decodeInspection({
      ...inspection,
      nextSequence: "9007199254740993",
    }),
    /does not follow/u,
  );
});

test("the value decoder rejects JavaScript numeric shortcuts", () => {
  assert.throws(
    () => decodeValue({ $: "Int", value: 9_007_199_254_740_992 }),
    /value must be text/u,
  );
  assert.throws(
    () => decodeValue({ $: "nonempty", items: [] }),
    /at least one/u,
  );
  assert.throws(
    () => decodeValue({ $: "Ratio", value: "2" }),
    /within 0\.\.1/u,
  );
  assert.throws(
    () => decodeValue({ $: "mystery" }),
    /unknown tag/u,
  );
});
