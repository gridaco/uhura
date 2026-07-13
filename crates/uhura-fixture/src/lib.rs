//! uhura-fixture: the scripted mock-Spock driver — tick-based, deterministic,
//! entirely outside uhura-core (design §9.5). Compiled natively for traces
//! and to wasm for play mode.
#![deny(clippy::float_arithmetic)]

mod driver;
mod script;

pub use driver::FixtureDriver;

/// TOML text → JSON for script and fixture files, refusing everything the
/// value model refuses (floats, datetimes — §7.5 determinism by type shape).
/// Kept semantically in lock-step with the checker's converter.
pub fn toml_to_json(text: &str) -> Result<serde_json::Value, String> {
    let table: toml::Table = text.parse().map_err(|e| format!("invalid TOML: {e}"))?;
    toml_value_to_json(&toml::Value::Table(table))
}

fn toml_value_to_json(value: &toml::Value) -> Result<serde_json::Value, String> {
    use serde_json::Value as J;
    match value {
        toml::Value::String(s) => Ok(J::String(s.clone())),
        toml::Value::Integer(i) => Ok(J::Number((*i).into())),
        toml::Value::Boolean(b) => Ok(J::Bool(*b)),
        toml::Value::Float(_) => Err("floats do not exist in fixture data (§7.5)".into()),
        toml::Value::Datetime(_) => {
            Err("no clocks: time labels are provider-formatted text (§9.1)".into())
        }
        toml::Value::Array(items) => items
            .iter()
            .map(toml_value_to_json)
            .collect::<Result<Vec<_>, _>>()
            .map(J::Array),
        toml::Value::Table(table) => table
            .iter()
            .map(|(k, v)| Ok((k.clone(), toml_value_to_json(v)?)))
            .collect::<Result<serde_json::Map<_, _>, String>>()
            .map(J::Object),
    }
}
