import { rebasePlayAsset } from "../app/host.js";

export interface AssetAppliers {
  applyImage(el: HTMLImageElement, assetRef: string | undefined): void;
  applyVideoSource(el: HTMLVideoElement, assetRef: string | undefined): void;
  applyVideoPoster(el: HTMLVideoElement, assetRef: string | undefined): void;
  dispose?(): void;
}

export interface EditorAsset {
  dataUri: string;
  alt: string;
}

export type EditorAssetTable = Record<string, EditorAsset>;
export type ResolveAsset = (assetRef: string) => Promise<string>;

type AssetExtension = "jpg" | "mp4";

interface AppliedSlot {
  assetRef: string | undefined;
  token: symbol;
}

function fixtureAssetUrl(assetRef: string, extension: AssetExtension): string {
  return rebasePlayAsset(
    `/api/play/assets/${encodeURIComponent(assetRef)}.${extension}`,
  );
}

function applyImageUrl(el: HTMLImageElement, url: string | undefined): void {
  if (url === undefined) el.removeAttribute("src");
  else el.setAttribute("src", url);
}

/** Deterministic local-only stand-in for an unresolved Editor media id. */
function fallbackEditorAsset(assetRef: string): string {
  let hue = 0;
  for (const byte of new TextEncoder().encode(assetRef)) {
    hue = (hue * 31 + byte) % 360;
  }
  const svg =
    `<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 64 64'>`
    + `<rect width='64' height='64' fill='hsl(${hue},35%,72%)'/>`
    + `<path d='M0 64 64 0v64z' fill='hsl(${hue},40%,48%)'/></svg>`;
  return `data:image/svg+xml;utf8,${svg.replaceAll("#", "%23")}`;
}

function editorAssetUrl(
  table: EditorAssetTable,
  assetRef: string | undefined,
): string | undefined {
  if (assetRef === undefined) return undefined;
  return table[assetRef]?.dataUri ?? fallbackEditorAsset(assetRef);
}

/** Synchronous, local-only asset lookup used by Editor previews. */
export function createEditorAssets(table: EditorAssetTable): AssetAppliers {
  return {
    applyImage(el, assetRef) {
      applyImageUrl(el, editorAssetUrl(table, assetRef));
    },
    applyVideoPoster(el, assetRef) {
      const url = editorAssetUrl(table, assetRef);
      if (url === undefined) el.removeAttribute("poster");
      else el.setAttribute("poster", url);
    },
    applyVideoSource(el) {
      // Editor previews never select, fetch, or play a media source.
      el.removeAttribute("src");
    },
  };
}

/** Creates asset appliers for the canonical Play projection renderer. */
export function createPlayAssets(resolveAsset?: ResolveAsset): AssetAppliers {
  const applied = new WeakMap<HTMLElement, Map<string, AppliedSlot>>();
  let disposed = false;

  function slotsFor(el: HTMLElement): Map<string, AppliedSlot> {
    let slots = applied.get(el);
    if (!slots) {
      slots = new Map();
      applied.set(el, slots);
    }
    return slots;
  }

  function applyResolved(
    el: HTMLElement,
    slot: string,
    assetRef: string | undefined,
    extension: AssetExtension,
    clearUrl: () => void,
    setUrl: (url: string) => void,
  ): void {
    if (disposed) return;
    const slots = slotsFor(el);
    const previous = slots.get(slot);
    if (previous && previous.assetRef === assetRef) return;

    const token = Symbol(assetRef);
    slots.set(slot, { assetRef, token });
    clearUrl();

    if (!assetRef) return;

    if (!resolveAsset) {
      setUrl(fixtureAssetUrl(assetRef, extension));
      return;
    }

    Promise.resolve()
      .then(() => resolveAsset(assetRef))
      .then((url) => {
        if (disposed) return;
        if (slotsFor(el).get(slot)?.token !== token) return;
        if (typeof url !== "string" || url.length === 0) {
          throw new Error(`asset resolver returned no URL for ${assetRef}`);
        }
        setUrl(url);
      })
      .catch((error: unknown) => {
        if (!disposed && slotsFor(el).get(slot)?.token === token) {
          slotsFor(el).delete(slot);
          console.error(`uhura asset resolution failed for ${assetRef}`, error);
        }
      });
  }

  function applyImage(el: HTMLImageElement, assetRef: string | undefined): void {
    applyResolved(
      el,
      "image",
      assetRef,
      "jpg",
      () => applyImageUrl(el, undefined),
      (url) => applyImageUrl(el, url),
    );
  }

  function applyVideoSource(el: HTMLVideoElement, assetRef: string | undefined): void {
    applyResolved(
      el,
      "video-source",
      assetRef,
      "mp4",
      () => {
        const hadSource = el.hasAttribute("src");
        el.removeAttribute("src");
        if (hadSource && typeof el.load === "function") el.load();
      },
      (url) => el.setAttribute("src", url),
    );
  }

  function applyVideoPoster(el: HTMLVideoElement, assetRef: string | undefined): void {
    applyResolved(
      el,
      "video-poster",
      assetRef,
      "jpg",
      () => el.removeAttribute("poster"),
      (url) => el.setAttribute("poster", url),
    );
  }

  return {
    applyImage,
    applyVideoSource,
    applyVideoPoster,
    dispose() {
      disposed = true;
    },
  };
}
