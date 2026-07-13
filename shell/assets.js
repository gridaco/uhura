// Image asset references are part of the view protocol, but resolving those
// references belongs to the selected play provider. Fixture play keeps the
// dev-server convention; a remote provider may resolve the same opaque ref to
// a signed/object URL asynchronously.

/** @typedef {(assetRef: string) => Promise<string>} ResolveAsset */

/** @param {string} assetRef */
function fixtureAssetUrl(assetRef) {
  return `/assets/${encodeURIComponent(assetRef)}.jpg`;
}

/** @param {HTMLElement} el @param {string} url */
function applyUrl(el, url) {
  // JSON string quoting is valid inside CSS url(...) and prevents quotes in a
  // provider URL from terminating the value.
  el.style.backgroundImage = `url(${JSON.stringify(url)})`;
}

/**
 * Creates one image applier for the current play driver.
 *
 * Resolution is cached only by DOM element + asset ref: ordinary reconciles
 * do not re-sign an unchanged image, while a remount asks the provider again.
 * The provider owns any cross-element or expiry-aware cache because only it
 * understands the lifetime of the returned URL.
 *
 * @param {ResolveAsset | undefined} resolveAsset
 */
export function createAssets(resolveAsset) {
  /** @type {WeakMap<HTMLElement, { assetRef: string | undefined, token: symbol }>} */
  const applied = new WeakMap();

  /** @param {HTMLElement} el @param {string | undefined} assetRef */
  function apply(el, assetRef) {
    const previous = applied.get(el);
    if (previous && previous.assetRef === assetRef) return;

    const token = Symbol(assetRef);
    applied.set(el, { assetRef, token });

    if (!assetRef) {
      el.style.backgroundImage = "";
      return;
    }

    if (!resolveAsset) {
      applyUrl(el, fixtureAssetUrl(assetRef));
      return;
    }

    // Do not display the previous ref while this reused node resolves its new
    // one. The token below prevents an older async completion from winning.
    el.style.backgroundImage = "";
    Promise.resolve()
      .then(() => resolveAsset(assetRef))
      .then((url) => {
        if (applied.get(el)?.token !== token) return;
        if (typeof url !== "string" || url.length === 0) {
          throw new Error(`asset resolver returned no URL for ${assetRef}`);
        }
        applyUrl(el, url);
      })
      .catch((error) => {
        // Superseded requests are irrelevant and should not create misleading
        // console noise. A current failure remains visible and leaves a blank
        // image instead of silently substituting fixture data. Forget that
        // attempt so a later reconcile can retry the same asset reference.
        if (applied.get(el)?.token === token) {
          applied.delete(el);
          console.error(`uhura asset resolution failed for ${assetRef}`, error);
        }
      });
  }

  return { apply };
}
