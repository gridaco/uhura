import type { Value } from "../protocol/machine.js";
import type {
  AdmittedPortRequirement,
  PortAdapter,
  PortAdapterContext,
} from "./adapter-host.js";
import {
  partitionAdapterRequirements,
  WEB_HISTORY_ADAPTER,
  WEB_ROUTER_CONTRACT,
} from "./adapter-host.js";
import type { UhuraProviderHost } from "./provider.js";
export { WEB_ROUTER_CONTRACT } from "./adapter-host.js";

const locationField = (command: Value): Value => {
  if (command.$ !== "variant") {
    throw new TypeError("Uhura web-history command must be a variant");
  }
  if (command.fields.length !== 1 || command.fields[0]?.name !== "location") {
    throw new TypeError(
      `Uhura web-history \`${command.case}\` command must contain exactly one named \`location\` field`,
    );
  }
  return command.fields[0].value;
};

/**
 * Implements the sealed browser-history capability for one checked Router
 * port. Route encoding and decoding stay in Wasm, so this adapter owns only
 * browser effects and never reconstructs an Uhura value or route table.
 */
export const createWebHistoryAdapter = (
  requirement: AdmittedPortRequirement,
  host: UhuraProviderHost,
): PortAdapter => {
  if (requirement.adapter !== WEB_HISTORY_ADAPTER) {
    throw new TypeError(
      `Uhura web history cannot take ownership of ${JSON.stringify(requirement.adapter)}`,
    );
  }
  if (requirement.contract !== WEB_ROUTER_CONTRACT) {
    throw new TypeError(
      `Uhura web history cannot implement ${JSON.stringify(requirement.contract)}`,
    );
  }
  let stop: (() => void) | null = null;
  return {
    port: requirement.port,
    adapter: requirement.adapter,
    contractHash: requirement.contractHash,
    contractInstanceHash: requirement.contractInstanceHash,
    start(context: PortAdapterContext): void {
      stop = host.onLocation((url) => {
        context.deliver(host.decodeRoute(requirement.port, url).value);
      });
    },
    accept(command): void {
      if (command.$ !== "variant") {
        throw new TypeError("Uhura web-history command must be a variant");
      }
      switch (command.case) {
        case "push":
        case "replace": {
          const url = host.encodeRoute(
            requirement.port,
            locationField(command),
          );
          host.navigate(command.case, url);
          return;
        }
        case "back":
          if (command.fields.length !== 0) {
            throw new TypeError(
              "Uhura web-history `back` command cannot contain fields",
            );
          }
          host.back();
          return;
        default:
          throw new TypeError(
            `unknown Uhura web-history command \`${command.case}\``,
          );
      }
    },
    dispose(): void {
      stop?.();
      stop = null;
    },
  };
};

/** Creates every host-owned browser adapter required by the checked machine. */
export const createBrowserPortAdapters = (
  requirements: readonly AdmittedPortRequirement[],
  host: UhuraProviderHost,
): PortAdapter[] => {
  const { browser } = partitionAdapterRequirements(requirements);
  return browser
    .map((requirement) => createWebHistoryAdapter(requirement, host));
};
