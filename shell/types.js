// The frozen ABI shapes (design §8.1, §9.3, §12.3), mirrored from
// crates/uhura-wasm/tests/abi_contract.rs — plan micro-decision #14.
// These typedefs ARE the contract the shell compiles against; a change
// here is a protocol version bump, not an edit.

/**
 * @typedef {Object} Descriptor
 * @property {"input" | "observe"} kind
 * @property {string} event    catalog event (`press`, `near-end`, `change`)
 * @property {string} emit     the machine event this emits
 * @property {string} scope    `"page:1"` | `"surface:2"`
 * @property {unknown} payload prebuilt by core — inert, echoed verbatim
 * @property {Record<string, "text" | "bool" | "int">} [carries]
 */

/**
 * @typedef {Object} VNode
 * @property {string} key       sibling-unique; key-path is global identity
 * @property {string} element   catalog element name
 * @property {string} [class]   authored CSS classes, verbatim
 * @property {Record<string, VValue>} props
 * @property {VNode[]} [children]
 * @property {Descriptor[]} [on]
 */

/**
 * §8.1 value: bool | int | bare token | inert human text | asset ref.
 * @typedef {boolean | number | string
 *   | { t: "plain", v: string }
 *   | { t: "image", asset: string }} VValue
 */

/**
 * @typedef {Object} SurfaceView
 * @property {string} key         `"comments-sheet:2"` (definition:serial)
 * @property {string} definition
 * @property {string} modality
 * @property {string} [restore-focus]
 * @property {Descriptor} dismiss first-class: Escape/scrim wire here
 * @property {VNode} root
 */

/**
 * @typedef {Object} Snapshot
 * @property {"uhura-view/0"} protocol
 * @property {number} revision
 * @property {{ route: string, root: VNode }} page
 * @property {SurfaceView[]} surfaces bottom → top
 */

/**
 * One provider wire message (`uhura-provider/0` — §9.3).
 * @typedef {Object} ProviderMsg
 * @property {"command" | "projection" | "projection-failed" | "outcome"} kind
 * @property {string} [port]
 * @property {string} [command]
 * @property {string} [correlation]
 * @property {unknown} [payload]
 * @property {string} [projection]
 * @property {unknown} [key]
 * @property {number} [revision]
 * @property {unknown} [value]
 * @property {string} [reason]
 * @property {unknown} [outcome]
 * @property {unknown[]} [updates]
 */

/**
 * A host intent (§7.4) — the spike shell executes `focus-restore` and
 * treats history intents as visible no-ops.
 * @typedef {{ intent: "history-push", route: string, params: Record<string, unknown> }
 *   | { intent: "history-back" }
 *   | { intent: "focus-restore", "key-path": string }} Intent
 */

/**
 * The step-result envelope `Session.dispatch` returns (§12.3): every key
 * always present.
 * @typedef {Object} StepResult
 * @property {ProviderMsg[]} c  command envelopes to forward to the driver
 * @property {Intent[]} i
 * @property {{ code: string, rule: string, message: string }[]} g
 * @property {Record<string, unknown>} t canonical trace record (§7.5)
 * @property {Snapshot} v
 */

/**
 * One `/events` SSE payload from `uhura dev` (§12.4).
 * @typedef {Object} DevEvent
 * @property {number} generation monotonic recheck counter
 * @property {boolean} ok        false ⇒ diagnostics, app keeps last-good
 * @property {Record<string, unknown>} [diagnostics] `uhura-diagnostics/0`
 */

export {};
