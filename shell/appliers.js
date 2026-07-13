// Semantic props → DOM/ARIA, one applier per catalog element (§8.2).
// Mirrors the static renderer in crates/uhura-project (same `uh-<element>`
// bases, same ARIA mapping) so both renderers stay two consumers of one
// protocol. Human text lands via TEXT NODES / attribute strings only —
// never markup.

/** @param {import("./types.js").VValue | undefined} v */
export function textOf(v) {
  if (typeof v === "string") return v;
  if (typeof v === "object" && v !== null && "t" in v && v.t === "plain") return v.v;
  return undefined;
}

/** @param {import("./types.js").VValue | undefined} v */
function boolOf(v) {
  return v === true;
}

/** @param {import("./types.js").VValue | undefined} v */
function assetOf(v) {
  if (typeof v === "object" && v !== null && "t" in v && v.t === "image") return v.asset;
  return undefined;
}

/** Asset ids resolve against the dev server (§12.4). */
function assetUrl(/** @type {string} */ id) {
  return `/assets/${encodeURIComponent(id)}.jpg`;
}

/**
 * @param {HTMLElement} el
 * @param {string} name
 * @param {string | undefined} value set when defined, remove when not
 */
function setAttr(el, name, value) {
  if (value === undefined) el.removeAttribute(name);
  else if (el.getAttribute(name) !== value) el.setAttribute(name, value);
}

/**
 * The DOM tag an element renders as (everything else is a div).
 * @param {string} element
 */
export function tagFor(element) {
  switch (element) {
    case "text":
      return "p";
    case "icon":
      return "span";
    case "button":
      return "button";
    default:
      return "div";
  }
}

/**
 * Applies one node's semantic props. Structure-owning elements
 * (text-field's input, pager's track/dots, icon's svg) manage their
 * mechanic children here; the reconciler owns keyed children.
 * @param {HTMLElement} el
 * @param {import("./types.js").VNode} node
 * @param {{ glyphs: Record<string, string>,
 *           textFields: ReturnType<import("./textfield.js").createTextFields>,
 *           holderOf: (el: HTMLElement) => { path: string,
 *             on: Record<string, import("./types.js").Descriptor> } }} ctx
 */
export function applyProps(el, node, ctx) {
  const props = node.props;
  switch (node.element) {
    case "view": {
      const role = textOf(props["role"]);
      setAttr(
        el,
        "role",
        role === "list" || role === "navigation" || role === "tablist" ? role : undefined,
      );
      break;
    }

    case "scroll":
      setAttr(el, "data-direction", textOf(props["direction"]) ?? "vertical");
      break;

    case "pager": {
      const label = textOf(props["label"]);
      setAttr(el, "role", "group");
      setAttr(el, "aria-label", label);
      // Keyed children live inside .uh-track (the reconciler asks for it
      // via childHost); dots are pure renderer mechanics.
      const track = ensureTrack(el);
      if (textOf(props["indicator"]) === "dots") {
        const dots = ensureDots(el);
        const count = node.children?.length ?? 0;
        while (dots.children.length > count) dots.lastElementChild?.remove();
        while (dots.children.length < count) {
          const dot = document.createElement("span");
          dot.className = "uh-dot";
          dots.append(dot);
        }
        updateDots(el, track);
      } else {
        el.querySelector(":scope > .uh-dots")?.remove();
      }
      break;
    }

    case "text":
      // Interpolations were evaluated by core into one plain `content`
      // prop; a text node is the only carrier (§8.2).
      if (el.textContent !== (textOf(props["content"]) ?? "")) {
        el.textContent = textOf(props["content"]) ?? "";
      }
      break;

    case "image": {
      const asset = assetOf(props["src"]);
      el.style.backgroundImage = asset ? `url("${assetUrl(asset)}")` : "";
      if (boolOf(props["decorative"])) {
        setAttr(el, "aria-hidden", "true");
        setAttr(el, "role", undefined);
        setAttr(el, "aria-label", undefined);
      } else {
        setAttr(el, "role", "img");
        setAttr(el, "aria-label", textOf(props["alt"]));
        setAttr(el, "aria-hidden", undefined);
      }
      break;
    }

    case "icon": {
      const name = textOf(props["name"]) ?? "";
      setAttr(el, "aria-hidden", "true");
      if (el.getAttribute("data-icon") !== name) {
        setAttr(el, "data-icon", name);
        const glyph =
          ctx.glyphs[name] ??
          '<circle cx="12" cy="12" r="8" fill="none" stroke="currentColor" stroke-width="1.8"/>';
        // Trusted markup: the glyph table is the checked catalog's icon
        // set served by `uhura dev` — never author or wire data.
        el.innerHTML = `<svg viewBox="0 0 24 24" width="24" height="24">${glyph}</svg>`;
      }
      break;
    }

    case "button": {
      const button = /** @type {HTMLButtonElement} */ (el);
      button.disabled = boolOf(props["disabled"]);
      setAttr(el, "aria-busy", boolOf(props["busy"]) ? "true" : undefined);
      setAttr(
        el,
        "aria-pressed",
        typeof props["pressed"] === "boolean" ? String(props["pressed"]) : undefined,
      );
      setAttr(el, "aria-current", boolOf(props["current"]) ? "true" : undefined);
      setAttr(el, "aria-label", textOf(props["label"]));
      break;
    }

    case "text-field": {
      const existing = el.querySelector(":scope > input");
      /** @type {HTMLInputElement} */
      let input;
      if (existing instanceof HTMLInputElement) {
        input = existing;
      } else {
        input = document.createElement("input");
        input.type = "text";
        input.setAttribute("data-uh-mechanic", "input");
        el.append(input);
        ctx.textFields.wire(input, ctx.holderOf(el));
      }
      setAttr(input, "placeholder", textOf(props["placeholder"]));
      setAttr(input, "aria-label", textOf(props["label"]));
      input.disabled = boolOf(props["disabled"]);
      // Core's draft applies through the in-flight gate — never directly.
      ctx.textFields.applyValue(input, textOf(props["value"]) ?? "");
      break;
    }

    case "region":
      setAttr(el, "role", "button");
      setAttr(el, "tabindex", "0");
      setAttr(el, "aria-label", textOf(props["label"]));
      break;

    default:
      // Honest labeled placeholder for anything unsupported (§8.2).
      setAttr(el, "data-unsupported", node.element);
  }
}

/** @param {HTMLElement} el */
function ensureTrack(el) {
  const found = el.querySelector(":scope > .uh-track");
  if (found instanceof HTMLElement) return found;
  const track = document.createElement("div");
  track.className = "uh-track";
  track.setAttribute("data-uh-mechanic", "track");
  el.append(track);
  track.addEventListener("scroll", () => updateDots(el, track), { passive: true });
  return track;
}

/** @param {HTMLElement} el */
function ensureDots(el) {
  const found = el.querySelector(":scope > .uh-dots");
  if (found instanceof HTMLElement) return found;
  const dots = document.createElement("div");
  dots.className = "uh-dots";
  dots.setAttribute("data-uh-mechanic", "dots");
  dots.setAttribute("aria-hidden", "true");
  el.append(dots);
  return dots;
}

/** @param {HTMLElement} pager @param {HTMLElement} track */
function updateDots(pager, track) {
  const dots = pager.querySelector(":scope > .uh-dots");
  if (!dots) return;
  const width = track.clientWidth || 1;
  const active = Math.min(
    dots.children.length - 1,
    Math.max(0, Math.round(track.scrollLeft / width)),
  );
  [...dots.children].forEach((dot, i) => dot.classList.toggle("on", i === active));
}
