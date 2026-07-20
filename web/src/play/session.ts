import type { Session as WasmSession } from "/api/play/wasm/uhura_wasm.js";

import type { PlayShell } from "./shell.js";
import {
  APPLICATION_PROVIDER_ADAPTER,
  createAdapterHost,
  partitionAdapterRequirements,
  WEB_HISTORY_ADAPTER,
  type AdapterIdentity,
  type AdmittedPortRequirement,
  type AdapterHost,
  type PortAdapter,
  type PortRequirement,
} from "./adapter-host.js";
import {
  UHURA_BROWSER_PROTOCOL,
  decodeIdentityProtocol,
  decodeInspection,
  decodeReactionReceipt,
  decodeResolvedCommand,
  decodeValue,
  hash,
  natural,
  type Hash,
  type Inspection,
  type NaturalText,
  type Observation,
  type Receipt,
  type ReactionReceipt,
  type ResolvedCommand,
  type ResolvedInput,
  type UhuraIdentityProtocol,
  type Value,
} from "../protocol/machine.js";
import type { AssetAppliers } from "../renderer/assets.js";
import type { IconFontRegistry } from "../renderer/icons.js";
import { UHURA_ADAPTER_PROVIDER_PROTOCOL } from "./provider.js";
import {
  createProjectionRenderer,
  decodeRenderDocument,
  type ProjectionRenderer,
  type RenderDocument,
} from "../renderer/projection.js";

export const UHURA_PLAY_CONFIG_PROTOCOL = "uhura-play-config/1" as const;

export interface PlayPortConfig extends PortRequirement {}

export interface PlayProviderConfig {
  readonly protocol: typeof UHURA_ADAPTER_PROVIDER_PROTOCOL;
  readonly module: string;
  readonly config: Readonly<Record<string, unknown>>;
}

export interface PlayConfig {
  readonly protocol: typeof UHURA_PLAY_CONFIG_PROTOCOL;
  readonly identityProtocol: UhuraIdentityProtocol;
  readonly entry: string;
  readonly machine: string;
  readonly presentation: string | null;
  readonly machineProgramHash: Hash;
  readonly presentationHash: Hash | null;
  readonly evidenceHash: Hash | null;
  readonly deploymentHash: Hash;
  readonly lifetime: "application-session";
  readonly instance: string;
  readonly configuration: Value;
  readonly ports: readonly PlayPortConfig[];
  readonly provider: PlayProviderConfig | null;
}

export interface PlayController {
  readonly session: WasmSession;
  dispose(): void;
}

export interface StartPlayOptions {
  readonly shell: PlayShell;
  readonly session: WasmSession;
  readonly config: PlayConfig;
  readonly adapters: readonly PortAdapter[];
  readonly assets?: AssetAppliers;
  readonly icons?: IconFontRegistry;
  /**
   * Publishes only a fully correlated runtime step. Inspection is
   * observational and never participates in machine execution.
   */
  readonly publishInspection: (
    inspection: Inspection,
    receipt: Receipt,
  ) => void;
  /** Reports a recoverable UI projection failure; the machine keeps running. */
  readonly onProjectionError?: (error: ProjectionFailure) => void;
  readonly onError: (error: unknown) => void;
}

export interface BrowserStep {
  readonly receipt: ReactionReceipt;
  readonly observation: Observation;
  readonly commands: readonly ResolvedCommand[];
  readonly presentation: BrowserPresentation;
  readonly inspection: Inspection;
}

export interface ProjectionFailure {
  readonly code: "projection-failed";
  readonly message: string;
  readonly machine: string;
  readonly presentation: string;
  readonly instance: string;
  readonly sequence: NaturalText;
}

export type BrowserPresentation =
  | { readonly kind: "none" }
  | { readonly kind: "view"; readonly view: RenderDocument }
  | { readonly kind: "error"; readonly error: ProjectionFailure };

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

const adapterIdentity = (
  value: unknown,
  context: string,
): AdapterIdentity => {
  if (
    value !== WEB_HISTORY_ADAPTER
    && value !== APPLICATION_PROVIDER_ADAPTER
  ) {
    throw new TypeError(`${context} is not in the sealed Uhura adapter table`);
  }
  return value;
};

const list = (value: unknown, context: string): readonly unknown[] => {
  if (!Array.isArray(value)) {
    throw new TypeError(`${context} must be a list`);
  }
  return value;
};

const exactKeys = (
  value: Readonly<Record<string, unknown>>,
  required: readonly string[],
  context: string,
  optional: readonly string[] = [],
): void => {
  const expected = new Set([...required, ...optional]);
  const missing = required.filter((key) => !Object.hasOwn(value, key));
  const extra = Object.keys(value).filter((key) => !expected.has(key));
  if (missing.length > 0 || extra.length > 0) {
    throw new TypeError(
      `${context} has the wrong fields; missing [${missing.join(", ")}], extra [${extra.join(", ")}]`,
    );
  }
};

const parseJson = (source: string, context: string): unknown => {
  try {
    return JSON.parse(source) as unknown;
  } catch (error) {
    throw new TypeError(`${context} is not JSON: ${String(error)}`);
  }
};

const decodeProvider = (value: unknown): PlayProviderConfig | null => {
  if (value === undefined || value === null) return null;
  const provider = object(value, "Uhura Play config.provider");
  exactKeys(
    provider,
    ["protocol", "module", "config"],
    "Uhura Play config.provider",
  );
  if (provider["protocol"] !== UHURA_ADAPTER_PROVIDER_PROTOCOL) {
    throw new TypeError(
      `Uhura Play config.provider.protocol must be ${JSON.stringify(UHURA_ADAPTER_PROVIDER_PROTOCOL)}`,
    );
  }
  return {
    protocol: UHURA_ADAPTER_PROVIDER_PROTOCOL,
    module: text(provider["module"], "Uhura Play config.provider.module"),
    config: object(provider["config"], "Uhura Play config.provider.config"),
  };
};

export const decodePlayConfig = (value: unknown): PlayConfig => {
  const config = object(value, "Uhura Play config");
  exactKeys(
    config,
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
      "lifetime",
      "instance",
      "configuration",
      "ports",
    ],
    "Uhura Play config",
    ["provider"],
  );
  if (config["protocol"] !== UHURA_PLAY_CONFIG_PROTOCOL) {
    throw new TypeError(
      `Uhura Play config.protocol must be ${JSON.stringify(UHURA_PLAY_CONFIG_PROTOCOL)}`,
    );
  }
  const identityProtocol = decodeIdentityProtocol(
    config["identityProtocol"],
    "Uhura Play config.identityProtocol",
  );
  if (config["lifetime"] !== "application-session") {
    throw new TypeError(
      "Uhura Play config.lifetime must be `application-session`",
    );
  }
  const presentation = config["presentation"];
  if (presentation !== null && typeof presentation !== "string") {
    throw new TypeError("Uhura Play config.presentation must be text or null");
  }
  if (presentation === "") {
    throw new TypeError(
      "Uhura Play config.presentation must be nonempty when present",
    );
  }
  const presentationHash = config["presentationHash"] === null
    ? null
    : hash(
      text(config["presentationHash"], "Uhura Play config.presentationHash"),
    );
  if ((presentation === null) !== (presentationHash === null)) {
    throw new TypeError(
      "Uhura Play config.presentation and presentationHash must either both be null or both be present",
    );
  }
  const evidenceHash = config["evidenceHash"] === null
    ? null
    : hash(text(config["evidenceHash"], "Uhura Play config.evidenceHash"));
  const ports = list(config["ports"], "Uhura Play config.ports").map(
    (value, index): PlayPortConfig => {
      const context = `Uhura Play config.ports[${index}]`;
      const port = object(value, context);
      exactKeys(
        port,
        ["port", "adapter", "contractHash", "contractInstanceHash"],
        context,
      );
      return {
        port: text(port["port"], `${context}.port`),
        adapter: adapterIdentity(port["adapter"], `${context}.adapter`),
        contractHash: hash(text(port["contractHash"], `${context}.contractHash`)),
        contractInstanceHash: hash(
          text(port["contractInstanceHash"], `${context}.contractInstanceHash`),
        ),
      };
    },
  );
  const names = new Set<string>();
  for (const port of ports) {
    if (names.has(port.port)) {
      throw new TypeError(`Uhura Play config repeats port \`${port.port}\``);
    }
    names.add(port.port);
  }
  const provider = decodeProvider(config["provider"]);
  const needsProvider = ports.some(
    (port) => port.adapter === APPLICATION_PROVIDER_ADAPTER,
  );
  if (needsProvider !== (provider !== null)) {
    throw new TypeError(
      needsProvider
        ? "Uhura Play config binds app.provider ports but has no provider module"
        : "Uhura Play config has a provider module but binds no app.provider ports",
    );
  }
  return {
    protocol: UHURA_PLAY_CONFIG_PROTOCOL,
    identityProtocol,
    entry: text(config["entry"], "Uhura Play config.entry"),
    machine: text(config["machine"], "Uhura Play config.machine"),
    presentation: typeof presentation === "string" ? presentation : null,
    machineProgramHash: hash(
      text(config["machineProgramHash"], "Uhura Play config.machineProgramHash"),
    ),
    presentationHash,
    evidenceHash,
    deploymentHash: hash(
      text(config["deploymentHash"], "Uhura Play config.deploymentHash"),
    ),
    lifetime: "application-session",
    instance: text(config["instance"], "Uhura Play config.instance"),
    configuration: decodeValue(
      config["configuration"],
      "Uhura Play config.configuration",
    ),
    ports,
    provider,
  };
};

export interface WasmPortRequirement extends Omit<PortRequirement, "adapter"> {
  readonly contract: string;
}

export const decodePortRequirements = (
  source: string,
): WasmPortRequirement[] => {
  const requirements = list(
    parseJson(source, "Uhura port requirements"),
    "Uhura port requirements",
  ).map((value, index): WasmPortRequirement => {
    const context = `Uhura port requirements[${index}]`;
    const requirement = object(value, context);
    exactKeys(
      requirement,
      ["port", "contract", "contractHash", "contractInstanceHash"],
      context,
    );
    return {
      port: text(requirement["port"], `${context}.port`),
      contract: text(requirement["contract"], `${context}.contract`),
      contractHash: hash(
        text(requirement["contractHash"], `${context}.contractHash`),
      ),
      contractInstanceHash: hash(
        text(
          requirement["contractInstanceHash"],
          `${context}.contractInstanceHash`,
        ),
      ),
    };
  });
  const names = new Set<string>();
  for (const requirement of requirements) {
    if (names.has(requirement.port)) {
      throw new TypeError(
        `Uhura machine runtime repeats port requirement \`${requirement.port}\``,
      );
    }
    names.add(requirement.port);
  }
  return requirements;
};

const sameWireValue = (left: unknown, right: unknown): boolean =>
  JSON.stringify(left) === JSON.stringify(right);

const requireSameWireValue = (
  left: unknown,
  right: unknown,
  message: string,
): void => {
  if (!sameWireValue(left, right)) throw new TypeError(message);
};

const validateInspectionIdentity = (
  inspection: Inspection,
  config: PlayConfig,
): void => {
  if (inspection.identityProtocol !== config.identityProtocol) {
    throw new TypeError(
      "Uhura machine runtime identity protocol differs from Play config",
    );
  }
  if (inspection.instance !== config.instance) {
    throw new TypeError(
      "Uhura machine runtime instance differs from Play config",
    );
  }
  if (inspection.machineProgramHash !== config.machineProgramHash) {
    throw new TypeError(
      "Uhura machine runtime machine identity differs from Play config",
    );
  }
  if (inspection.presentation !== config.presentation) {
    throw new TypeError(
      "Uhura machine runtime presentation differs from Play config",
    );
  }
  if (inspection.presentationHash !== config.presentationHash) {
    throw new TypeError(
      "Uhura machine runtime presentation identity differs from Play config",
    );
  }
  requireSameWireValue(
    inspection.configuration,
    config.configuration,
    "Uhura machine runtime configuration differs from Play config",
  );
};

const validateViewIdentity = (
  view: RenderDocument,
  receipt: Receipt,
  config: PlayConfig,
): void => {
  if (config.presentation === null) {
    throw new TypeError("headless Uhura Play received an undeclared view");
  }
  if (
    view.instance !== config.instance
    || view.machine !== config.machine
    || view.presentation !== config.presentation
    || view.sequence !== receipt.sequence
  ) {
    throw new TypeError(
      "Uhura reaction view identity or sequence differs from its admitted receipt",
    );
  }
};

const decodeProjectionFailure = (
  value: unknown,
  receipt: Receipt,
  config: PlayConfig,
  context: string,
): ProjectionFailure => {
  if (config.presentation === null) {
    throw new TypeError("headless Uhura Play received a projection error");
  }
  const failure = object(value, context);
  exactKeys(
    failure,
    ["code", "message", "machine", "presentation", "instance", "sequence"],
    context,
  );
  if (failure["code"] !== "projection-failed") {
    throw new TypeError(`${context}.code must be \`projection-failed\``);
  }
  const decoded: ProjectionFailure = {
    code: "projection-failed",
    message: text(failure["message"], `${context}.message`),
    machine: text(failure["machine"], `${context}.machine`),
    presentation: text(
      failure["presentation"],
      `${context}.presentation`,
    ),
    instance: text(failure["instance"], `${context}.instance`),
    sequence: natural(text(failure["sequence"], `${context}.sequence`)),
  };
  if (
    decoded.machine !== config.machine
    || decoded.presentation !== config.presentation
    || decoded.instance !== config.instance
    || decoded.sequence !== receipt.sequence
  ) {
    throw new TypeError(
      "Uhura projection error identity or sequence differs from its admitted receipt",
    );
  }
  return decoded;
};

const decodeBrowserPresentation = (
  value: unknown,
  receipt: Receipt,
  config: PlayConfig,
  context: string,
): BrowserPresentation => {
  const presentation = object(value, context);
  const kind = text(presentation["kind"], `${context}.kind`);
  switch (kind) {
    case "none":
      exactKeys(presentation, ["kind"], context);
      if (config.presentation !== null) {
        throw new TypeError(
          "presented Uhura Play omitted both its view and projection error",
        );
      }
      return { kind: "none" };
    case "view": {
      exactKeys(presentation, ["kind", "view"], context);
      const view = decodeRenderDocument(
        presentation["view"],
        `${context}.view`,
      );
      validateViewIdentity(view, receipt, config);
      return { kind: "view", view };
    }
    case "error":
      exactKeys(presentation, ["kind", "error"], context);
      return {
        kind: "error",
        error: decodeProjectionFailure(
          presentation["error"],
          receipt,
          config,
          `${context}.error`,
        ),
      };
    default:
      throw new TypeError(`${context}.kind is not supported`);
  }
};

/**
 * Validates one complete Wasm reaction boundary before Play mutates the DOM or
 * publishes a command to an adapter.
 */
export const decodePlayStep = (
  source: string,
  inspectionSource: string,
  config: PlayConfig,
): BrowserStep => {
  const step = object(
    parseJson(source, "Uhura reaction step"),
    "Uhura reaction step",
  );
  exactKeys(
    step,
    ["protocol", "receipt", "observation", "commands", "presentation"],
    "Uhura reaction step",
  );
  if (step["protocol"] !== UHURA_BROWSER_PROTOCOL) {
    throw new TypeError("Uhura reaction step has an unsupported protocol");
  }
  const receipt = decodeReactionReceipt(
    step["receipt"],
    "Uhura reaction step.receipt",
  );
  const observation = decodeValue(
    step["observation"],
    "Uhura reaction step.observation",
  );
  const commands = list(step["commands"], "Uhura reaction step.commands").map(
    (command, index) =>
      decodeResolvedCommand(
        command,
        `Uhura reaction step.commands[${index}]`,
      ),
  );
  const presentation = decodeBrowserPresentation(
    step["presentation"],
    receipt,
    config,
    "Uhura reaction step.presentation",
  );
  requireSameWireValue(
    commands,
    receipt.orderedCommands,
    "Uhura reaction step commands differ from receipt.orderedCommands",
  );
  requireSameWireValue(
    observation,
    receipt.postObservation,
    "Uhura reaction step observation differs from receipt.postObservation",
  );

  const inspection = decodeInspection(
    parseJson(inspectionSource, "Uhura reaction inspection"),
    "Uhura reaction inspection",
  );
  validateInspectionIdentity(inspection, config);
  if (
    receipt.instance !== inspection.instance
    || receipt.machineProgramHash !== inspection.machineProgramHash
    || receipt.configurationHash !== inspection.configurationHash
  ) {
    throw new TypeError(
      "Uhura reaction receipt identity differs from the inspected runtime",
    );
  }
  const latest = inspection.receipts.at(-1);
  if (latest?.kind !== "reaction") {
    throw new TypeError(
      "Uhura reaction inspection does not end in a reaction receipt",
    );
  }
  requireSameWireValue(
    receipt,
    latest,
    "Uhura reaction step receipt differs from the latest inspected receipt",
  );
  return { receipt, observation, commands, presentation, inspection };
};

export const admitConfiguredPorts = (
  requirements: readonly WasmPortRequirement[],
  config: PlayConfig,
): AdmittedPortRequirement[] => {
  const configured = new Map(config.ports.map((port) => [port.port, port]));
  const admitted: AdmittedPortRequirement[] = [];
  for (const requirement of requirements) {
    const port = configured.get(requirement.port);
    if (!port) {
      throw new Error(
        `Uhura Play has no configured adapter for \`${requirement.port}\``,
      );
    }
    if (
      port.contractHash !== requirement.contractHash
      || port.contractInstanceHash !== requirement.contractInstanceHash
    ) {
      throw new Error(
        `Uhura Play contract identity does not match port \`${port.port}\``,
      );
    }
    admitted.push({ ...requirement, adapter: port.adapter });
  }
  for (const port of configured.keys()) {
    if (!requirements.some((requirement) => requirement.port === port)) {
      throw new Error(`Uhura Play config binds undeclared port \`${port}\``);
    }
  }
  partitionAdapterRequirements(admitted);
  return admitted;
};

const showProjectionFailure = (
  root: HTMLElement,
  failure: ProjectionFailure,
): void => {
  const notice = root.ownerDocument.createElement("section");
  notice.className = "uh-projection-error";
  notice.setAttribute("role", "alert");
  notice.setAttribute("aria-live", "polite");
  const heading = root.ownerDocument.createElement("strong");
  heading.textContent = "Presentation unavailable";
  const message = root.ownerDocument.createElement("p");
  message.textContent = failure.message;
  const context = root.ownerDocument.createElement("code");
  context.textContent = `${failure.presentation} at reaction ${failure.sequence}`;
  notice.append(heading, message, context);
  root.replaceChildren(notice);
};

export function startPlay(
  options: StartPlayOptions,
): PlayController {
  const { shell, session, config } = options;
  const identityInspection = decodeInspection(
    parseJson(session.inspect(), "Uhura identity inspection"),
    "Uhura identity inspection",
  );
  validateInspectionIdentity(identityInspection, config);

  const requirements = admitConfiguredPorts(
    decodePortRequirements(session.port_requirements()),
    config,
  );
  const initialReceipt = identityInspection.receipts.at(-1);
  if (!initialReceipt) {
    throw new TypeError("Uhura initial inspection has no admitted receipt");
  }
  const initialPresentation = decodeBrowserPresentation(
    parseJson(session.presentation(), "Uhura initial presentation"),
    initialReceipt,
    config,
    "Uhura initial presentation",
  );

  let disposed = false;
  let renderer: ProjectionRenderer | null = null;
  let adapters: AdapterHost | null = null;
  let currentView: RenderDocument | null = null;

  function createRenderer(): ProjectionRenderer {
    return createProjectionRenderer({
      root: shell.pageHost,
      mode: "play",
      assets: options.assets,
      icons: options.icons,
      dispatch(binding, event): void {
        if (disposed || currentView === null) return;
        try {
          applyStep(
            session.dispatch_ui(
              binding,
              currentView.sequence,
              JSON.stringify(event),
            ),
          );
        } catch (error) {
          options.onError(error);
        }
      },
    });
  }

  function applyPresentation(presentation: BrowserPresentation): void {
    switch (presentation.kind) {
      case "none":
        currentView = null;
        renderer?.dispose();
        renderer = null;
        return;
      case "error":
        currentView = null;
        renderer?.dispose();
        renderer = null;
        showProjectionFailure(shell.pageHost, presentation.error);
        options.onProjectionError?.(presentation.error);
        return;
      case "view":
        renderer ??= createRenderer();
        currentView = null;
        renderer.render(presentation.view);
        currentView = presentation.view;
        return;
    }
  }

  function applyStep(source: string): void {
    const step = decodePlayStep(source, session.inspect(), config);
    options.publishInspection(step.inspection, step.receipt);
    // Committed commands are never contingent on optional UI projection or
    // DOM reconciliation. Adapter delivery therefore precedes presentation.
    adapters?.publish(step.commands);
    applyPresentation(step.presentation);
  }

  const submit = (input: ResolvedInput): void => {
    if (disposed) return;
    try {
      applyStep(session.submit(JSON.stringify(input)));
    } catch (error) {
      options.onError(error);
    }
  };

  applyPresentation(initialPresentation);

  adapters = createAdapterHost({
    requirements,
    adapters: options.adapters,
    deliver: submit,
    localCommand(command): void {
      options.onError(
        new Error(
          `Uhura Play has no host target for local command ${JSON.stringify(command)}`,
        ),
      );
    },
    adapterError(error, port): void {
      options.onError(
        error instanceof Error
          ? error
          : new Error(`Uhura adapter ${port} failed: ${String(error)}`),
      );
    },
  });

  options.publishInspection(identityInspection, initialReceipt);
  adapters.start();

  return {
    session,
    dispose(): void {
      if (disposed) return;
      disposed = true;
      adapters?.dispose();
      adapters = null;
      renderer?.dispose();
      renderer = null;
      session.free();
    },
  };
}
