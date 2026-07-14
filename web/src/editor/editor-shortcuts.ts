export type CommentShortcutAction = "open-source" | "toggle-canvas-comments";

export interface CommentShortcutInput {
  code: string;
  repeat: boolean;
  shiftKey: boolean;
  metaKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
}

/** Resolves the comments shortcuts without taking over text editing or OS chords. */
export const commentShortcutAction = (
  input: CommentShortcutInput,
  textEntryActive: boolean,
): CommentShortcutAction | null => {
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
  return input.shiftKey ? "toggle-canvas-comments" : "open-source";
};
