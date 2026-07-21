/**
 * Browser-facing machine-kernel protocol.
 *
 * This is a transport projection, not the canonical semantic byte encoding.
 * In particular, JavaScript numbers never carry Uhura machine numeric values or
 * sequence counters: exact numerics use canonical decimal text.
 */
export const UHURA_BROWSER_PROTOCOL = "uhura-browser/3" as const;
export const UHURA_RUNTIME_SNAPSHOT_PROTOCOL =
  "uhura-runtime-snapshot/0" as const;
export const UHURA_MACHINE_PROGRAM_ID_PROTOCOL =
  "uhura-machine-program/0" as const;
export type UhuraIdentityProtocol = typeof UHURA_MACHINE_PROGRAM_ID_PROTOCOL;
export const UHURA_GENESIS_RECEIPT_PROTOCOL =
  "uhura-genesis-receipt/0" as const;
export const UHURA_REACTION_RECEIPT_PROTOCOL =
  "uhura-reaction-receipt/0" as const;
export const UHURA_INGRESS_RECORD_PROTOCOL =
  "uhura-ingress-record/0" as const;

declare const integerBrand: unique symbol;
declare const naturalBrand: unique symbol;
declare const positiveBrand: unique symbol;
declare const decimalBrand: unique symbol;
declare const ratioBrand: unique symbol;
declare const hashBrand: unique symbol;

export type IntegerText = string & {
  readonly [integerBrand]: true;
};
export type NaturalText = string & {
  readonly [naturalBrand]: true;
};
export type PositiveText = string & {
  readonly [positiveBrand]: true;
};
export type DecimalText = string & {
  readonly [decimalBrand]: true;
};
export type RatioText = string & {
  readonly [ratioBrand]: true;
};
export type Hash = string & {
  readonly [hashBrand]: true;
};

const INTEGER = /^(?:0|-[1-9]\d*|[1-9]\d*)$/u;
const NATURAL = /^(?:0|[1-9]\d*)$/u;
const POSITIVE = /^[1-9]\d*$/u;
const DECIMAL = /^(?:0|-?(?:(?:[1-9]\d*)(?:\.\d*[1-9])?|0\.\d*[1-9]))$/u;
const HASH = /^[0-9a-f]{64}$/u;

const exactText = <T extends string>(
  value: string,
  pattern: RegExp,
  name: string,
): T => {
  if (!pattern.test(value)) {
    throw new TypeError(`${name} must use canonical exact text, got \`${value}\``);
  }
  return value as T;
};

export const integer = (value: string): IntegerText =>
  exactText(value, INTEGER, "Uhura Int");

export const natural = (value: string): NaturalText =>
  exactText(value, NATURAL, "Uhura Nat");

export const positive = (value: string): PositiveText =>
  exactText(value, POSITIVE, "Uhura PositiveInt");

export const decimal = (value: string): DecimalText =>
  exactText(value, DECIMAL, "Uhura Decimal");

export const ratio = (value: string): RatioText => {
  const decimal = exactText<RatioText>(value, DECIMAL, "Uhura Ratio");
  if (
    value.startsWith("-")
    || (value !== "0" && value !== "1" && !value.startsWith("0."))
  ) {
    throw new RangeError(`Uhura Ratio must be within 0..1, got \`${value}\``);
  }
  return decimal;
};

export const hash = (value: string): Hash =>
  exactText(value, HASH, "Uhura hash");

export const decodeIdentityProtocol = (
  value: unknown,
  context: string,
): UhuraIdentityProtocol => {
  if (value !== UHURA_MACHINE_PROGRAM_ID_PROTOCOL) {
    throw new TypeError(
      `${context} must be ${JSON.stringify(UHURA_MACHINE_PROGRAM_ID_PROTOCOL)}`,
    );
  }
  return value;
};

export type BoundaryValue =
  | {
      readonly $: "BoundaryNumber";
      readonly case: "finite";
      readonly value: DecimalText;
    }
  | {
      readonly $: "BoundaryNumber";
      readonly case: "nan" | "positive_infinity" | "negative_infinity";
    };

export interface VariantField {
  readonly name: string | null;
  readonly value: Value;
}

export interface RecordField {
  readonly name: string;
  readonly value: Value;
}

/**
 * The lossless JSON projection of the Uhura value domain.
 *
 * `$` is intentionally reserved as the value tag. Declared type identity is
 * retained for nominal keys and variants. Sequence and field order are kept
 * where Uhura semantics make order observable.
 */
export type Value =
  | { readonly $: "unit" }
  | { readonly $: "bool"; readonly value: boolean }
  | { readonly $: "Int"; readonly value: IntegerText }
  | { readonly $: "Nat"; readonly value: NaturalText }
  | { readonly $: "PositiveInt"; readonly value: PositiveText }
  | { readonly $: "Decimal"; readonly value: DecimalText }
  | { readonly $: "Ratio"; readonly value: RatioText }
  | BoundaryValue
  | { readonly $: "Text"; readonly value: string }
  | {
      readonly $: "key";
      readonly type: string;
      readonly value: Value;
    }
  | { readonly $: "tuple"; readonly items: readonly Value[] }
  | {
      readonly $: "record";
      readonly fields: readonly RecordField[];
    }
  | {
      readonly $: "variant";
      readonly type: string;
      readonly case: string;
      readonly fields: readonly VariantField[];
    }
  | {
      readonly $: "seq" | "nonempty" | "set";
      readonly items: readonly Value[];
    }
  | {
      readonly $: "map";
      readonly entries: readonly (readonly [Value, Value])[];
    }
  | {
      readonly $: "table";
      readonly keyType: string;
      readonly entries: readonly (readonly [string, Value])[];
    };

export type Observation = Value;

export type ResolvedInput =
  | {
      readonly source: "local";
      readonly value: Value;
    }
  | {
      readonly source: "port";
      readonly port: string;
      readonly value: Value;
    };

export type ResolvedCommand =
  | {
      readonly target: "local";
      readonly value: Value;
    }
  | {
      readonly target: "port";
      readonly port: string;
      readonly value: Value;
    };

export interface ProgramFault {
  readonly code: string;
  readonly message: string;
  readonly source?: {
    readonly id: string;
    readonly path: string;
    readonly start: NaturalText;
    readonly end: NaturalText;
  };
}

export type ReactionResolution =
  | {
      readonly kind: "completed";
      readonly outcome: Value;
      readonly disposition: "commit" | "abort";
    }
  | {
      readonly kind: "fault";
      readonly fault: ProgramFault;
    };

interface ReceiptIdentity {
  readonly instance: string;
  readonly machineProgramHash: Hash;
  readonly configurationHash: Hash;
  readonly sequence: NaturalText;
}

export interface GenesisReceipt extends ReceiptIdentity {
  readonly protocol: typeof UHURA_GENESIS_RECEIPT_PROTOCOL;
  readonly kind: "genesis";
  readonly sequence: NaturalText;
  readonly initialObservation: Observation;
  readonly initialStateHash: Hash;
}

export interface ReactionReceipt extends ReceiptIdentity {
  readonly protocol: typeof UHURA_REACTION_RECEIPT_PROTOCOL;
  readonly kind: "reaction";
  readonly input: ResolvedInput;
  readonly resolution: ReactionResolution;
  readonly orderedCommands: readonly ResolvedCommand[];
  readonly postObservation: Observation;
  readonly preStateHash: Hash;
  readonly postStateHash: Hash;
}

export type Receipt = GenesisReceipt | ReactionReceipt;

export type InstanceLifecycle = "running" | "faulted" | "stopped";

/** Privileged headless data. Presentation code receives observation only. */
export interface Inspection {
  readonly protocol: typeof UHURA_BROWSER_PROTOCOL;
  readonly identityProtocol: UhuraIdentityProtocol;
  readonly instance: string;
  readonly machineProgramHash: Hash;
  readonly presentation: string | null;
  readonly presentationHash: Hash | null;
  readonly configurationHash: Hash;
  readonly configuration: Value;
  readonly state: Value;
  readonly observation: Observation;
  readonly inbox: readonly ResolvedInput[];
  readonly lifecycle: InstanceLifecycle;
  readonly nextSequence: NaturalText;
  readonly tracePrefixHash: Hash;
  readonly receipts: readonly Receipt[];
  readonly ingressPrefixHash: Hash;
  readonly nextIngressOrdinal: NaturalText;
  readonly ingressRecords: readonly IngressRecord[];
}

/**
 * Bounded current runtime facts published with one reaction.
 *
 * Unlike {@link Inspection}, a snapshot deliberately contains no cumulative
 * receipt or ingress log. The reaction receipt travels beside it, and full
 * inspection remains an explicit privileged operation.
 */
export interface RuntimeSnapshot {
  readonly protocol: typeof UHURA_RUNTIME_SNAPSHOT_PROTOCOL;
  readonly instance: string;
  readonly machineProgramHash: Hash;
  readonly presentation: string | null;
  readonly presentationHash: Hash | null;
  readonly configurationHash: Hash;
  readonly state: Value;
  readonly stateHash: Hash;
  readonly lifecycle: InstanceLifecycle;
  readonly nextSequence: NaturalText;
  readonly tracePrefixHash: Hash;
  readonly ingressPrefixHash: Hash;
  readonly nextIngressOrdinal: NaturalText;
}

export interface IngressRecord {
  readonly protocol: typeof UHURA_INGRESS_RECORD_PROTOCOL;
  readonly instance: string;
  readonly machineProgramHash: Hash;
  readonly ordinal: NaturalText;
  readonly machineSequence: NaturalText;
  readonly rejection:
    | "malformed-transport"
    | "invalid-value"
    | "lifecycle"
    | "missing-machine";
  readonly message: string;
  readonly attempt:
    | { readonly kind: "transport-text"; readonly text: string }
    | { readonly kind: "value"; readonly value: Value };
}

export interface ReactionStep {
  readonly protocol: typeof UHURA_BROWSER_PROTOCOL;
  readonly receipt: ReactionReceipt;
  readonly snapshot: RuntimeSnapshot;
}

const objectValue = (
  value: unknown,
  context: string,
): Readonly<Record<string, unknown>> => {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new TypeError(`${context} must be an object`);
  }
  return value as Readonly<Record<string, unknown>>;
};

const exactObjectFields = (
  object: Readonly<Record<string, unknown>>,
  fields: readonly string[],
  context: string,
): void => {
  const expected = new Set(fields);
  const actual = Object.keys(object);
  const missing = fields.filter((field) => !Object.hasOwn(object, field));
  const extra = actual.filter((field) => !expected.has(field));
  if (missing.length > 0 || extra.length > 0) {
    throw new TypeError(
      `${context} has the wrong fields; missing [${missing.join(", ")}], extra [${extra.join(", ")}]`,
    );
  }
};

const textField = (
  object: Readonly<Record<string, unknown>>,
  field: string,
  context: string,
): string => {
  const value = object[field];
  if (typeof value !== "string") {
    throw new TypeError(`${context}.${field} must be text`);
  }
  return value;
};

const arrayField = (
  object: Readonly<Record<string, unknown>>,
  field: string,
  context: string,
): readonly unknown[] => {
  const value = object[field];
  if (!Array.isArray(value)) {
    throw new TypeError(`${context}.${field} must be a list`);
  }
  return value;
};

const decodeItems = (
  object: Readonly<Record<string, unknown>>,
  field: string,
  context: string,
): Value[] =>
  arrayField(object, field, context).map((value, index) =>
    decodeValue(value, `${context}.${field}[${index}]`)
  );

/**
 * Validates unknown host JSON before it enters browser code. The decoder
 * creates a fresh typed value so no unchecked foreign object survives.
 */
export function decodeValue(
  input: unknown,
  context = "Uhura value",
): Value {
  const object = objectValue(input, context);
  const tag = textField(object, "$", context);
  switch (tag) {
    case "unit":
      exactObjectFields(object, ["$"], context);
      return { $: "unit" };
    case "bool": {
      exactObjectFields(object, ["$", "value"], context);
      const value = object["value"];
      if (typeof value !== "boolean") {
        throw new TypeError(`${context}.value must be boolean`);
      }
      return { $: "bool", value };
    }
    case "Int":
      exactObjectFields(object, ["$", "value"], context);
      return { $: "Int", value: integer(textField(object, "value", context)) };
    case "Nat":
      exactObjectFields(object, ["$", "value"], context);
      return { $: "Nat", value: natural(textField(object, "value", context)) };
    case "PositiveInt":
      exactObjectFields(object, ["$", "value"], context);
      return {
        $: "PositiveInt",
        value: positive(textField(object, "value", context)),
      };
    case "Decimal":
      exactObjectFields(object, ["$", "value"], context);
      return {
        $: "Decimal",
        value: decimal(textField(object, "value", context)),
      };
    case "Ratio":
      exactObjectFields(object, ["$", "value"], context);
      return {
        $: "Ratio",
        value: ratio(textField(object, "value", context)),
      };
    case "BoundaryNumber": {
      const boundaryCase = textField(object, "case", context);
      if (boundaryCase === "finite") {
        exactObjectFields(object, ["$", "case", "value"], context);
        return {
          $: "BoundaryNumber",
          case: boundaryCase,
          value: decimal(textField(object, "value", context)),
        };
      }
      if (
        boundaryCase === "nan"
        || boundaryCase === "positive_infinity"
        || boundaryCase === "negative_infinity"
      ) {
        exactObjectFields(object, ["$", "case"], context);
        return { $: "BoundaryNumber", case: boundaryCase };
      }
      throw new TypeError(`${context}.case is not a BoundaryNumber case`);
    }
    case "Text":
      exactObjectFields(object, ["$", "value"], context);
      return { $: "Text", value: textField(object, "value", context) };
    case "key":
      exactObjectFields(object, ["$", "type", "value"], context);
      return {
        $: "key",
        type: textField(object, "type", context),
        value: decodeValue(object["value"], `${context}.value`),
      };
    case "tuple":
      exactObjectFields(object, ["$", "items"], context);
      return { $: "tuple", items: decodeItems(object, "items", context) };
    case "record": {
      exactObjectFields(object, ["$", "fields"], context);
      const names = new Set<string>();
      const decoded = arrayField(object, "fields", context).map(
        (field, index): RecordField => {
          const fieldContext = `${context}.fields[${index}]`;
          const fieldObject = objectValue(field, fieldContext);
          exactObjectFields(fieldObject, ["name", "value"], fieldContext);
          const name = textField(fieldObject, "name", fieldContext);
          if (names.has(name)) {
            throw new TypeError(`${context} contains duplicate field \`${name}\``);
          }
          names.add(name);
          return {
            name,
            value: decodeValue(
              fieldObject["value"],
              `${fieldContext}.value`,
            ),
          };
        },
      );
      return { $: "record", fields: decoded };
    }
    case "variant":
      exactObjectFields(object, ["$", "type", "case", "fields"], context);
      return {
        $: "variant",
        type: textField(object, "type", context),
        case: textField(object, "case", context),
        fields: arrayField(object, "fields", context).map((field, index) => {
          const fieldObject = objectValue(
            field,
            `${context}.fields[${index}]`,
          );
          exactObjectFields(
            fieldObject,
            ["name", "value"],
            `${context}.fields[${index}]`,
          );
          const name = fieldObject["name"];
          if (name !== null && typeof name !== "string") {
            throw new TypeError(
              `${context}.fields[${index}].name must be text or null`,
            );
          }
          return {
            name: typeof name === "string" ? name : null,
            value: decodeValue(
              fieldObject["value"],
              `${context}.fields[${index}].value`,
            ),
          };
        }),
      };
    case "seq":
    case "nonempty":
    case "set": {
      exactObjectFields(object, ["$", "items"], context);
      const items = decodeItems(object, "items", context);
      if (tag === "nonempty" && items.length === 0) {
        throw new TypeError(`${context} must contain at least one item`);
      }
      return { $: tag, items };
    }
    case "map":
      exactObjectFields(object, ["$", "entries"], context);
      return {
        $: "map",
        entries: arrayField(object, "entries", context).map((entry, index) => {
          if (!Array.isArray(entry) || entry.length !== 2) {
            throw new TypeError(`${context}.entries[${index}] must be a pair`);
          }
          return [
            decodeValue(entry[0], `${context}.entries[${index}][0]`),
            decodeValue(entry[1], `${context}.entries[${index}][1]`),
          ];
        }),
      };
    case "table":
      exactObjectFields(object, ["$", "keyType", "entries"], context);
      return {
        $: "table",
        keyType: textField(object, "keyType", context),
        entries: arrayField(object, "entries", context).map((entry, index) => {
          if (
            !Array.isArray(entry)
            || entry.length !== 2
            || typeof entry[0] !== "string"
          ) {
            throw new TypeError(
              `${context}.entries[${index}] must be a text/value pair`,
            );
          }
          return [
            entry[0],
            decodeValue(entry[1], `${context}.entries[${index}][1]`),
          ];
        }),
      };
    default:
      throw new TypeError(`${context} has unknown tag \`${tag}\``);
  }
}

const nonemptyTextField = (
  object: Readonly<Record<string, unknown>>,
  field: string,
  context: string,
): string => {
  const value = textField(object, field, context);
  if (value.length === 0) {
    throw new TypeError(`${context}.${field} must be nonempty text`);
  }
  return value;
};

export const decodeResolvedInput = (
  input: unknown,
  context: string,
): ResolvedInput => {
  const object = objectValue(input, context);
  const source = textField(object, "source", context);
  if (source === "local") {
    exactObjectFields(object, ["source", "value"], context);
    return {
      source,
      value: decodeValue(object["value"], `${context}.value`),
    };
  }
  if (source === "port") {
    exactObjectFields(object, ["source", "port", "value"], context);
    return {
      source,
      port: nonemptyTextField(object, "port", context),
      value: decodeValue(object["value"], `${context}.value`),
    };
  }
  throw new TypeError(`${context}.source must be \`local\` or \`port\``);
};

export const decodeResolvedCommand = (
  input: unknown,
  context: string,
): ResolvedCommand => {
  const object = objectValue(input, context);
  const target = textField(object, "target", context);
  if (target === "local") {
    exactObjectFields(object, ["target", "value"], context);
    return {
      target,
      value: decodeValue(object["value"], `${context}.value`),
    };
  }
  if (target === "port") {
    exactObjectFields(object, ["target", "port", "value"], context);
    return {
      target,
      port: nonemptyTextField(object, "port", context),
      value: decodeValue(object["value"], `${context}.value`),
    };
  }
  throw new TypeError(`${context}.target must be \`local\` or \`port\``);
};

const decodeProgramFault = (
  input: unknown,
  context: string,
): ProgramFault => {
  const object = objectValue(input, context);
  const sourceValue = object["source"];
  exactObjectFields(
    object,
    sourceValue === undefined
      ? ["code", "message"]
      : ["code", "message", "source"],
    context,
  );
  let source: ProgramFault["source"];
  if (sourceValue !== undefined) {
    const sourceObject = objectValue(sourceValue, `${context}.source`);
    exactObjectFields(
      sourceObject,
      ["id", "path", "start", "end"],
      `${context}.source`,
    );
    const start = natural(
      textField(sourceObject, "start", `${context}.source`),
    );
    const end = natural(
      textField(sourceObject, "end", `${context}.source`),
    );
    if (BigInt(end) < BigInt(start)) {
      throw new TypeError(`${context}.source.end must not precede start`);
    }
    source = {
      id: nonemptyTextField(sourceObject, "id", `${context}.source`),
      path: nonemptyTextField(sourceObject, "path", `${context}.source`),
      start,
      end,
    };
  }
  return {
    code: nonemptyTextField(object, "code", context),
    message: textField(object, "message", context),
    ...(source === undefined ? {} : { source }),
  };
};

const decodeReactionResolution = (
  input: unknown,
  context: string,
): ReactionResolution => {
  const object = objectValue(input, context);
  const kind = textField(object, "kind", context);
  if (kind === "completed") {
    exactObjectFields(
      object,
      ["kind", "outcome", "disposition"],
      context,
    );
    const disposition = textField(object, "disposition", context);
    if (disposition !== "commit" && disposition !== "abort") {
      throw new TypeError(
        `${context}.disposition must be \`commit\` or \`abort\``,
      );
    }
    return {
      kind,
      outcome: decodeValue(object["outcome"], `${context}.outcome`),
      disposition,
    };
  }
  if (kind === "fault") {
    exactObjectFields(object, ["kind", "fault"], context);
    return {
      kind,
      fault: decodeProgramFault(object["fault"], `${context}.fault`),
    };
  }
  throw new TypeError(`${context}.kind is not a Uhura reaction resolution`);
};

const decodeReceiptIdentity = (
  object: Readonly<Record<string, unknown>>,
  context: string,
): ReceiptIdentity => {
  return {
    instance: nonemptyTextField(object, "instance", context),
    machineProgramHash: hash(
      textField(object, "machineProgramHash", context),
    ),
    configurationHash: hash(
      textField(object, "configurationHash", context),
    ),
    sequence: natural(textField(object, "sequence", context)),
  };
};

export const decodeGenesisReceipt = (
  input: unknown,
  context = "Uhura genesis receipt",
): GenesisReceipt => {
  const object = objectValue(input, context);
  exactObjectFields(
    object,
    [
      "protocol",
      "kind",
      "instance",
      "machineProgramHash",
      "configurationHash",
      "sequence",
      "initialObservation",
      "initialStateHash",
    ],
    context,
  );
  if (object["kind"] !== "genesis") {
    throw new TypeError(`${context}.kind must be \`genesis\``);
  }
  if (object["protocol"] !== UHURA_GENESIS_RECEIPT_PROTOCOL) {
    throw new TypeError(
      `${context}.protocol must be ${JSON.stringify(UHURA_GENESIS_RECEIPT_PROTOCOL)}`,
    );
  }
  const identity = decodeReceiptIdentity(object, context);
  if (identity.sequence !== "0") {
    throw new TypeError(`${context}.sequence must be canonical genesis zero`);
  }
  return {
    protocol: UHURA_GENESIS_RECEIPT_PROTOCOL,
    ...identity,
    kind: "genesis",
    initialObservation: decodeValue(
      object["initialObservation"],
      `${context}.initialObservation`,
    ),
    initialStateHash: hash(
      textField(object, "initialStateHash", context),
    ),
  };
};

export const decodeReactionReceipt = (
  input: unknown,
  context = "Uhura reaction receipt",
): ReactionReceipt => {
  const object = objectValue(input, context);
  exactObjectFields(
    object,
    [
      "protocol",
      "kind",
      "instance",
      "machineProgramHash",
      "configurationHash",
      "sequence",
      "input",
      "resolution",
      "orderedCommands",
      "postObservation",
      "preStateHash",
      "postStateHash",
    ],
    context,
  );
  if (object["kind"] !== "reaction") {
    throw new TypeError(`${context}.kind must be \`reaction\``);
  }
  if (object["protocol"] !== UHURA_REACTION_RECEIPT_PROTOCOL) {
    throw new TypeError(
      `${context}.protocol must be ${JSON.stringify(UHURA_REACTION_RECEIPT_PROTOCOL)}`,
    );
  }
  const identity = decodeReceiptIdentity(object, context);
  if (identity.sequence === "0") {
    throw new TypeError(`${context}.sequence must be positive`);
  }
  return {
    protocol: UHURA_REACTION_RECEIPT_PROTOCOL,
    ...identity,
    kind: "reaction",
    input: decodeResolvedInput(object["input"], `${context}.input`),
    resolution: decodeReactionResolution(
      object["resolution"],
      `${context}.resolution`,
    ),
    orderedCommands: arrayField(object, "orderedCommands", context).map(
      (command, index) =>
        decodeResolvedCommand(
          command,
          `${context}.orderedCommands[${index}]`,
        ),
    ),
    postObservation: decodeValue(
      object["postObservation"],
      `${context}.postObservation`,
    ),
    preStateHash: hash(textField(object, "preStateHash", context)),
    postStateHash: hash(textField(object, "postStateHash", context)),
  };
};

export const decodeReceipt = (
  input: unknown,
  context = "Uhura receipt",
): Receipt => {
  const object = objectValue(input, context);
  if (object["kind"] === "genesis") {
    return decodeGenesisReceipt(object, context);
  }
  if (object["kind"] === "reaction") {
    return decodeReactionReceipt(object, context);
  }
  throw new TypeError(`${context}.kind is not a Uhura receipt kind`);
};

const decodeIngressRecord = (
  input: unknown,
  context: string,
): IngressRecord => {
  const object = objectValue(input, context);
  exactObjectFields(
    object,
    [
      "protocol",
      "instance",
      "machineProgramHash",
      "ordinal",
      "machineSequence",
      "rejection",
      "message",
      "attempt",
    ],
    context,
  );
  if (object["protocol"] !== UHURA_INGRESS_RECORD_PROTOCOL) {
    throw new TypeError(
      `${context}.protocol must be ${JSON.stringify(UHURA_INGRESS_RECORD_PROTOCOL)}`,
    );
  }
  const rejection = textField(object, "rejection", context);
  if (
    rejection !== "malformed-transport"
    && rejection !== "invalid-value"
    && rejection !== "lifecycle"
    && rejection !== "missing-machine"
  ) {
    throw new TypeError(`${context}.rejection is not supported`);
  }
  const attemptObject = objectValue(object["attempt"], `${context}.attempt`);
  const attemptKind = textField(attemptObject, "kind", `${context}.attempt`);
  let attempt: IngressRecord["attempt"];
  if (attemptKind === "transport-text") {
    exactObjectFields(
      attemptObject,
      ["kind", "text"],
      `${context}.attempt`,
    );
    attempt = {
      kind: attemptKind,
      text: textField(attemptObject, "text", `${context}.attempt`),
    };
  } else if (attemptKind === "value") {
    exactObjectFields(
      attemptObject,
      ["kind", "value"],
      `${context}.attempt`,
    );
    attempt = {
      kind: attemptKind,
      value: decodeValue(
        attemptObject["value"],
        `${context}.attempt.value`,
      ),
    };
  } else {
    throw new TypeError(`${context}.attempt.kind is not supported`);
  }
  return {
    protocol: UHURA_INGRESS_RECORD_PROTOCOL,
    instance: nonemptyTextField(object, "instance", context),
    machineProgramHash: hash(
      textField(object, "machineProgramHash", context),
    ),
    ordinal: natural(textField(object, "ordinal", context)),
    machineSequence: natural(
      textField(object, "machineSequence", context),
    ),
    rejection,
    message: textField(object, "message", context),
    attempt,
  };
};

const sameWireValue = (left: unknown, right: unknown): boolean =>
  JSON.stringify(left) === JSON.stringify(right);

export const decodeInspection = (
  input: unknown,
  context = "Uhura inspection",
): Inspection => {
  const object = objectValue(input, context);
  exactObjectFields(
    object,
    [
      "protocol",
      "identityProtocol",
      "instance",
      "machineProgramHash",
      "presentation",
      "presentationHash",
      "configurationHash",
      "configuration",
      "state",
      "observation",
      "inbox",
      "lifecycle",
      "nextSequence",
      "tracePrefixHash",
      "receipts",
      "ingressPrefixHash",
      "nextIngressOrdinal",
      "ingressRecords",
    ],
    context,
  );
  if (object["protocol"] !== UHURA_BROWSER_PROTOCOL) {
    throw new TypeError(
      `${context}.protocol must be ${JSON.stringify(UHURA_BROWSER_PROTOCOL)}`,
    );
  }
  const identityProtocol = decodeIdentityProtocol(
    object["identityProtocol"],
    `${context}.identityProtocol`,
  );
  const presentation = object["presentation"];
  if (presentation !== null && typeof presentation !== "string") {
    throw new TypeError(`${context}.presentation must be text or null`);
  }
  if (presentation === "") {
    throw new TypeError(`${context}.presentation must be nonempty when present`);
  }
  const presentationHash = object["presentationHash"] === null
    ? null
    : hash(textField(object, "presentationHash", context));
  if ((presentation === null) !== (presentationHash === null)) {
    throw new TypeError(
      `${context}.presentation and presentationHash must either both be null or both be present`,
    );
  }
  const lifecycle = textField(object, "lifecycle", context);
  if (
    lifecycle !== "running"
    && lifecycle !== "faulted"
    && lifecycle !== "stopped"
  ) {
    throw new TypeError(`${context}.lifecycle is not supported`);
  }
  const receipts = arrayField(object, "receipts", context).map(
    (receipt, index) =>
      decodeReceipt(receipt, `${context}.receipts[${index}]`),
  );
  if (receipts.length === 0 || receipts[0]?.kind !== "genesis") {
    throw new TypeError(`${context}.receipts must begin with genesis`);
  }
  const instance = nonemptyTextField(object, "instance", context);
  const machineProgramHash = hash(
    textField(object, "machineProgramHash", context),
  );
  const configurationHash = hash(
    textField(object, "configurationHash", context),
  );
  let expectedSequence = 0n;
  for (const [index, receipt] of receipts.entries()) {
    if (
      receipt.instance !== instance
      || receipt.machineProgramHash !== machineProgramHash
      || receipt.configurationHash !== configurationHash
    ) {
      throw new TypeError(
        `${context}.receipts[${index}] has a different admitted identity`,
      );
    }
    if (BigInt(receipt.sequence) !== expectedSequence) {
      throw new TypeError(
        `${context}.receipts must have contiguous canonical sequences`,
      );
    }
    expectedSequence += 1n;
  }
  const nextSequence = natural(
    textField(object, "nextSequence", context),
  );
  if (BigInt(nextSequence) !== expectedSequence) {
    throw new TypeError(
      `${context}.nextSequence does not follow the retained receipt log`,
    );
  }
  const observation = decodeValue(
    object["observation"],
    `${context}.observation`,
  );
  const latest = receipts.at(-1);
  const latestObservation = latest?.kind === "reaction"
    ? latest.postObservation
    : latest?.initialObservation;
  if (
    latestObservation === undefined
    || !sameWireValue(observation, latestObservation)
  ) {
    throw new TypeError(
      `${context}.observation does not match the latest receipt`,
    );
  }
  const ingressRecords = arrayField(object, "ingressRecords", context).map(
    (record, index) =>
      decodeIngressRecord(record, `${context}.ingressRecords[${index}]`),
  );
  for (const [index, record] of ingressRecords.entries()) {
    if (
      record.instance !== instance
      || record.machineProgramHash !== machineProgramHash
      || BigInt(record.ordinal) !== BigInt(index + 1)
    ) {
      throw new TypeError(
        `${context}.ingressRecords[${index}] has incoherent identity or ordinal`,
      );
    }
  }
  const nextIngressOrdinal = natural(
    textField(object, "nextIngressOrdinal", context),
  );
  if (BigInt(nextIngressOrdinal) !== BigInt(ingressRecords.length + 1)) {
    throw new TypeError(
      `${context}.nextIngressOrdinal does not follow the retained ingress log`,
    );
  }
  return {
    protocol: UHURA_BROWSER_PROTOCOL,
    identityProtocol,
    instance,
    machineProgramHash,
    presentation:
      typeof presentation === "string"
        ? presentation
        : null,
    presentationHash,
    configurationHash,
    configuration: decodeValue(
      object["configuration"],
      `${context}.configuration`,
    ),
    state: decodeValue(object["state"], `${context}.state`),
    observation,
    inbox: arrayField(object, "inbox", context).map((value, index) =>
      decodeResolvedInput(value, `${context}.inbox[${index}]`)
    ),
    lifecycle,
    nextSequence,
    tracePrefixHash: hash(
      textField(object, "tracePrefixHash", context),
    ),
    receipts,
    ingressPrefixHash: hash(
      textField(object, "ingressPrefixHash", context),
    ),
    nextIngressOrdinal,
    ingressRecords,
  };
};

export const decodeRuntimeSnapshot = (
  input: unknown,
  context = "Uhura runtime snapshot",
): RuntimeSnapshot => {
  const object = objectValue(input, context);
  exactObjectFields(
    object,
    [
      "protocol",
      "instance",
      "machineProgramHash",
      "presentation",
      "presentationHash",
      "configurationHash",
      "state",
      "stateHash",
      "lifecycle",
      "nextSequence",
      "tracePrefixHash",
      "ingressPrefixHash",
      "nextIngressOrdinal",
    ],
    context,
  );
  if (object["protocol"] !== UHURA_RUNTIME_SNAPSHOT_PROTOCOL) {
    throw new TypeError(
      `${context}.protocol must be ${JSON.stringify(UHURA_RUNTIME_SNAPSHOT_PROTOCOL)}`,
    );
  }
  const presentation = object["presentation"];
  if (presentation !== null && typeof presentation !== "string") {
    throw new TypeError(`${context}.presentation must be text or null`);
  }
  if (presentation === "") {
    throw new TypeError(`${context}.presentation must be nonempty when present`);
  }
  const presentationHash = object["presentationHash"] === null
    ? null
    : hash(textField(object, "presentationHash", context));
  if ((presentation === null) !== (presentationHash === null)) {
    throw new TypeError(
      `${context}.presentation and presentationHash must either both be null or both be present`,
    );
  }
  const lifecycle = textField(object, "lifecycle", context);
  if (
    lifecycle !== "running"
    && lifecycle !== "faulted"
    && lifecycle !== "stopped"
  ) {
    throw new TypeError(`${context}.lifecycle is not supported`);
  }
  return {
    protocol: UHURA_RUNTIME_SNAPSHOT_PROTOCOL,
    instance: nonemptyTextField(object, "instance", context),
    machineProgramHash: hash(
      textField(object, "machineProgramHash", context),
    ),
    presentation:
      typeof presentation === "string"
        ? presentation
        : null,
    presentationHash,
    configurationHash: hash(
      textField(object, "configurationHash", context),
    ),
    state: decodeValue(object["state"], `${context}.state`),
    stateHash: hash(
      textField(object, "stateHash", context),
    ),
    lifecycle,
    nextSequence: natural(
      textField(object, "nextSequence", context),
    ),
    tracePrefixHash: hash(
      textField(object, "tracePrefixHash", context),
    ),
    ingressPrefixHash: hash(
      textField(object, "ingressPrefixHash", context),
    ),
    nextIngressOrdinal: natural(
      textField(object, "nextIngressOrdinal", context),
    ),
  };
};
