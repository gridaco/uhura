import { describe, expect, it } from "vitest";

import {
  decodeHostConfig,
  escapeHtmlAttribute,
  normalizeHostBase,
  normalizeMountPath,
  normalizePlayEntry,
  prefixHostPath,
  rebasePlayAsset,
  rebasePlayAssetForHost,
  resolvePlayEntry,
  stripHostPath,
} from "./host.js";

describe("mounted host paths", () => {
  it("keeps the native root topology unchanged", () => {
    expect(normalizeHostBase("/")).toBe("");
    expect(normalizeMountPath("/")).toBe("/");
    expect(prefixHostPath("/", "/play")).toBe("/play");
    expect(stripHostPath("/", "/search")).toBe("/search");
  });

  it("mounts Editor, Play, APIs, and application routes below one prefix", () => {
    expect(normalizeHostBase("/demo/")).toBe("/demo");
    expect(prefixHostPath("/demo/", "/")).toBe("/demo/");
    expect(prefixHostPath("/demo/", "/play")).toBe("/demo/play");
    expect(prefixHostPath("/demo/", "/api/play/ir.json"))
      .toBe("/demo/api/play/ir.json");
    expect(stripHostPath("/demo/", "/demo/profile/mira"))
      .toBe("/profile/mira");
    expect(stripHostPath("/demo/", "/docs/")).toBeNull();
  });

  it("canonicalizes encoded paths before they become host identity", () => {
    expect(normalizeMountPath("/space%20name")).toBe("/space%20name/");
    expect(normalizeMountPath("/%eb%8d%b0%eb%aa%a8/"))
      .toBe("/%EB%8D%B0%EB%AA%A8/");
    expect(normalizeMountPath("/%41%3a/")).toBe("/A%3A/");
    expect(normalizePlayEntry("/orders/%e2%82%ac?step=%69tems"))
      .toBe("/orders/%E2%82%AC?step=items");
    expect(normalizePlayEntry("/search?q=a%26b%3dc"))
      .toBe("/search?q=a%26b%3Dc");
    expect(normalizePlayEntry(
      "/orders/return%2fwith%2fslash?step=review%2fconfirm%2b",
    )).toBe(
      "/orders/return%2Fwith%2Fslash?step=review%2Fconfirm%2B",
    );
    expect(normalizePlayEntry("/orders/100?note=%27ok%27#sum%6dary"))
      .toBe("/orders/100?note=%27ok%27#summary");
    expect(normalizePlayEntry("/author's")).toBe("/author's");

    const stableEntry = normalizePlayEntry(
      "/orders/return%2fwith%2fslash?step=review%2fconfirm%2b#details",
    );
    const roundTripped = new URL(stableEntry, "https://example.test");
    expect(
      `${roundTripped.pathname}${roundTripped.search}${roundTripped.hash}`,
    ).toBe(stableEntry);

    for (const mount of [
      "/",
      "/space%20name/",
      "/%EB%8D%B0%EB%AA%A8/",
      "/research&proof/",
    ]) {
      expect(new URL(mount, "https://example.test").pathname).toBe(mount);
      const route = prefixHostPath(mount, "/orders/100");
      expect(stripHostPath(mount, new URL(route, "https://example.test").pathname))
        .toBe("/orders/100");
    }
  });

  it("pins a mounted export to an application-owned Play entry", () => {
    expect(resolvePlayEntry("/", undefined)).toBe("/play");
    expect(resolvePlayEntry("/demo/", "/orders/100?step=items"))
      .toBe("/demo/orders/100?step=items");
    expect(() => resolvePlayEntry("/demo/", "https://example.com"))
      .toThrow(/origin-local path/u);
    for (const reserved of [
      "/",
      "/_uhura/editor",
      "/api",
      "/api/play/config.json",
      "/assets",
      "/assets/app.js",
    ]) {
      expect(() => resolvePlayEntry("/demo/", reserved))
        .toThrow(/select the Play surface/u);
    }
  });

  it("does not reinterpret provider-owned site API URLs as Uhura assets", () => {
    expect(rebasePlayAsset("/api/site/avatar/1")).toBe("/api/site/avatar/1");
    expect(rebasePlayAsset("/api/play/assets/poster%2fone.jpg"))
      .toBe("/api/play/assets/poster%2Fone.jpg");
  });

  it("keeps internal resource identities inside the declared mount", () => {
    expect(prefixHostPath(
      "/demo/",
      "/api/play/assets/poster%2fone.jpg",
    )).toBe("/demo/api/play/assets/poster%2Fone.jpg");
    for (const resource of [
      "/../outside",
      "/%2e%2e/outside",
      "/api/play/assets/poster%2F..%2Foutside",
      "/api/play/assets/%2Foutside",
    ]) {
      expect(() => prefixHostPath("/demo/", resource))
        .toThrow(/unsafe path segment/u);
    }
  });

  it("uses ordinary hierarchy separators for captured files on static hosts", () => {
    expect(rebasePlayAssetForHost(
      "/api/play/assets/gallery%2fsummer%20day.jpg",
      "/demo/",
      true,
    )).toBe("/demo/api/play/assets/gallery/summer%20day.jpg");
    expect(rebasePlayAssetForHost(
      "/api/play/assets/gallery%2fsummer%20day.jpg?download=%2F",
      "/demo/",
      true,
    )).toBe("/demo/api/play/assets/gallery/summer%20day.jpg?download=%2F");
    expect(rebasePlayAssetForHost(
      "/api/play/assets/gallery%2fsummer%20day.jpg",
      "/demo/",
      false,
    )).toBe("/demo/api/play/assets/gallery%2Fsummer%20day.jpg");
  });

  it.each([
    "demo",
    "//example.com/",
    "/demo//",
    "/./",
    "/../",
    "/%2e%2e/",
    "/a%2fb/",
    "/a%5cb/",
    "/demo/?query",
    "/demo/#fragment",
    "/demo\\",
    "/demo space/",
    "/데모/",
    "/demo\"/",
    "/demo`/",
    "/%00/",
    "/%7f/",
    "/%c2%85/",
    "/%ff/",
  ])("rejects unsafe mount path %s", (mountPath) => {
    expect(() => normalizeMountPath(mountPath)).toThrow(/Uhura mount path/u);
  });

  it.each([
    "/orders/",
    "/orders/$identity",
    "/play?query",
    "/play?=value",
    "/play?state=open&&step=review",
    "/play?state=open=again",
    "/play?unsafe=+",
    "/play?unsafe='",
    "/play?unsafe=\u{7f}",
    "/play?unsafe=%C2%85",
    "/play?unsafe=raw space",
    "/play#unsafe#fragment",
  ])("rejects unsafe Play entry %s", (playEntry) => {
    expect(() => normalizePlayEntry(playEntry)).toThrow(/Uhura Play entry/u);
  });

  it("decodes one explicit runtime host contract", () => {
    expect(decodeHostConfig(JSON.stringify({
      protocol: "uhura-host-config/0",
      mountPath: "/products/%75hura/",
      mode: "static",
      playEntry: "/orders/%31%30%30?step=%69tems#summary",
    }))).toEqual({
      protocol: "uhura-host-config/0",
      mountPath: "/products/uhura/",
      mode: "static",
      playEntry: "/orders/100?step=items#summary",
    });
  });

  it("rejects invalid runtime config rather than inferring a host mode", () => {
    expect(() => decodeHostConfig("{}")).toThrow(/unsupported shape/u);
    expect(() => decodeHostConfig(JSON.stringify({
      protocol: "uhura-host-config/0",
      mountPath: "/demo/",
      mode: "maybe",
      playEntry: "/play",
    }))).toThrow(/unsupported shape/u);
  });

  it("escapes configured routes before they enter host-owned markup", () => {
    expect(escapeHtmlAttribute("/demo/?next=\"<&"))
      .toBe("/demo/?next=&quot;&lt;&amp;");
  });
});
