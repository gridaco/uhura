// Pure selection policy for the browser Play runtime. A live-provider profile
// may keep its fixture for Canvas/check/trace without exposing that deliberately
// partial script as an interactive Play backend.

/** @typedef {"remote" | "fixture"} ProviderMode */

/**
 * @param {import("./types.js").PlayConfig} play
 * @param {string | null} storedProvider
 * @returns {{ provider: ProviderMode, providers: ProviderMode[], clearStoredProvider: boolean }}
 */
export function selectPlayProvider(play, storedProvider) {
  const hasRemote = play.provider.kind === "module";
  /** @type {ProviderMode[]} */
  const providers = hasRemote
    ? play.allow_fixture === false
      ? ["remote"]
      : ["remote", "fixture"]
    : ["fixture"];
  const storedCandidate =
    storedProvider === "remote" || storedProvider === "fixture"
      ? storedProvider
      : null;
  const override =
    storedCandidate !== null && providers.includes(storedCandidate)
      ? storedCandidate
      : null;
  return {
    provider: override ?? (hasRemote ? "remote" : "fixture"),
    providers,
    clearStoredProvider:
      storedProvider !== null &&
      (storedCandidate === null || !providers.includes(storedCandidate)),
  };
}
