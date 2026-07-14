// The diagnostics overlay (§12.4): a broken edit renders its
// `uhura-diagnostics/0` envelope OVER the still-running last-good app —
// state-preserving reload is an open RFC topic the spike must not fake,
// so the overlay never touches the app underneath.

export function createOverlay(host: HTMLElement) {
  host.className = "uh-dev-overlay";
  host.hidden = true;
  // Fatal overlays cover a page with NOTHING running underneath — only a
  // reload clears them; diagnostics overlays sit over the last-good app.
  let fatal = false;
  // A diagnostics backdrop dismisses because the app underneath still runs;
  // a fatal boot stays visible until a host restart/provider/actor change.
  host.addEventListener("click", (event) => {
    if (event.target === host && !fatal) host.hidden = true;
  });

  /** @param {string} title @param {HTMLElement[]} body */
  function show(title: string, body: HTMLElement[]): void {
    host.replaceChildren();
    const panel = document.createElement("div");
    panel.className = "uh-dev-panel";
    const heading = document.createElement("h2");
    heading.textContent = title;
    const hint = document.createElement("p");
    hint.className = "uh-dev-hint";
    hint.textContent = "click the backdrop to dismiss";
    panel.append(heading, ...body, hint);
    host.append(panel);
    host.hidden = false;
  }

  return {
    /**
     * @param {Record<string, unknown>} envelope `uhura-diagnostics/0`
     */
    showDiagnostics(envelope: Record<string, unknown>) {
      fatal = false;
      const diags = Array.isArray(envelope["diagnostics"])
        ? (envelope["diagnostics"] as Record<string, unknown>[])
        : [];
      const items = diags.map((d) => {
        const item = document.createElement("div");
        item.className = `uh-diag uh-diag-${String(d["severity"] ?? "error")}`;
        const head = document.createElement("div");
        head.className = "uh-diag-head";
        const where = describeSpan(d);
        head.textContent =
          `${String(d["code"] ?? "UH????")} ${String(d["rule"] ?? "")}` +
          (where ? ` — ${where}` : "");
        const message = document.createElement("div");
        message.className = "uh-diag-message";
        message.textContent = String(d["message"] ?? "");
        item.append(head, message);
        return item;
      });
      if (items.length === 0) {
        const item = document.createElement("div");
        item.className = "uh-diag";
        item.textContent = "the check failed with no diagnostics (?)";
        items.push(item);
      }
      show(`check failed — running the last good build (${items.length})`, items);
    },

    /** A boot-fatal condition: nothing is running underneath. */
    showFatal(message: string) {
      fatal = true;
      const body = document.createElement("pre");
      body.className = "uh-diag-message";
      body.textContent = message;
      show("uhura play failed to boot", [body]);
    },

    hide() {
      host.hidden = true;
      host.replaceChildren();
    },

    /** SSE "all clear": never dismisses a fatal overlay — the dead page
     * under it would just be blank. */
    hideDiagnostics() {
      if (!fatal) {
        host.hidden = true;
        host.replaceChildren();
      }
    },
  };
}

/** @param {Record<string, unknown>} d */
function describeSpan(d: Record<string, unknown>): string {
  const span = d["span"];
  if (typeof span !== "object" || span === null) return "";
  const s = span as Record<string, unknown>;
  const file = typeof s["file"] === "string" ? s["file"] : "";
  const line = typeof s["line"] === "number" ? `:${s["line"]}` : "";
  return file ? `${file}${line}` : "";
}
