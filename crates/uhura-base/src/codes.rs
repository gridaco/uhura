//! The UHnxxx diagnostic code registry (plan micro-decision #5).
//!
//! Blocks by pipeline pass:
//! - UH0xxx — lex/parse (incl. bounds)
//! - UH1xxx — routes / resolution / imports
//! - UH2xxx — catalog / ports / lock
//! - UH3xxx — types / expressions
//! - UH4xxx — machine (statements, handlers)
//! - UH5xxx — markup rules
//! - UH6xxx — style
//! - UH7xxx — examples
//! - UH8xxx — runtime (minted by core, appear in `G`/traces)
//! - UH9xxx — internal invariants
//!
//! Every constant pairs the stable code with its human `rule` slug. Codes
//! are appended, never renumbered; each new pass adds its block here so
//! collisions are impossible.

/// `(code, rule)` pair type for registry entries.
pub type Code = (&'static str, &'static str);

// ── UH0xxx: lex/parse ──────────────────────────────────────────────────────
pub const UNEXPECTED_TOKEN: Code = ("UH0001", "syntax/unexpected-token");
pub const UNTERMINATED_STRING: Code = ("UH0002", "syntax/unterminated-string");
pub const UNKEYED_EACH: Code = ("UH0003", "syntax/unkeyed-each");
pub const UNCLOSED_TAG: Code = ("UH0004", "syntax/unclosed-tag");
pub const UNCLOSED_BLOCK: Code = ("UH0005", "syntax/unclosed-block");
pub const RAW_BRACE_IN_TEXT: Code = ("UH0006", "syntax/raw-brace-in-text");
pub const INVALID_IDENT: Code = ("UH0007", "syntax/invalid-identifier");
pub const FILE_TOO_LARGE: Code = ("UH0010", "bounds/file-too-large");
pub const NESTING_TOO_DEEP: Code = ("UH0011", "bounds/nesting-too-deep");
pub const TOO_MANY_NODES: Code = ("UH0012", "bounds/too-many-view-nodes");
pub const TOO_MANY_HANDLERS: Code = ("UH0013", "bounds/too-many-handlers");
pub const MISPLACED_SECTION: Code = ("UH0014", "syntax/misplaced-section");
pub const INVALID_STYLE_BLOCK: Code = ("UH0015", "syntax/invalid-style-block");
pub const MALFORMED_MARKUP_COMMENT: Code = ("UH0016", "syntax/malformed-markup-comment");
pub const DANGLING_METADATA: Code = ("UH0017", "syntax/dangling-metadata");
pub const MISPLACED_INNER_DOC: Code = ("UH0018", "syntax/misplaced-inner-doc");
pub const INCOMPATIBLE_METADATA_TARGET: Code = ("UH0019", "syntax/incompatible-metadata-target");

// ── UH1xxx: routes / resolution / imports ──────────────────────────────────
pub const BAD_PAGE_PATH: Code = ("UH1001", "routes/bad-page-path");
pub const ROUTE_COLLISION: Code = ("UH1002", "routes/route-collision");
pub const HEADER_BASENAME_MISMATCH: Code = ("UH1003", "resolve/header-basename-mismatch");
pub const WRONG_DIRECTORY: Code = ("UH1004", "resolve/wrong-directory");
pub const UNKNOWN_IMPORT: Code = ("UH1005", "resolve/unknown-import");
pub const IMPORT_CYCLE: Code = ("UH1006", "resolve/import-cycle");
pub const SHADOWED_NAME: Code = ("UH1007", "resolve/shadowed-name");
pub const DUPLICATE_IMPORT: Code = ("UH1008", "resolve/duplicate-import");
pub const PARAM_ROUTE_MISMATCH: Code = ("UH1009", "resolve/param-route-mismatch");
pub const MISPLACED_DECLARATION: Code = ("UH1010", "resolve/misplaced-declaration");
pub const ENTRY_ROUTE_MISSING: Code = ("UH1011", "resolve/entry-route-missing");
pub const ORPHAN_EXAMPLES_FILE: Code = ("UH1012", "resolve/orphan-examples-file");

// ── UH2xxx: catalog / ports / manifest / lock ──────────────────────────────
pub const INVALID_MANIFEST: Code = ("UH2001", "contract/invalid-manifest");
pub const INVALID_CATALOG: Code = ("UH2002", "contract/invalid-catalog");
pub const INVALID_PORT_CONTRACT: Code = ("UH2003", "contract/invalid-port-contract");
pub const UNKNOWN_PORT: Code = ("UH2004", "contract/unknown-port");
pub const UNKNOWN_PORT_ITEM: Code = ("UH2005", "contract/unknown-port-item");
pub const PORT_NAME_COLLISION: Code = ("UH2006", "contract/port-name-collision");
pub const LOCK_DRIFT: Code = ("UH2007", "contract/lock-drift");
pub const PORT_NAME_MISMATCH: Code = ("UH2008", "contract/port-name-mismatch");
pub const INVALID_ICON_FONT: Code = ("UH2010", "contract/invalid-icon-font");

// ── UH3xxx: types / expressions ────────────────────────────────────────────
pub const TYPE_MISMATCH: Code = ("UH3001", "types/type-mismatch");
pub const UNKNOWN_FIELD: Code = ("UH3002", "types/unknown-field");
pub const UNRESOLVED_NAME: Code = ("UH3003", "types/unresolved-name");
pub const WRONG_ARGS: Code = ("UH3004", "types/wrong-arguments");
pub const BAD_INDEX: Code = ("UH3005", "types/bad-index");
pub const BAD_OPERAND: Code = ("UH3006", "types/bad-operand");
pub const BAD_STATE_TYPE: Code = ("UH3007", "types/bad-state-type");
pub const BAD_BUILTIN_CALL: Code = ("UH3008", "types/bad-builtin-call");
pub const UNGUARDED_PROJECTION_READ: Code = ("UH3009", "types/unguarded-projection-read");
pub const BAD_MAP_KEY: Code = ("UH3010", "types/bad-map-key");

// ── UH4xxx: machine (statements, handlers) ─────────────────────────────────
pub const UNKNOWN_ROUTE: Code = ("UH4001", "machine/unknown-route");
pub const DISMISS_OUTSIDE_SURFACE: Code = ("UH4002", "machine/dismiss-outside-surface");
pub const UNKNOWN_SURFACE: Code = ("UH4003", "machine/unknown-surface");
pub const UNKNOWN_COMMAND: Code = ("UH4004", "machine/unknown-command");
pub const UNREACHABLE_HANDLER: Code = ("UH4005", "machine/unreachable-handler");
pub const HANDLER_SIGNATURE_MISMATCH: Code = ("UH4006", "machine/handler-signature-mismatch");
pub const BAD_OUTCOME_SIGNATURE: Code = ("UH4007", "machine/bad-outcome-signature");
pub const MULTIPLE_NAVIGATES: Code = ("UH4008", "machine/multiple-navigates");
pub const STORE_NOT_ALLOWED: Code = ("UH4009", "machine/store-not-allowed");
pub const DUPLICATE_STATE_FIELD: Code = ("UH4010", "machine/duplicate-state-field");

// ── UH5xxx: markup rules ───────────────────────────────────────────────────
pub const UNKNOWN_ELEMENT: Code = ("UH5001", "markup/unknown-element");
pub const EVENT_NOT_DECLARED: Code = ("UH5002", "markup/event-not-declared");
pub const UNDECLARED_EMIT: Code = ("UH5003", "markup/undeclared-emit");
pub const MISSING_REQUIRED_PROP: Code = ("UH5004", "markup/missing-required-prop");
pub const UNKNOWN_PROP: Code = ("UH5005", "markup/unknown-prop");
pub const BAD_CHILDREN: Code = ("UH5006", "markup/bad-children");
pub const NESTED_INTERACTIVE: Code = ("UH5007", "markup/nested-interactive");
pub const CONTROLLED_PROMOTION: Code = ("UH5008", "markup/controlled-promotion");
pub const A11Y_ALT: Code = ("UH5009", "markup/a11y-alt");
pub const LIST_NEEDS_KEYED_EACH: Code = ("UH5010", "markup/list-needs-keyed-each");
pub const ONE_ROOT: Code = ("UH5011", "markup/one-root");
pub const INTERP_OUTSIDE_TEXT: Code = ("UH5012", "markup/interpolation-outside-text");
pub const UNHANDLED_EVENT: Code = ("UH5013", "markup/unhandled-event");
pub const DUPLICATE_ATTR: Code = ("UH5014", "markup/duplicate-attribute");
pub const BAD_AVAILABILITY_ARMS: Code = ("UH5015", "markup/bad-availability-arms");
pub const BAD_UNION_ARMS: Code = ("UH5016", "markup/bad-union-arms");
pub const UNKNOWN_ICON: Code = ("UH5017", "markup/unknown-icon");
pub const CARRIED_FIELD_NAMED: Code = ("UH5018", "markup/carried-field-named");
pub const ELEMENT_EVENT_NEEDS_EMIT: Code = ("UH5019", "markup/element-event-needs-emit");
pub const SUPPLEMENTARY_UNREACHABLE: Code = ("UH5020", "markup/supplementary-unreachable");
pub const UNKNOWN_ICON_FAMILY: Code = ("UH5021", "markup/unknown-icon-family");

// ── UH6xxx: style ──────────────────────────────────────────────────────────
pub const CLASS_ROOTING: Code = ("UH6001", "style/class-rooting");
pub const UNDEFINED_CLASS: Code = ("UH6002", "style/undefined-class");

// ── UH7xxx: examples ───────────────────────────────────────────────────────
pub const ILLEGAL_CLAUSE: Code = ("UH7001", "examples/illegal-clause");
pub const MULTIPLE_DEFAULTS: Code = ("UH7002", "examples/multiple-defaults");
pub const NO_DEFAULT: Code = ("UH7003", "examples/no-default");
pub const BAD_FROM: Code = ("UH7004", "examples/bad-from");
pub const UNKNOWN_PIN_TARGET: Code = ("UH7005", "examples/unknown-pin-target");
pub const BAD_PIN: Code = ("UH7006", "examples/bad-pin");
pub const UNKNOWN_FIXTURE: Code = ("UH7007", "examples/unknown-fixture");
pub const BAD_EXAMPLE_EVENT: Code = ("UH7008", "examples/bad-example-event");
pub const PIN_DECODE: Code = ("UH7009", "examples/pin-decode");
pub const BOOT_UNBOUND: Code = ("UH7010", "examples/boot-unbound");
pub const MULTIPLE_FIXTURES: Code = ("UH7011", "examples/multiple-fixtures");
pub const REPLAY_STEP: Code = ("UH7012", "examples/replay-step");
pub const REPLAY_BLOCKED: Code = ("UH7013", "examples/blocked-by-ancestor");

// ── UH8xxx: runtime (minted by core, appear in `G`/traces) ─────────────────
pub const DUPLICATE_IN_FLIGHT: Code = ("UH8001", "runtime/duplicate-in-flight-send");
pub const INVALID_FIXTURE: Code = ("UH2009", "contract/invalid-fixture");

// ── UH9xxx: internal invariants ────────────────────────────────────────────────
pub const TEMPLATE_ORIGIN_COVERAGE: Code = ("UH9001", "internal/template-origin-coverage");
