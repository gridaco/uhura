const ICON_FONT_PROTOCOL = "uhura-icon-fonts/0";
const TOKEN_PATTERN = /^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$/;
const SHA256_PATTERN = /^[0-9a-f]{64}$/;

export type IconFontManifestAuthority = "editor" | "play";

export interface IconFontFamilyManifest {
  font: string;
  sha256: string;
  glyphs: Readonly<Record<string, number>>;
}

export interface IconFontManifest {
  protocol: typeof ICON_FONT_PROTOCOL;
  revision?: number;
  generation?: number;
  default: string;
  families: Readonly<Record<string, IconFontFamilyManifest>>;
}

export interface IconFontLoadRequest {
  document: Document;
  font: string;
  sha256: string;
  cssFamily: string;
}

export type IconFontLoader = (request: IconFontLoadRequest) => Promise<void>;

/** Loaded, closed icon-family vocabulary used by one renderer artifact. */
export interface IconFontRegistry {
  readonly defaultFamily: string;
  /** Stable across manifests with identical realization data. */
  readonly fingerprint: string;
  apply(host: HTMLElement, family: string | undefined, name: string): void;
}

export interface LoadIconFontRegistryOptions {
  document: Document;
  manifest: IconFontManifest;
  loadFont?: IconFontLoader;
}

export class IconFontContractError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "IconFontContractError";
  }
}

function fail(path: string, message: string): never {
  throw new IconFontContractError(`${path} ${message}`);
}

function recordOf(value: unknown, path: string): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return fail(path, "must be an object");
  }
  return value as Record<string, unknown>;
}

function expectExactKeys(
  value: Record<string, unknown>,
  path: string,
  expected: readonly string[],
): void {
  const expectedSet = new Set(expected);
  for (const key of Object.keys(value)) {
    if (!expectedSet.has(key)) fail(`${path}.${key}`, "is not allowed");
  }
  for (const key of expected) {
    if (!(key in value)) fail(`${path}.${key}`, "is required");
  }
}

function stringOf(value: unknown, path: string): string {
  if (typeof value !== "string" || value.length === 0) {
    return fail(path, "must be a non-empty string");
  }
  return value;
}

function tokenOf(value: unknown, path: string): string {
  const token = stringOf(value, path);
  if (!TOKEN_PATTERN.test(token)) {
    return fail(path, "must be lowercase kebab-case");
  }
  return token;
}

function positiveIntegerOf(value: unknown, path: string): number {
  if (!Number.isSafeInteger(value) || (value as number) < 1) {
    return fail(path, "must be a positive safe integer");
  }
  return value as number;
}

function isPrivateUseCodepoint(value: number): boolean {
  return (
    (value >= 0xe000 && value <= 0xf8ff) ||
    (value >= 0xf0000 && value <= 0xffffd) ||
    (value >= 0x100000 && value <= 0x10fffd)
  );
}

function decodeGlyphs(
  value: unknown,
  path: string,
): Readonly<Record<string, number>> {
  const source = recordOf(value, path);
  if (Object.keys(source).length === 0) return fail(path, "must not be empty");

  const glyphs: Record<string, number> = {};
  for (const [rawName, rawCodepoint] of Object.entries(source)) {
    const name = tokenOf(rawName, `${path}.${rawName}`);
    if (!Number.isSafeInteger(rawCodepoint) || !isPrivateUseCodepoint(rawCodepoint as number)) {
      fail(`${path}.${rawName}`, "must be a Private Use Area codepoint number");
    }
    glyphs[name] = rawCodepoint as number;
  }
  return glyphs;
}

function decodeFamilies(
  value: unknown,
  path: string,
): Readonly<Record<string, IconFontFamilyManifest>> {
  const source = recordOf(value, path);
  if (Object.keys(source).length === 0) return fail(path, "must not be empty");

  const families: Record<string, IconFontFamilyManifest> = {};
  for (const [rawName, rawFamily] of Object.entries(source)) {
    const name = tokenOf(rawName, `${path}.${rawName}`);
    const family = recordOf(rawFamily, `${path}.${name}`);
    expectExactKeys(family, `${path}.${name}`, ["font", "sha256", "glyphs"]);
    const sha256 = stringOf(family.sha256, `${path}.${name}.sha256`);
    if (!SHA256_PATTERN.test(sha256)) {
      fail(`${path}.${name}.sha256`, "must be a lowercase SHA-256 digest");
    }
    families[name] = {
      font: stringOf(family.font, `${path}.${name}.font`),
      sha256,
      glyphs: decodeGlyphs(family.glyphs, `${path}.${name}.glyphs`),
    };
  }
  return families;
}

/** Strictly decodes the host-owned icon-font resource manifest. */
export function decodeIconFontManifest(
  value: unknown,
  authority: IconFontManifestAuthority,
): IconFontManifest {
  const source = recordOf(value, "icon font manifest");
  const authorityKey = authority === "editor" ? "revision" : "generation";
  expectExactKeys(source, "icon font manifest", [
    "protocol",
    authorityKey,
    "default",
    "families",
  ]);

  if (source.protocol !== ICON_FONT_PROTOCOL) {
    fail("icon font manifest.protocol", `must be ${JSON.stringify(ICON_FONT_PROTOCOL)}`);
  }

  const defaultFamily = tokenOf(source.default, "icon font manifest.default");
  const families = decodeFamilies(source.families, "icon font manifest.families");
  if (!Object.hasOwn(families, defaultFamily)) {
    fail("icon font manifest.default", "must name a declared family");
  }

  const authorityValue = positiveIntegerOf(
    source[authorityKey],
    `icon font manifest.${authorityKey}`,
  );
  return {
    protocol: ICON_FONT_PROTOCOL,
    ...(authority === "editor"
      ? { revision: authorityValue }
      : { generation: authorityValue }),
    default: defaultFamily,
    families,
  };
}

function fontUrl(document: Document, raw: string): URL {
  let base: URL;
  let resolved: URL;
  try {
    base = new URL(document.baseURI);
    resolved = new URL(raw, base);
  } catch {
    return fail("icon font manifest font", "must be a valid URL");
  }
  if (base.protocol !== "http:" && base.protocol !== "https:") {
    return fail("document.baseURI", "must use HTTP or HTTPS to load icon fonts");
  }
  if (resolved.origin !== base.origin) {
    return fail("icon font manifest font", "must be same-origin");
  }
  return resolved;
}

const browserFontLoads = new WeakMap<Document, Map<string, Promise<void>>>();

async function performBrowserFontLoad(request: IconFontLoadRequest): Promise<void> {
  const FontFaceConstructor = request.document.defaultView?.FontFace;
  if (FontFaceConstructor === undefined) {
    throw new IconFontContractError("FontFace is unavailable in this renderer");
  }
  if (request.document.fonts === undefined) {
    throw new IconFontContractError("document.fonts is unavailable in this renderer");
  }

  const face = new FontFaceConstructor(
    request.cssFamily,
    `url(${JSON.stringify(request.font)}) format("woff2")`,
    { style: "normal", weight: "400" },
  );
  try {
    const loaded = await face.load();
    request.document.fonts.add(loaded);
  } catch (error) {
    const detail = error instanceof Error ? `: ${error.message}` : "";
    throw new IconFontContractError(
      `failed to load icon font ${request.sha256}${detail}`,
    );
  }
}

async function loadBrowserFont(request: IconFontLoadRequest): Promise<void> {
  let documentLoads = browserFontLoads.get(request.document);
  if (documentLoads === undefined) {
    documentLoads = new Map();
    browserFontLoads.set(request.document, documentLoads);
  }

  let pending = documentLoads.get(request.sha256);
  if (pending === undefined) {
    pending = performBrowserFontLoad(request);
    documentLoads.set(request.sha256, pending);
  }
  try {
    await pending;
  } catch (error) {
    if (documentLoads.get(request.sha256) === pending) {
      documentLoads.delete(request.sha256);
    }
    throw error;
  }
}

interface LoadedFamily {
  cssFamily: string;
  glyphs: Readonly<Record<string, number>>;
}

async function registryFingerprint(
  document: Document,
  manifest: IconFontManifest,
): Promise<string> {
  const families = Object.entries(manifest.families)
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([name, family]) => [
      name,
      family.sha256,
      Object.entries(family.glyphs).sort(([left], [right]) => left.localeCompare(right)),
    ]);
  const canonical = JSON.stringify([manifest.default, families]);
  const cryptography = document.defaultView?.crypto ?? globalThis.crypto;
  if (cryptography?.subtle === undefined) {
    throw new IconFontContractError("Web Crypto is unavailable for icon resources");
  }
  const digest = await cryptography.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(canonical),
  );
  return [...new Uint8Array(digest)]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

/** Loads every digest before exposing a registry, so rendering cannot fall back. */
export async function loadIconFontRegistry(
  options: LoadIconFontRegistryOptions,
): Promise<IconFontRegistry> {
  const loadFont = options.loadFont ?? loadBrowserFont;
  const fingerprint = await registryFingerprint(options.document, options.manifest);
  const loadedDigests = new Set<string>();
  const families = new Map<string, LoadedFamily>();

  for (const [name, family] of Object.entries(options.manifest.families)) {
    const cssFamily = `uhura-icon-${family.sha256}`;
    const resolvedFont = fontUrl(options.document, family.font);
    if (!loadedDigests.has(family.sha256)) {
      await loadFont({
        document: options.document,
        font: resolvedFont.href,
        sha256: family.sha256,
        cssFamily,
      });
      loadedDigests.add(family.sha256);
    }
    families.set(name, { cssFamily, glyphs: family.glyphs });
  }

  return {
    defaultFamily: options.manifest.default,
    fingerprint,
    apply(host, requestedFamily, name) {
      const familyName = requestedFamily ?? options.manifest.default;
      const family = families.get(familyName);
      if (family === undefined) {
        host.textContent = "";
        host.style.fontFamily = "";
        throw new IconFontContractError(`unknown icon family ${JSON.stringify(familyName)}`);
      }
      if (!Object.hasOwn(family.glyphs, name)) {
        host.textContent = "";
        host.style.fontFamily = "";
        throw new IconFontContractError(
          `unknown icon glyph ${JSON.stringify(name)} in family ${JSON.stringify(familyName)}`,
        );
      }
      const codepoint = family.glyphs[name] as number;

      const glyph = String.fromCodePoint(codepoint);
      if (host.textContent !== glyph) host.textContent = glyph;
      host.style.fontFamily = family.cssFamily;
      host.style.fontSynthesis = "none";
      host.style.fontVariantLigatures = "none";
      host.style.lineHeight = "1";
      host.style.userSelect = "none";
    },
  };
}

export { ICON_FONT_PROTOCOL };
