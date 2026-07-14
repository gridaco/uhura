import type { SurfaceLoader } from "./router.js";
import { createRouter } from "./router.js";

const root = document.getElementById("uhura-root");
if (!root) throw new Error("Uhura application entry lost #uhura-root");

const loadEditor: SurfaceLoader = async () => {
  const { mountEditor } = await import("../editor/mount.js");
  return mountEditor;
};

const loadPlay: SurfaceLoader = async () => {
  const { mountPlay } = await import("../play/mount.js");
  return mountPlay;
};

createRouter({ root, loadEditor, loadPlay }).start();
