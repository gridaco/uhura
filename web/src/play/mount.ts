import "./shell.css";

import { mountPlayChrome } from "./chrome.js";
import { startPlayRuntime } from "./main.js";
import { createPlayShell } from "./shell.js";

export type PlayDispose = () => void;

/** Mounts the complete `/play` route and returns its idempotent cleanup. */
export function mountPlay(root: HTMLElement): PlayDispose {
  const document = root.ownerDocument;
  const shell = createPlayShell(document);
  const applicationStyle = document.createElement("style");
  applicationStyle.dataset["uhuraPlayApplication"] = "";
  const hadBodyClass = document.body.classList.contains("uh-play-shell");
  document.body.classList.add("uh-play-shell");
  root.replaceChildren(shell.container, applicationStyle);

  let disposed = false;
  let chrome: ReturnType<typeof mountPlayChrome> | null = null;
  let runtime: ReturnType<typeof startPlayRuntime> | null = null;
  try {
    chrome = mountPlayChrome(shell);
    runtime = startPlayRuntime(shell, applicationStyle);
  } catch (error) {
    chrome?.dispose();
    runtime?.dispose();
    applicationStyle.remove();
    shell.container.remove();
    if (!hadBodyClass) document.body.classList.remove("uh-play-shell");
    throw error;
  }

  return (): void => {
    if (disposed) return;
    disposed = true;
    runtime?.dispose();
    chrome?.dispose();
    runtime = null;
    chrome = null;
    applicationStyle.remove();
    shell.container.remove();
    if (!hadBodyClass) document.body.classList.remove("uh-play-shell");
  };
}
