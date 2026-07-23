import "./shell.css";

import { mountPlayChrome } from "./chrome.js";
import { mountPlayDebugSurface } from "./debug-surface.js";
import { startPlayRuntime } from "./main.js";
import { createPlayShell } from "./shell.js";
import { lockPlayPageScale } from "./viewport-lock.js";

export type PlayDispose = () => void;

/** Mounts the complete `/play` route and returns its idempotent cleanup. */
export function mountPlay(root: HTMLElement): PlayDispose {
  const document = root.ownerDocument;
  const shell = createPlayShell(document);
  const applicationStyle = document.createElement("style");
  applicationStyle.dataset["uhuraPlayApplication"] = "";
  const unlockPageScale = lockPlayPageScale(document);
  const hadBodyClass = document.body.classList.contains("uh-play-shell");
  document.body.classList.add("uh-play-shell");
  // The authored application stylesheet lives inside the runtime shadow root,
  // after the base runtime styles, so it still wins the cascade (#30).
  shell.runtimeRoot.append(applicationStyle);
  root.replaceChildren(shell.container);

  let disposed = false;
  let chrome: ReturnType<typeof mountPlayChrome> | null = null;
  let runtime: ReturnType<typeof startPlayRuntime> | null = null;
  let debugSurface: ReturnType<typeof mountPlayDebugSurface> | null = null;
  try {
    chrome = mountPlayChrome(shell);
    runtime = startPlayRuntime(shell, applicationStyle);
    debugSurface = mountPlayDebugSurface(shell, runtime.inspection, {
      onOpenChange: (open) => chrome?.setDebugOpen(open),
    });
  } catch (error) {
    debugSurface?.dispose();
    chrome?.dispose();
    runtime?.dispose();
    applicationStyle.remove();
    shell.container.remove();
    if (!hadBodyClass) document.body.classList.remove("uh-play-shell");
    unlockPageScale();
    throw error;
  }

  return (): void => {
    if (disposed) return;
    disposed = true;
    debugSurface?.dispose();
    runtime?.dispose();
    chrome?.dispose();
    debugSurface = null;
    runtime = null;
    chrome = null;
    applicationStyle.remove();
    shell.container.remove();
    if (!hadBodyClass) document.body.classList.remove("uh-play-shell");
    unlockPageScale();
  };
}
