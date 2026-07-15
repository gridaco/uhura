export type SourceShortcutAction = "open-source" | "toggle-workflow-connectors";

export interface SourceShortcutInput {
  code: string;
  repeat: boolean;
  shiftKey: boolean;
  metaKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
}

/** Resolves Source and workflow shortcuts without taking over text editing or OS chords. */
export const sourceShortcutAction = (
  input: SourceShortcutInput,
  textEntryActive: boolean,
): SourceShortcutAction | null => {
  if (
    input.code !== "KeyY"
    || input.repeat
    || textEntryActive
    || input.metaKey
    || input.ctrlKey
    || input.altKey
  ) {
    return null;
  }
  return input.shiftKey ? "toggle-workflow-connectors" : "open-source";
};
