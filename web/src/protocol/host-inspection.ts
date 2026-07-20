import {
  decodeInteractionGraphArtifacts,
  type InteractionGraph,
  type InteractionGraphSources,
} from "./interaction-graph.js";
import {
  decodeIdentityProtocol,
  hash,
  type Hash,
  type UhuraIdentityProtocol,
} from "./machine.js";
import {
  decodeSemanticProvenance,
  type SemanticProvenance,
} from "./provenance.js";

export const UHURA_HOST_INSPECTION_PROTOCOL = "uhura-inspection/0" as const;

export interface InspectionSource {
  readonly file: number;
  readonly path: string;
  readonly sha256: Hash;
  readonly bytes: number;
}

export interface HostInspection {
  readonly protocol: typeof UHURA_HOST_INSPECTION_PROTOCOL;
  readonly identityProtocol: UhuraIdentityProtocol;
  readonly entry: string;
  readonly machine: string;
  readonly presentation: string | null;
  readonly machineProgramHash: Hash;
  readonly presentationHash: Hash | null;
  readonly evidenceHash: Hash | null;
  readonly deploymentHash: Hash;
  readonly sources: readonly InspectionSource[];
  readonly provenance: SemanticProvenance;
  readonly interactionGraph: InteractionGraph;
  readonly graphSources: InteractionGraphSources;
  readonly evidence: unknown;
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

const text = (value: unknown, context: string): string => {
  if (typeof value !== "string" || value.length === 0) {
    throw new TypeError(`${context} must be nonempty text`);
  }
  return value;
};

const integer = (value: unknown, context: string): number => {
  if (!Number.isSafeInteger(value) || (value as number) < 0) {
    throw new TypeError(`${context} must be a non-negative safe integer`);
  }
  return value as number;
};

const list = (value: unknown, context: string): readonly unknown[] => {
  if (!Array.isArray(value)) throw new TypeError(`${context} must be a list`);
  return value;
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

export const decodeHostInspection = (
  value: unknown,
  context = "Uhura host inspection",
): HostInspection => {
  const source = object(value, context);
  exactKeys(
    source,
    [
      "protocol",
      "identityProtocol",
      "entry",
      "machine",
      "presentation",
      "machineProgramHash",
      "presentationHash",
      "evidenceHash",
      "deploymentHash",
      "sources",
      "provenance",
      "interactionGraph",
      "graphSources",
      "evidence",
    ],
    context,
  );
  if (source["protocol"] !== UHURA_HOST_INSPECTION_PROTOCOL) {
    throw new TypeError(
      `${context}.protocol must be ${JSON.stringify(UHURA_HOST_INSPECTION_PROTOCOL)}`,
    );
  }
  const identityProtocol = decodeIdentityProtocol(
    source["identityProtocol"],
    `${context}.identityProtocol`,
  );
  const presentation = source["presentation"];
  if (presentation !== null && (typeof presentation !== "string" || presentation.length === 0)) {
    throw new TypeError(`${context}.presentation must be nonempty text or null`);
  }
  const presentationHash = source["presentationHash"] === null
    ? null
    : hash(text(source["presentationHash"], `${context}.presentationHash`));
  if ((presentation === null) !== (presentationHash === null)) {
    throw new TypeError(
      `${context}.presentation and presentationHash must either both be null or both be present`,
    );
  }
  const sources = list(source["sources"], `${context}.sources`).map(
    (value, index): InspectionSource => {
      const item = object(value, `${context}.sources[${index}]`);
      exactKeys(
        item,
        ["file", "path", "sha256", "bytes"],
        `${context}.sources[${index}]`,
      );
      return {
        file: integer(item["file"], `${context}.sources[${index}].file`),
        path: text(item["path"], `${context}.sources[${index}].path`),
        sha256: hash(text(item["sha256"], `${context}.sources[${index}].sha256`)),
        bytes: integer(item["bytes"], `${context}.sources[${index}].bytes`),
      };
    },
  );
  const paths = new Map<string, number>();
  const fileIds = new Set<number>();
  for (const item of sources) {
    if (paths.has(item.path) || fileIds.has(item.file)) {
      throw new TypeError(`${context}.sources must have unique files and paths`);
    }
    paths.set(item.path, item.bytes);
    fileIds.add(item.file);
  }
  const interaction = decodeInteractionGraphArtifacts(
    source["interactionGraph"],
    source["graphSources"],
  );
  if (interaction.graph.identityProtocol !== identityProtocol) {
    throw new TypeError(
      `${context}.interactionGraph identity protocol does not match the deployment`,
    );
  }
  const machine = text(source["machine"], `${context}.machine`);
  const machineProgramHash = hash(
    text(source["machineProgramHash"], `${context}.machineProgramHash`),
  );
  if (interaction.graph.machineProgramHashes[machine] !== machineProgramHash) {
    throw new TypeError(
      `${context}.interactionGraph machine identity does not match the deployment`,
    );
  }
  if (
    presentation !== null
    && interaction.graph.presentationHashes[presentation] !== presentationHash
  ) {
    throw new TypeError(
      `${context}.interactionGraph presentation identity does not match the deployment`,
    );
  }
  for (const entry of [
    ...interaction.sources.nodes,
    ...interaction.sources.edges,
  ]) {
    for (const span of entry.sources) {
      const bytes = paths.get(span.path);
      if (bytes === undefined || span.end > bytes) {
        throw new TypeError(
          `${context}.graphSources span must resolve within the accepted source inventory`,
        );
      }
    }
  }
  const provenance = decodeSemanticProvenance(
    source["provenance"],
    `${context}.provenance`,
  );
  for (const semanticSource of provenance.sources) {
    const physical = sources.find((item) => item.path === semanticSource.path);
    if (
      physical === undefined
      || physical.sha256 !== semanticSource.sha256
      || physical.bytes !== semanticSource.bytes
    ) {
      throw new TypeError(
        `${context}.provenance source must match the accepted source inventory`,
      );
    }
  }
  return {
    protocol: UHURA_HOST_INSPECTION_PROTOCOL,
    identityProtocol,
    entry: text(source["entry"], `${context}.entry`),
    machine,
    presentation,
    machineProgramHash,
    presentationHash,
    evidenceHash: source["evidenceHash"] === null
      ? null
      : hash(text(source["evidenceHash"], `${context}.evidenceHash`)),
    deploymentHash: hash(
      text(source["deploymentHash"], `${context}.deploymentHash`),
    ),
    sources,
    provenance,
    interactionGraph: interaction.graph,
    graphSources: interaction.sources,
    evidence: source["evidence"],
  };
};
