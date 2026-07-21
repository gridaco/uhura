export const UHURA_EVIDENCE_SUMMARY_PROTOCOL =
  "uhura-evidence-summary/0" as const;

export interface EvidenceSummary {
  readonly protocol: typeof UHURA_EVIDENCE_SUMMARY_PROTOCOL;
  readonly passed: boolean;
  readonly scenarios: {
    readonly total: number;
    readonly passed: number;
    readonly failed: number;
  };
  readonly artifacts: {
    readonly pins: number;
    readonly examples: number;
    readonly checkpoints: number;
  };
  readonly failureCount: number;
}

const object = (
  value: unknown,
  context: string,
): Readonly<Record<string, unknown>> => {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new TypeError(`${context} must be an object`);
  }
  return value as Readonly<Record<string, unknown>>;
};

const exactKeys = (
  value: Readonly<Record<string, unknown>>,
  keys: readonly string[],
  context: string,
): void => {
  const expected = new Set(keys);
  const actual = Object.keys(value);
  const missing = keys.filter((key) => !Object.hasOwn(value, key));
  const extra = actual.filter((key) => !expected.has(key));
  if (missing.length > 0 || extra.length > 0) {
    throw new TypeError(
      `${context} has the wrong fields; missing [${missing.join(", ")}], extra [${extra.join(", ")}]`,
    );
  }
};

const integer = (value: unknown, context: string): number => {
  if (!Number.isSafeInteger(value) || (value as number) < 0) {
    throw new TypeError(`${context} must be a non-negative safe integer`);
  }
  return value as number;
};

export const decodeEvidenceSummary = (
  value: unknown,
  context: string,
): EvidenceSummary => {
  const source = object(value, context);
  exactKeys(
    source,
    ["protocol", "passed", "scenarios", "artifacts", "failureCount"],
    context,
  );
  if (source["protocol"] !== UHURA_EVIDENCE_SUMMARY_PROTOCOL) {
    throw new TypeError(
      `${context}.protocol must be ${JSON.stringify(UHURA_EVIDENCE_SUMMARY_PROTOCOL)}`,
    );
  }
  if (typeof source["passed"] !== "boolean") {
    throw new TypeError(`${context}.passed must be a boolean`);
  }

  const scenarios = object(source["scenarios"], `${context}.scenarios`);
  exactKeys(scenarios, ["total", "passed", "failed"], `${context}.scenarios`);
  const total = integer(scenarios["total"], `${context}.scenarios.total`);
  const passed = integer(scenarios["passed"], `${context}.scenarios.passed`);
  const failed = integer(scenarios["failed"], `${context}.scenarios.failed`);
  if (passed + failed !== total) {
    throw new TypeError(
      `${context}.scenarios passed and failed counts must add up to total`,
    );
  }

  const artifacts = object(source["artifacts"], `${context}.artifacts`);
  exactKeys(
    artifacts,
    ["pins", "examples", "checkpoints"],
    `${context}.artifacts`,
  );
  const failureCount = integer(source["failureCount"], `${context}.failureCount`);
  if (source["passed"] !== (failureCount === 0)) {
    throw new TypeError(`${context}.passed must agree with failureCount`);
  }

  return {
    protocol: UHURA_EVIDENCE_SUMMARY_PROTOCOL,
    passed: source["passed"],
    scenarios: { total, passed, failed },
    artifacts: {
      pins: integer(artifacts["pins"], `${context}.artifacts.pins`),
      examples: integer(artifacts["examples"], `${context}.artifacts.examples`),
      checkpoints: integer(
        artifacts["checkpoints"],
        `${context}.artifacts.checkpoints`,
      ),
    },
    failureCount,
  };
};
