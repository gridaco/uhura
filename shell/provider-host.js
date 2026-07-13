// Browser capabilities offered to app-owned play providers. The shell keeps
// browser-only values (such as File) outside Uhura's provider envelopes.

/**
 * @returns {import("./types.js").ProviderHost}
 */
export function createProviderHost() {
  /** @type {(() => void) | null} */
  let cancelActivePicker = null;

  return {
    pickFile({ accept } = {}) {
      // A provider should normally serialize commands, but do not strand an
      // earlier picker if it starts another one before the first settles.
      cancelActivePicker?.();

      const input = document.createElement("input");
      input.type = "file";
      if (accept) input.accept = accept;
      input.tabIndex = -1;
      input.setAttribute("aria-hidden", "true");
      Object.assign(input.style, {
        position: "fixed",
        inlineSize: "1px",
        blockSize: "1px",
        insetInlineStart: "-10000px",
        opacity: "0",
        pointerEvents: "none",
      });
      document.body.append(input);

      return new Promise((resolve, reject) => {
        let settled = false;
        // Modern browsers report an abandoned chooser explicitly. In those
        // browsers, `change`/`cancel` are the authority: window focus can be
        // restored before the selected file has reached the input, so using
        // focus as an eager cancellation signal can discard a valid choice.
        const supportsCancelEvent = "oncancel" in input;
        let sawWindowBlur = false;
        let sawDocumentHidden = false;
        /** @type {number | null} */
        let focusTimer = null;

        function cleanup() {
          input.removeEventListener("change", onChange);
          input.removeEventListener("cancel", onCancel);
          window.removeEventListener("blur", onWindowBlur);
          window.removeEventListener("focus", onWindowFocus);
          window.removeEventListener("pagehide", onPageHide);
          document.removeEventListener("visibilitychange", onVisibilityChange);
          if (focusTimer !== null) window.clearTimeout(focusTimer);
          input.remove();
          if (cancelActivePicker === cancel) cancelActivePicker = null;
        }

        /** @param {File | null} file */
        function settle(file) {
          if (settled) return;
          settled = true;
          cleanup();
          resolve(file);
        }

        function selectedFile() {
          return input.files?.item(0) ?? null;
        }

        function onChange() {
          settle(selectedFile());
        }

        function onCancel() {
          settle(null);
        }

        function cancel() {
          settle(null);
        }

        function onWindowBlur() {
          sawWindowBlur = true;
        }

        function settleAfterDialogCloses() {
          if (supportsCancelEvent) return;
          if (focusTimer !== null) window.clearTimeout(focusTimer);
          // Legacy engines have no `cancel` event. Give their `change` event
          // ample time to arrive before falling back to the FileList; this
          // path is only for detecting cancellation in those engines.
          focusTimer = window.setTimeout(() => settle(selectedFile()), 2_000);
        }

        function onWindowFocus() {
          if (sawWindowBlur) settleAfterDialogCloses();
        }

        function onVisibilityChange() {
          if (document.visibilityState === "hidden") {
            sawDocumentHidden = true;
          } else if (sawDocumentHidden) {
            settleAfterDialogCloses();
          }
        }

        function onPageHide() {
          settle(null);
        }

        input.addEventListener("change", onChange);
        input.addEventListener("cancel", onCancel);
        window.addEventListener("blur", onWindowBlur);
        window.addEventListener("focus", onWindowFocus);
        window.addEventListener("pagehide", onPageHide);
        document.addEventListener("visibilitychange", onVisibilityChange);
        cancelActivePicker = cancel;

        try {
          // This must stay in the synchronous pickFile call stack so the
          // browser recognizes the provider command as user-activated.
          input.click();
        } catch (error) {
          settled = true;
          cleanup();
          reject(error);
        }
      });
    },
  };
}
