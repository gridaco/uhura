import assert from "node:assert/strict";
import { test } from "vitest";

import {
  decodeIconFontManifest,
  IconFontContractError,
  loadIconFontRegistry,
} from "../icons.js";

const FONT_SHA = "a".repeat(64);

function manifest(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    protocol: "uhura-icon-fonts/0",
    generation: 7,
    default: "lucide",
    families: {
      lucide: {
        font: `/api/play/icon-fonts/${FONT_SHA}.woff2`,
        sha256: FONT_SHA,
        glyphs: { heart: 0xe001 },
      },
      brand: {
        font: `/api/play/icon-fonts/${FONT_SHA}.woff2`,
        sha256: FONT_SHA,
        glyphs: { logo: 0xf0000 },
      },
    },
    ...overrides,
  };
}

function fakeDocument(): Document {
  return { baseURI: "https://uhura.test/play/" } as Document;
}

function fakeHost(): HTMLElement {
  return {
    textContent: "",
    style: {
      fontFamily: "",
      fontSynthesis: "",
      fontVariantLigatures: "",
      lineHeight: "",
      userSelect: "",
    },
  } as unknown as HTMLElement;
}

test("the strict manifest loads each font digest once and resolves named glyphs", async () => {
  const decoded = decodeIconFontManifest(manifest(), "play");
  const loads: { font: string; cssFamily: string }[] = [];
  const registry = await loadIconFontRegistry({
    document: fakeDocument(),
    manifest: decoded,
    loadFont: async ({ font, cssFamily }) => {
      loads.push({ font, cssFamily });
    },
  });

  assert.deepEqual(loads, [{
    font: `https://uhura.test/api/play/icon-fonts/${FONT_SHA}.woff2`,
    cssFamily: `uhura-icon-${FONT_SHA}`,
  }]);
  assert.equal(registry.defaultFamily, "lucide");
  assert.notEqual(registry.fingerprint, "");

  const host = fakeHost();
  registry.apply(host, undefined, "heart");
  assert.equal(host.textContent, "\ue001");
  assert.equal(host.style.fontFamily, `uhura-icon-${FONT_SHA}`);
  assert.equal(host.style.fontSynthesis, "none");
  assert.equal(host.style.fontVariantLigatures, "none");
  assert.equal(host.style.userSelect, "none");

  registry.apply(host, "brand", "logo");
  assert.equal(host.textContent, String.fromCodePoint(0xf0000));
  assert.throws(
    () => registry.apply(host, "missing", "heart"),
    /unknown icon family "missing"/,
  );
  assert.equal(host.textContent, "");
  assert.equal(host.style.fontFamily, "");
  assert.throws(
    () => registry.apply(host, "lucide", "missing"),
    /unknown icon glyph "missing" in family "lucide"/,
  );
  assert.equal(host.textContent, "");
  assert.equal(host.style.fontFamily, "");
});

test("manifest authority, shape, hashes, names, and PUA values are closed", () => {
  const editor = manifest({ revision: 9 });
  delete editor.generation;
  assert.equal(decodeIconFontManifest(editor, "editor").revision, 9);

  assert.throws(
    () => decodeIconFontManifest(manifest({ protocol: "uhura-icon-fonts/1" }), "play"),
    /protocol must be "uhura-icon-fonts\/0"/,
  );
  assert.throws(
    () => decodeIconFontManifest({ ...manifest(), extra: true }, "play"),
    /extra is not allowed/,
  );
  assert.throws(
    () => decodeIconFontManifest(manifest({ default: "unknown" }), "play"),
    /default must name a declared family/,
  );
  assert.throws(
    () => decodeIconFontManifest(manifest({ generation: 0 }), "play"),
    /generation must be a positive safe integer/,
  );

  const badHash = manifest();
  ((badHash.families as Record<string, Record<string, unknown>>).lucide!).sha256 = "ABC";
  assert.throws(
    () => decodeIconFontManifest(badHash, "play"),
    /sha256 must be a lowercase SHA-256 digest/,
  );

  const badGlyph = manifest();
  ((badGlyph.families as Record<string, Record<string, unknown>>).lucide!).glyphs = {
    heart: 65,
  };
  assert.throws(
    () => decodeIconFontManifest(badGlyph, "play"),
    /heart must be a Private Use Area codepoint number/,
  );
});

test("the browser loader uses FontFace and admits only same-origin font URLs", async () => {
  const faces: { family: string; source: string }[] = [];
  const added: unknown[] = [];
  class FakeFontFace {
    constructor(family: string, source: string) {
      faces.push({ family, source });
    }

    async load(): Promise<this> {
      return this;
    }
  }
  const document = {
    baseURI: "https://uhura.test/editor/",
    defaultView: { FontFace: FakeFontFace },
    fonts: { add: (face: unknown) => added.push(face) },
  } as unknown as Document;
  const editorManifest = manifest({ revision: 3 });
  delete editorManifest.generation;
  const decoded = decodeIconFontManifest(editorManifest, "editor");
  await loadIconFontRegistry({ document, manifest: decoded });

  assert.deepEqual(faces, [{
    family: `uhura-icon-${FONT_SHA}`,
    source:
      `url("https://uhura.test/api/play/icon-fonts/${FONT_SHA}.woff2") format("woff2")`,
  }]);
  assert.equal(added.length, 1);

  const external = manifest();
  const families = external.families as Record<string, Record<string, unknown>>;
  families.lucide!.font = "https://other.test/icons.woff2";
  families.brand!.font = "https://other.test/icons.woff2";
  await assert.rejects(
    loadIconFontRegistry({
      document,
      manifest: decodeIconFontManifest(external, "play"),
      loadFont: async () => {},
    }),
    (error: unknown) =>
      error instanceof IconFontContractError && /must be same-origin/.test(error.message),
  );
});
