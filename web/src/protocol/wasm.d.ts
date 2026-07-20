// Typing for the wasm-bindgen bundle `uhura dev` serves at
// /wasm/uhura_wasm.js (built by scripts/build-wasm.sh, --target web).
// The host tsconfig maps the runtime specifier here so a clean checkout can
// typecheck before Wasm is built. wasm-bindgen also emits declarations with
// the generated package; this stable facade mirrors the versioned JSON ABI.

/** Loads and instantiates the wasm binary (fetches uhura_wasm_bg.wasm). */
export default function init(module_or_path?: unknown): Promise<unknown>;

/** Canonical machine/browser protocol capability set. */
export function protocols(): string;

/** One admitted Uhura machine instance and its optional UI presentation. */
export class Session {
  constructor(
    ir_json: string,
    machine: string,
    configuration_json: string,
    instance: string,
    presentation: string | undefined,
    expected_identity_json: string,
  );
  protocols(): string;
  genesis(): string;
  semantic_genesis(): string;
  view(): string;
  presentation(): string;
  inspect(): string;
  next_sequence(): string;
  port_requirements(): string;
  submit(input_json: string): string;
  submit_value(value_json: string): string;
  dispatch_ui(
    binding_id: string,
    projection_sequence: string,
    event_json: string,
  ): string;
  decode_route(port: string, url: string): string;
  encode_route(port: string, location_json: string): string;
  checkpoint(): string;
  semantic_checkpoint(): string;
  restore(checkpoint_json: string): void;
  semantic_receipt(): string;
  free(): void;
}
