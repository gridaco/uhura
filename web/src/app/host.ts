/** Browser-host topology selected by the document that boots Uhura. */

export const UHURA_HOST_CONFIG_PROTOCOL = "uhura-host-config/0" as const;

export interface HostConfig {
  protocol: typeof UHURA_HOST_CONFIG_PROTOCOL;
  mountPath: string;
  mode: "live" | "static";
  /** Origin-local application path, before the mount prefix is applied. */
  playEntry: string;
}

const DEFAULT_HOST_CONFIG: HostConfig = {
  protocol: UHURA_HOST_CONFIG_PROTOCOL,
  mountPath: "/",
  mode: "live",
  playEntry: "/play",
};

interface JsonObject {
  [key: string]: unknown;
}

const asObject = (value: unknown): JsonObject | null =>
  typeof value === "object" && value !== null && !Array.isArray(value)
    ? value as JsonObject
    : null;

const splitPathSuffix = (
  value: string,
): { pathname: string; suffix: string } => {
  const query = value.indexOf("?");
  const fragment = value.indexOf("#");
  const boundary =
    query === -1
      ? fragment
      : fragment === -1
        ? query
        : Math.min(query, fragment);
  return boundary === -1
    ? { pathname: value, suffix: "" }
    : { pathname: value.slice(0, boundary), suffix: value.slice(boundary) };
};

const isAsciiAlphaNumeric = (byte: number): boolean =>
  (byte >= 0x30 && byte <= 0x39)
  || (byte >= 0x41 && byte <= 0x5a)
  || (byte >= 0x61 && byte <= 0x7a);

const isPathSegmentCharacter = (byte: number): boolean =>
  isAsciiAlphaNumeric(byte)
  || "-._~!$&'()*+,;=:@".includes(String.fromCharCode(byte));

const isUrlSuffixCharacter = (byte: number): boolean =>
  isAsciiAlphaNumeric(byte)
  || "-._~!$&()*+,;=:@/?".includes(String.fromCharCode(byte));

const isUnreserved = (byte: number): boolean =>
  isAsciiAlphaNumeric(byte)
  || "-._~".includes(String.fromCharCode(byte));

const isRoutePathComponentCharacter = (byte: number): boolean =>
  isAsciiAlphaNumeric(byte)
  || "-._!~*'()".includes(String.fromCharCode(byte));

const isRouteQueryComponentCharacter = (byte: number): boolean =>
  isRoutePathComponentCharacter(byte) && byte !== 0x27;

const hexValue = (byte: number): number | null => {
  if (byte >= 0x30 && byte <= 0x39) return byte - 0x30;
  if (byte >= 0x41 && byte <= 0x46) return byte - 0x41 + 10;
  if (byte >= 0x61 && byte <= 0x66) return byte - 0x61 + 10;
  return null;
};

const utf8Decoder = new TextDecoder("utf-8", { fatal: true });
const utf8Encoder = new TextEncoder();

const normalizeUrlComponent = (
  value: string,
  label: string,
  rawAllowed: (byte: number) => boolean,
  escapedRawAllowed: (byte: number) => boolean = isUnreserved,
): { canonical: string; decoded: string } => {
  const source = utf8Encoder.encode(value);
  const decodedBytes: number[] = [];
  const canonical: string[] = [];
  for (let index = 0; index < source.length;) {
    const byte = source[index]!;
    if (byte !== 0x25) {
      if (byte > 0x7f || !rawAllowed(byte)) {
        throw new TypeError(
          `${label} contains a character that must be percent-encoded`,
        );
      }
      decodedBytes.push(byte);
      canonical.push(String.fromCharCode(byte));
      index += 1;
      continue;
    }
    const high = source[index + 1] === undefined
      ? null
      : hexValue(source[index + 1]!);
    const low = source[index + 2] === undefined
      ? null
      : hexValue(source[index + 2]!);
    if (high === null || low === null) {
      throw new TypeError(`${label} contains an invalid percent escape`);
    }
    const decodedByte = (high << 4) | low;
    decodedBytes.push(decodedByte);
    canonical.push(
      escapedRawAllowed(decodedByte)
        ? String.fromCharCode(decodedByte)
        : `%${decodedByte.toString(16).toUpperCase().padStart(2, "0")}`,
    );
    index += 3;
  }

  let decoded: string;
  try {
    decoded = utf8Decoder.decode(Uint8Array.from(decodedBytes));
  } catch {
    throw new TypeError(`${label} contains an invalid UTF-8 escape`);
  }
  if ([...decoded].some((character) => {
    const point = character.codePointAt(0)!;
    return point <= 0x1f || (point >= 0x7f && point <= 0x9f);
  })) {
    throw new TypeError(`${label} contains an unsafe control character`);
  }
  return { canonical: canonical.join(""), decoded };
};

const normalizePathSegment = (
  segment: string,
  label: string,
  allowEncodedSlashes: boolean,
): string => {
  const { canonical, decoded } = normalizeUrlComponent(
    segment,
    label,
    isPathSegmentCharacter,
  );
  if (decoded === "." || decoded === ".." || decoded.includes("\\")) {
    throw new TypeError(`${label} contains an unsafe path segment`);
  }
  if (
    decoded.includes("/")
    && (
      !allowEncodedSlashes
      || decoded.split("/").some((part) =>
        part === "" || part === "." || part === ".."
      )
    )
  ) {
    throw new TypeError(`${label} contains an unsafe path segment`);
  }
  return canonical;
};

const normalizeOriginPath = (
  pathname: string,
  label: string,
  directory: boolean,
  allowEncodedSlashes = false,
): string => {
  if (
    !pathname.startsWith("/")
    || pathname.startsWith("//")
    || pathname.includes("\\")
    || pathname.includes("?")
    || pathname.includes("#")
    || [...pathname].some((character) => character.codePointAt(0)! < 0x20)
  ) {
    throw new TypeError(`${label} must be an origin-local path`);
  }
  if (directory && !pathname.endsWith("/")) {
    throw new TypeError(`${label} must end with /`);
  }
  if (pathname === "/") return "/";
  const segments = pathname.split("/");
  const body = segments.slice(1, pathname.endsWith("/") ? -1 : undefined);
  if (body.some((segment) => segment === "")) {
    throw new TypeError(`${label} contains an empty path segment`);
  }
  const normalized = body.map((segment) =>
    normalizePathSegment(segment, label, allowEncodedSlashes)
  );
  return `/${normalized.join("/")}${pathname.endsWith("/") ? "/" : ""}`;
};

const normalizePlayEntryPath = (pathname: string, label: string): string => {
  if (
    !pathname.startsWith("/")
    || pathname.startsWith("//")
    || pathname.includes("\\")
    || pathname.includes("?")
    || pathname.includes("#")
    || [...pathname].some((character) => character.codePointAt(0)! < 0x20)
  ) {
    throw new TypeError(`${label} must be an origin-local path`);
  }
  if (pathname === "/") return "/";
  if (pathname.endsWith("/") && pathname !== "/play/") {
    throw new TypeError(`${label} contains an empty path segment`);
  }
  const body = pathname.slice(1, pathname.endsWith("/") ? -1 : undefined);
  const segments = body.split("/");
  if (segments.some((segment) => segment === "")) {
    throw new TypeError(`${label} contains an empty path segment`);
  }
  const normalized = segments.map((segment) => {
    const { canonical, decoded } = normalizeUrlComponent(
      segment,
      label,
      isRoutePathComponentCharacter,
      isRoutePathComponentCharacter,
    );
    if (decoded === "." || decoded === "..") {
      throw new TypeError(`${label} contains a non-canonical route component`);
    }
    return canonical;
  });
  return `/${normalized.join("/")}${pathname.endsWith("/") ? "/" : ""}`;
};

/** Return the canonical public mount spelling: `/` or `/segment/.../`. */
export const normalizeMountPath = (mountPath: string): string => {
  if (mountPath !== mountPath.trim() || mountPath === "") {
    throw new TypeError("Uhura mount path must be an origin-local path");
  }
  const normalized =
    mountPath === "/" || mountPath.endsWith("/")
      ? mountPath
      : `${mountPath}/`;
  return normalizeOriginPath(normalized, "Uhura mount path", true);
};

/** Internal prefix spelling, without the mount's trailing slash. */
export const normalizeHostBase = (base: string): string => {
  if (base === "") return "";
  const mountPath = normalizeMountPath(base);
  return mountPath === "/" ? "" : mountPath.slice(0, -1);
};

export const prefixHostPath = (base: string, path: string): string => {
  const { pathname, suffix } = splitPathSuffix(path);
  const hostPath = normalizeOriginPath(
    pathname,
    "Uhura host path",
    false,
    true,
  );
  const hostSuffix = normalizeUrlSuffix(suffix, "Uhura host path");
  const normalized = normalizeHostBase(base);
  if (hostPath === "/") {
    return `${normalized === "" ? "/" : `${normalized}/`}${hostSuffix}`;
  }
  return `${normalized}${hostPath}${hostSuffix}`;
};

const normalizeUrlSuffix = (suffix: string, label: string): string => {
  if (suffix === "") return "";
  if (suffix.startsWith("?")) {
    const fragmentIndex = suffix.indexOf("#", 1);
    const query = fragmentIndex === -1
      ? suffix.slice(1)
      : suffix.slice(1, fragmentIndex);
    const fragment = fragmentIndex === -1
      ? null
      : suffix.slice(fragmentIndex + 1);
    const normalizedQuery = normalizeUrlComponent(
      query,
      label,
      isUrlSuffixCharacter,
    ).canonical;
    if (fragment === null) return `?${normalizedQuery}`;
    const normalizedFragment = normalizeUrlComponent(
      fragment,
      label,
      isUrlSuffixCharacter,
    ).canonical;
    return `?${normalizedQuery}#${normalizedFragment}`;
  }
  if (suffix.startsWith("#")) {
    return `#${
      normalizeUrlComponent(
        suffix.slice(1),
        label,
        isUrlSuffixCharacter,
      ).canonical
    }`;
  }
  throw new TypeError(`${label} has an invalid URL suffix`);
};

const normalizeRouteQueryComponent = (
  value: string,
  label: string,
): string =>
  normalizeUrlComponent(
    value,
    label,
    isRouteQueryComponentCharacter,
    isRouteQueryComponentCharacter,
  ).canonical;

const normalizeRouteQuery = (query: string, label: string): string => {
  if (query === "") return "";
  const normalized = query.split("&").map((pair) => {
    if (pair === "") {
      throw new TypeError(`${label} query contains an empty pair`);
    }
    const separator = pair.indexOf("=");
    if (
      separator <= 0
      || pair.indexOf("=", separator + 1) !== -1
    ) {
      throw new TypeError(`${label} query contains a malformed pair`);
    }
    const key = normalizeRouteQueryComponent(pair.slice(0, separator), label);
    const value = normalizeRouteQueryComponent(
      pair.slice(separator + 1),
      label,
    );
    return `${key}=${value}`;
  });
  return `?${normalized.join("&")}`;
};

const normalizePlayEntrySuffix = (suffix: string, label: string): string => {
  if (suffix === "") return "";
  if (suffix.startsWith("?")) {
    const fragmentIndex = suffix.indexOf("#", 1);
    const query = fragmentIndex === -1
      ? suffix.slice(1)
      : suffix.slice(1, fragmentIndex);
    const fragment = fragmentIndex === -1
      ? null
      : suffix.slice(fragmentIndex + 1);
    const normalizedQuery = normalizeRouteQuery(query, label);
    if (fragment === null || fragment === "") return normalizedQuery;
    const normalizedFragment = normalizeUrlComponent(
      fragment,
      label,
      isUrlSuffixCharacter,
    ).canonical;
    return normalizedFragment === ""
      ? normalizedQuery
      : `${normalizedQuery}#${normalizedFragment}`;
  }
  if (suffix.startsWith("#")) {
    const fragment = normalizeUrlComponent(
      suffix.slice(1),
      label,
      isUrlSuffixCharacter,
    ).canonical;
    return fragment === "" ? "" : `#${fragment}`;
  }
  throw new TypeError(`${label} has an invalid URL suffix`);
};

const playEntryPathIsReserved = (pathname: string): boolean =>
  pathname === "/"
  || pathname === "/_uhura/editor"
  || pathname === "/_uhura/editor/"
  || pathname === "/api"
  || pathname.startsWith("/api/")
  || pathname === "/assets"
  || pathname.startsWith("/assets/");

export const normalizePlayEntry = (configured: string | undefined): string => {
  const entry = configured === undefined ? "/play" : configured;
  if (entry === "" || entry !== entry.trim()) {
    throw new TypeError("Uhura Play entry must be an origin-local path");
  }
  const { pathname, suffix } = splitPathSuffix(entry);
  const normalizedPath = normalizePlayEntryPath(
    pathname,
    "Uhura Play entry",
  );
  if (playEntryPathIsReserved(normalizedPath)) {
    throw new TypeError("Uhura Play entry must select the Play surface");
  }
  return `${
    normalizedPath
  }${normalizePlayEntrySuffix(suffix, "Uhura Play entry")}`;
};

export const resolvePlayEntry = (
  base: string,
  configured: string | undefined,
): string => {
  const entry = normalizePlayEntry(configured);
  return prefixHostPath(base, entry);
};

export const stripHostPath = (
  base: string,
  pathname: string,
): string | null => {
  const normalized = normalizeHostBase(base);
  if (normalized === "") return pathname;
  if (pathname === normalized || pathname === `${normalized}/`) return "/";
  return pathname.startsWith(`${normalized}/`)
    ? pathname.slice(normalized.length)
    : null;
};

export const decodeHostConfig = (serialized: string): HostConfig => {
  let decoded: unknown;
  try {
    decoded = JSON.parse(serialized);
  } catch (error) {
    throw new TypeError(`Uhura host config is not valid JSON: ${String(error)}`);
  }
  const config = asObject(decoded);
  if (
    config === null
    || config["protocol"] !== UHURA_HOST_CONFIG_PROTOCOL
    || typeof config["mountPath"] !== "string"
    || (config["mode"] !== "live" && config["mode"] !== "static")
    || typeof config["playEntry"] !== "string"
  ) {
    throw new TypeError("Uhura host config has an unsupported shape");
  }
  const mountPath = normalizeMountPath(config["mountPath"]);
  const playEntry = normalizePlayEntry(config["playEntry"]);
  return {
    protocol: UHURA_HOST_CONFIG_PROTOCOL,
    mountPath,
    mode: config["mode"],
    playEntry,
  };
};

const documentHostConfig = (): HostConfig => {
  if (typeof document === "undefined") return DEFAULT_HOST_CONFIG;
  const config = document.getElementById("uhura-host-config");
  if (config === null) {
    throw new TypeError("Uhura document is missing #uhura-host-config");
  }
  return decodeHostConfig(config.textContent ?? "");
};

export const UHURA_HOST_CONFIG = documentHostConfig();
export const UHURA_HOST_BASE = normalizeHostBase(
  UHURA_HOST_CONFIG.mountPath,
);
export const UHURA_STATIC_HOST = UHURA_HOST_CONFIG.mode === "static";
export const UHURA_PLAY_ENTRY = resolvePlayEntry(
  UHURA_HOST_CONFIG.mountPath,
  UHURA_HOST_CONFIG.playEntry,
);

export const escapeHtmlAttribute = (value: string): string =>
  value
    .replaceAll("&", "&amp;")
    .replaceAll("\"", "&quot;")
    .replaceAll("'", "&#39;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;");

export const hostPath = (path: string): string =>
  prefixHostPath(UHURA_HOST_BASE, path);

/** Rebase native-host absolute resources into a mounted host prefix. */
export const rebaseHostResource = (resource: string): string => {
  if (!resource.startsWith("/api/")) return resource;
  return prefixHostPath(UHURA_HOST_BASE, resource);
};

export const rebasePlayAssetForHost = (
  resource: string,
  base: string,
  staticHost: boolean,
): string => {
  if (!resource.startsWith("/api/play/assets/")) return resource;
  const rebased = prefixHostPath(base, resource);
  if (!staticHost) return rebased;
  const { pathname, suffix } = splitPathSuffix(rebased);
  return `${pathname.replaceAll("%2F", "/")}${suffix}`;
};

/** Rebase only the transport namespace owned by Uhura's asset publisher. */
export const rebasePlayAsset = (resource: string): string =>
  rebasePlayAssetForHost(resource, UHURA_HOST_BASE, UHURA_STATIC_HOST);
