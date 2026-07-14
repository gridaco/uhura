// Pure selection policy for the browser Play runtime. A live-provider profile
// may keep its fixture for Editor/check/trace without exposing that deliberately
// partial script as an interactive Play backend.

import type { PlayConfig, ProviderMode } from "../protocol/types.js";

export function selectPlayProvider(
  play: PlayConfig,
  storedProvider: string | null,
): {
  provider: ProviderMode;
  providers: ProviderMode[];
  clearStoredProvider: boolean;
} {
  const hasRemote = play.provider.kind === "module";
  const providers: ProviderMode[] = hasRemote
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
