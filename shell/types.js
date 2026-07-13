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
 * The host-facing driver interface. The fixture driver implements the
 * synchronous three-method core; remote play providers additionally assemble
 * their boot deliveries from authority truth.
 * @typedef {Object} Driver
 * @property {(commandJson: string) => void} deliver
 * @property {() => string[]} tick
 * @property {() => boolean} idle
 */

/**
 * One app-owned provider module selected only by `uhura dev` play mode.
 * Canvas generation, checks, examples, and traces never consume it.
 * @typedef {{ kind: "fixture" }
 *   | { kind: "module", module: string, config: Record<string, string> }} PlayProvider
 */

/** @typedef {{ provider: PlayProvider }} PlayConfig */

/** @typedef {"starting" | "ready" | "error"} SystemStatus */
/** @typedef {"remote" | "fixture"} ProviderMode */

/**
 * One provider-owned auth actor offered to system chrome. The shell treats
 * the id as opaque and uses the provider-authored label for display.
 * @typedef {Object} SystemActor
 * @property {string} id
 * @property {string} username
 * @property {string} label
 */

/**
 * Read-only snapshot published by the play host as
 * `window.__uhura.system` and `uhura:system-state` event detail.
 * @typedef {Object} SystemState
 * @property {SystemStatus} status
 * @property {ProviderMode | null} provider
 * @property {ProviderMode[]} providers
 * @property {string | null} actor
 * @property {SystemActor[]} actors
 * @property {boolean} canSwitchActor
 * @property {string} [error]
 */

/**
 * Optional metadata a remote provider can expose after (or during a failed)
 * boot. It stays outside the Uhura provider envelope: auth selection belongs
 * to the play host, not to app-authored state.
 * @typedef {Object} RemoteSystemInfo
 * @property {string | null} actor
 * @property {SystemActor[]} actors
 */

/**
 * Browser-only capabilities passed to an app-owned provider factory. Values
 * returned here stay in the shell/provider boundary and never enter a core
 * command or projection envelope.
 * @typedef {Object} ProviderHost
 * @property {(options?: { accept?: string }) => Promise<File | null>} pickFile
 */

/**
 * @typedef {Object} RemoteDriver
 * @property {(commandJson: string) => void} deliver
 * @property {() => string[]} tick
 * @property {() => boolean} idle
 * @property {() => Promise<string>} assembleBoot
 * @property {(assetRef: string) => Promise<string>} [resolveAsset]
 * @property {() => RemoteSystemInfo} [systemInfo]
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
