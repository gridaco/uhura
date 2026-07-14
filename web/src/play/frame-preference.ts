// Play chrome preferences are browser-local host state. They must not become
// part of an Uhura application's URL or session model.

export const FRAME_PREFERENCE_KEY = "uhura:play:frame";

export type FramePreference = "mobile" | "desktop";
export type FramePreferenceStorage = Pick<Storage, "getItem" | "setItem">;

/**
 * @param {Storage | undefined} [storage]
 * @returns {FramePreference}
 */
export function readFramePreference(storage?: FramePreferenceStorage): FramePreference {
  try {
    const value = (storage ?? globalThis.localStorage).getItem(FRAME_PREFERENCE_KEY);
    return value === "desktop" ? "desktop" : "mobile";
  } catch {
    return "mobile";
  }
}

/**
 * @param {FramePreference} value
 * @param {Storage | undefined} [storage]
 */
export function writeFramePreference(
  value: FramePreference,
  storage?: FramePreferenceStorage,
): void {
  try {
    (storage ?? globalThis.localStorage).setItem(FRAME_PREFERENCE_KEY, value);
  } catch {
    // Storage can be unavailable for an opaque origin or blocked by browser
    // policy. The active frame still works for the life of this page.
  }
}
