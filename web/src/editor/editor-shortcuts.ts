export type SourceShortcutAction =
  | "open-source"
  | "toggle-annotation-layer";

export interface SourceShortcutInput {
  code: string;
  repeat: boolean;
  shiftKey: boolean;
  metaKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
}

/** Resolves Source and canvas-overlay shortcuts without taking over text editing or OS chords. */
export const sourceShortcutAction = (
  input: SourceShortcutInput,
  textEntryActive: boolean,
): SourceShortcutAction | null => {
  if (
    input.repeat
    || textEntryActive
    || input.metaKey
    || input.ctrlKey
    || input.altKey
  ) {
    return null;
  }
  if (input.code === "KeyY") {
    return input.shiftKey ? "toggle-annotation-layer" : "open-source";
  }
  return null;
};
