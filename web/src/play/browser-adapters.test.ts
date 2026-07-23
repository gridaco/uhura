import { describe, expect, it, vi } from "vitest";

import { hash, type Value } from "../protocol/machine.js";
import type { PortAdapterContext } from "./adapter-host.js";
import {
  type AdmittedPortRequirement,
  APPLICATION_PROVIDER_ADAPTER,
  WEB_HISTORY_ADAPTER,
} from "./adapter-host.js";
import {
  createBrowserPortAdapters,
  createWebHistoryAdapter,
  WEB_ROUTER_CONTRACT,
} from "./browser-adapters.js";
import type { UhuraProviderHost } from "./provider.js";

const requirement: AdmittedPortRequirement = {
  port: "router",
  adapter: WEB_HISTORY_ADAPTER,
  contract: WEB_ROUTER_CONTRACT,
  contractHash: hash("1".repeat(64)),
  contractInstanceHash: hash("2".repeat(64)),
};

const location: Value = {
  $: "variant",
  type: "example.app@1::Location",
  case: "orders",
  fields: [],
};

const changed: Value = {
  $: "variant",
  type: "uhura.web_router@1::RouterReceive<Location>",
  case: "changed",
  fields: [{ name: "location", value: location }],
};

const command = (kind: "push" | "replace"): Value => ({
  $: "variant",
  type: "uhura.web_router@1::RouterSend<Location>",
  case: kind,
  fields: [{ name: "location", value: location }],
});

const setup = () => {
  let locationListener: ((url: string) => void) | null = null;
  const host = {
    signal: new AbortController().signal,
    pickFile: vi.fn<UhuraProviderHost["pickFile"]>(),
    port: vi.fn<UhuraProviderHost["port"]>(() => requirement),
    decodeRoute: vi.fn<UhuraProviderHost["decodeRoute"]>(() => ({
      source: "port" as const,
      port: "router",
      value: changed,
    })),
    encodeRoute: vi.fn<UhuraProviderHost["encodeRoute"]>(() => "/orders"),
    onLocation: vi.fn<UhuraProviderHost["onLocation"]>((listener) => {
      locationListener = listener;
      return vi.fn<() => void>();
    }),
    navigate: vi.fn<UhuraProviderHost["navigate"]>(),
    back: vi.fn<UhuraProviderHost["back"]>(),
  } satisfies UhuraProviderHost;
  const deliver = vi.fn<PortAdapterContext["deliver"]>();
  const context = {
    signal: new AbortController().signal,
    deliver,
  } satisfies PortAdapterContext;
  return {
    host,
    context,
    emitLocation: (url: string) => {
      if (locationListener === null) throw new Error("adapter was not started");
      locationListener(url);
    },
  };
};

describe("built-in browser adapters", () => {
  it("constructs web history only for a Router assigned to web.history", () => {
    const { host } = setup();
    expect(createBrowserPortAdapters([requirement], host)).toHaveLength(1);
    expect(createBrowserPortAdapters([{
      ...requirement,
      adapter: APPLICATION_PROVIDER_ADAPTER,
    }], host)).toEqual([]);
  });

  it("rejects unsupported adapter and contract pairs", () => {
    const { host } = setup();
    expect(() => createBrowserPortAdapters([{
      ...requirement,
      contract: "uhura.ports@1::RequestPort",
    }], host)).toThrow(/cannot implement/u);
    expect(() => createBrowserPortAdapters([{
      ...requirement,
      adapter: "unknown.adapter",
    } as never], host)).toThrow(/unknown sealed Uhura adapter/u);
    expect(() => createBrowserPortAdapters([
      requirement,
      { ...requirement, port: "router_backup" },
    ], host)).toThrow(/at most one port/u);
  });

  it("decodes committed browser locations through Wasm", () => {
    const { host, context, emitLocation } = setup();
    const adapter = createWebHistoryAdapter(requirement, host);
    adapter.start?.(context);
    emitLocation("/orders?state=open");
    expect(host.decodeRoute).toHaveBeenCalledWith(
      "router",
      "/orders?state=open",
    );
    expect(context.deliver).toHaveBeenCalledWith(changed);
  });

  it("keeps browser fragments outside the Wasm route contract", () => {
    const { host, context, emitLocation } = setup();
    const adapter = createWebHistoryAdapter(requirement, host);
    adapter.start?.(context);
    emitLocation("/orders?state=open#details");
    expect(host.decodeRoute).toHaveBeenCalledWith(
      "router",
      "/orders?state=open",
    );
    expect(context.deliver).toHaveBeenCalledWith(changed);
  });

  it("encodes push/replace and delegates back without inventing values", () => {
    const { host, context } = setup();
    const adapter = createWebHistoryAdapter(requirement, host);
    adapter.accept(command("push"), context);
    adapter.accept(command("replace"), context);
    adapter.accept({
      $: "variant",
      type: "uhura.web_router@1::RouterSend<Location>",
      case: "back",
      fields: [],
    }, context);
    expect(host.encodeRoute).toHaveBeenCalledTimes(2);
    expect(host.navigate).toHaveBeenNthCalledWith(1, "push", "/orders");
    expect(host.navigate).toHaveBeenNthCalledWith(2, "replace", "/orders");
    expect(host.back).toHaveBeenCalledOnce();
  });
});
