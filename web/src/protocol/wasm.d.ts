// Typing for the wasm-bindgen bundle `uhura dev` serves at
// /wasm/uhura_wasm.js (built by scripts/build-wasm.sh, --target web).
// The host tsconfig maps the runtime specifier here so a clean checkout can
// typecheck before Wasm is built. wasm-bindgen also emits declarations with
// the generated package; this stable facade mirrors the versioned JSON ABI.

/** Loads and instantiates the wasm binary (fetches uhura_wasm_bg.wasm). */
export default function init(module_or_path?: unknown): Promise<unknown>;

/** Protocol capability set, including `uhura-inspect/0`. */
export function protocols(): string;

export class Session {
  /** `ir_json`: the canonical `uhura-ir/0` artifact. Throws a string. */
  constructor(ir_json: string);
  /** Applies `{"updates": [...]}` boot deliveries before `Init` (§9.2). */
  boot(boot_json: string): void;
  /** One step; returns the step-result envelope as canonical JSON. */
  dispatch(event_json: string): string;
  /** Current `uhura-view/0` snapshot, canonical JSON. */
  view(): string;
  /** Complete read-only `uhura-inspect/0` snapshot, canonical JSON. */
  inspect(): string;
  /** `U.rev` — `+1` every step. */
  revision(): number;
  /** `"uhura-ir/0"` */
  ir_version(): string;
  free(): void;
}

export class FixtureDriver {
  constructor(fixture_json: string, script_json: string);
  /** Accepts one command envelope (wire form). Throws a string. */
  deliver(cmd_json: string): void;
  /** Advances one tick; returns the provider messages due. */
  tick(): string[];
  /** True when nothing is scheduled or in flight. */
  idle(): boolean;
  free(): void;
}
