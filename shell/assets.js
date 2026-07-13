// Asset references are part of the view protocol, but resolving those
// references belongs to the selected play provider. Fixture play keeps the
// dev-server convention; a remote provider may resolve the same opaque ref to
// a signed/object URL asynchronously. The consuming semantic prop selects the
// fixture rendition (`image`/video poster => JPEG, video source => MP4).

/** @typedef {(assetRef: string) => Promise<string>} ResolveAsset */

/** @param {string} assetRef @param {"jpg" | "mp4"} extension */
function fixtureAssetUrl(assetRef, extension) {
  return `/assets/${encodeURIComponent(assetRef)}.${extension}`;
}

/** @param {HTMLElement} el @param {string} url */
function applyUrl(el, url) {
  // JSON string quoting is valid inside CSS url(...) and prevents quotes in a
  // provider URL from terminating the value.
  el.style.backgroundImage = `url(${JSON.stringify(url)})`;
}

/**
 * Creates the asset appliers for the current play driver.
 *
 * Resolution is cached only by DOM element + asset ref: ordinary reconciles
 * do not re-sign an unchanged asset slot, while a remount asks the provider
 * again. Source and poster are separate slots because one video owns both.
 * The provider owns any cross-element or expiry-aware cache because only it
 * understands the lifetime of the returned URL.
 *
 * @param {ResolveAsset | undefined} resolveAsset
 */
export function createAssets(resolveAsset) {
  /**
   * One element can own independent source and poster resolutions. Keeping a
   * token per slot ensures an older async completion cannot overwrite a newer
   * ref when keyed reconciliation reuses the same DOM node.
   * @type {WeakMap<HTMLElement, Map<string, { assetRef: string | undefined, token: symbol }>>}
   */
  const applied = new WeakMap();

  /** @param {HTMLElement} el */
  function slotsFor(el) {
    let slots = applied.get(el);
    if (!slots) {
      slots = new Map();
      applied.set(el, slots);
    }
    return slots;
  }

  /**
   * @param {HTMLElement} el
   * @param {string} slot
   * @param {string | undefined} assetRef
   * @param {"jpg" | "mp4"} extension
   * @param {() => void} clearUrl
   * @param {(url: string) => void} setUrl
   */
  function applyResolved(el, slot, assetRef, extension, clearUrl, setUrl) {
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

    // Do not display the previous ref while this reused node resolves its new
    // one. The token below prevents an older async completion from winning.
    Promise.resolve()
      .then(() => resolveAsset(assetRef))
      .then((url) => {
        if (slotsFor(el).get(slot)?.token !== token) return;
        if (typeof url !== "string" || url.length === 0) {
          throw new Error(`asset resolver returned no URL for ${assetRef}`);
        }
        setUrl(url);
      })
      .catch((error) => {
        // Superseded requests are irrelevant and should not create misleading
        // console noise. A current failure remains visible and leaves a blank
        // image instead of silently substituting fixture data. Forget that
        // attempt so a later reconcile can retry the same asset reference.
        if (slotsFor(el).get(slot)?.token === token) {
          slotsFor(el).delete(slot);
          console.error(`uhura asset resolution failed for ${assetRef}`, error);
        }
      });
  }

  /** @param {HTMLElement} el @param {string | undefined} assetRef */
  function applyImage(el, assetRef) {
    applyResolved(
      el,
      "image",
      assetRef,
      "jpg",
      () => {
        el.style.backgroundImage = "";
      },
      (url) => applyUrl(el, url),
    );
  }

  /** @param {HTMLVideoElement} el @param {string | undefined} assetRef */
  function applyVideoSource(el, assetRef) {
    applyResolved(
      el,
      "video-source",
      assetRef,
      "mp4",
      () => {
        const hadSource = el.hasAttribute("src");
        el.removeAttribute("src");
        // Removing `src` alone does not stop the previous media resource.
        // `load()` resets playback, but avoid it on the initial empty apply.
        if (hadSource && typeof el.load === "function") {
          el.load();
        }
      },
      (url) => el.setAttribute("src", url),
    );
  }

  /** @param {HTMLVideoElement} el @param {string | undefined} assetRef */
  function applyVideoPoster(el, assetRef) {
    applyResolved(
      el,
      "video-poster",
      assetRef,
      "jpg",
      () => el.removeAttribute("poster"),
      (url) => el.setAttribute("poster", url),
    );
  }

  // `apply` remains the image alias consumed by older shells/tests.
  return { apply: applyImage, applyImage, applyVideoSource, applyVideoPoster };
}
