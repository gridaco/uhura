//! The semantic view protocol `uhura-view/0` (design §8.1) — the full-
//! snapshot value both renderers consume. Hand-written JSON shapes (plan
//! micro-decision #14): the wire form IS the contract, so no serde-derived
//! drift can occur.

use std::collections::BTreeMap;

use uhura_base::{Ident, hash_json, to_canonical_json};

pub const VIEW_PROTOCOL: &str = "uhura-view/0";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub revision: u64,
    pub page: PageView,
    /// Bottom → top.
    pub surfaces: Vec<SurfaceView>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageView {
    pub route: String,
    pub root: Node,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SurfaceView {
    /// `"comments-sheet:2"` (definition:serial).
    pub key: String,
    pub definition: String,
    pub modality: String,
    /// Key-path of the node focus returns to on dismiss.
    pub restore_focus: Option<String>,
    /// First-class: Escape/scrim wire here (§8.1).
    pub dismiss: Descriptor,
    pub root: Node,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    /// Sibling-unique; the key-path is global identity (§8.1).
    pub key: String,
    /// Catalog element name.
    pub element: Ident,
    /// Authored CSS classes, passed through verbatim.
    pub class: Option<String>,
    /// SEMANTIC props only.
    pub props: BTreeMap<Ident, VValue>,
    pub children: Vec<Node>,
    pub on: Vec<Descriptor>,
}

/// §8.1 Value: bool | i64 | text | inert human text | asset reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VValue {
    Bool(bool),
    Int(i64),
    /// Bare semantic token (`role`, `direction`, icon names).
    Text(String),
    /// `{ t: "plain", v: text }` — inert human text, never markup.
    Plain(String),
    /// `{ t: "image", asset: text }` — opaque asset id; core never fetches.
    Image(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Descriptor {
    pub kind: DescriptorKind,
    /// The catalog event (`press`, `near-end`, `change`).
    pub event: Ident,
    /// The machine event this emits.
    pub emit: Ident,
    /// `"page:1"` | `"surface:2"` — minted serials.
    pub scope: String,
    /// PREBUILT by core — inert JSON, echoed verbatim (§8.1).
    pub payload: serde_json::Value,
    /// Renderer-carried fields (`{ value: "text" }`).
    pub carries: BTreeMap<Ident, String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DescriptorKind {
    Input,
    Observe,
}

impl VValue {
    pub fn to_json(&self) -> serde_json::Value {
        use serde_json::json;
        match self {
            VValue::Bool(b) => json!(b),
            VValue::Int(i) => json!(i),
            VValue::Text(s) => json!(s),
            VValue::Plain(s) => json!({ "t": "plain", "v": s }),
            VValue::Image(asset) => json!({ "t": "image", "asset": asset }),
        }
    }
}

impl Descriptor {
    /// Parses the wire form back (the play shell echoes descriptors
    /// verbatim in `ui` events — §8.1/§12.3). Tolerant of extra fields,
    /// strict on the ones that matter.
    pub fn from_json(json: &serde_json::Value) -> Result<Self, String> {
        let str_field = |field: &str| -> Result<&str, String> {
            json.get(field)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| format!("a descriptor needs a text `{field}`"))
        };
        let kind = match str_field("kind")? {
            "input" => DescriptorKind::Input,
            "observe" => DescriptorKind::Observe,
            other => return Err(format!("`{other}` is not a descriptor kind")),
        };
        let ident_field = |field: &str| -> Result<Ident, String> {
            Ident::new(str_field(field)?).map_err(|e| format!("descriptor `{field}`: {e}"))
        };
        let mut carries = BTreeMap::new();
        match json.get("carries") {
            None => {}
            Some(serde_json::Value::Object(map)) => {
                for (field, ty) in map {
                    let field = Ident::new(field).map_err(|e| e.to_string())?;
                    let ty = ty
                        .as_str()
                        .ok_or("a `carries` entry names its shape as text")?;
                    carries.insert(field, ty.to_string());
                }
            }
            Some(_) => return Err("`carries` must be an object".into()),
        }
        Ok(Descriptor {
            kind,
            event: ident_field("event")?,
            emit: ident_field("emit")?,
            scope: str_field("scope")?.to_string(),
            payload: json
                .get("payload")
                .cloned()
                .ok_or("a descriptor needs a `payload`")?,
            carries,
        })
    }

    pub fn to_json(&self) -> serde_json::Value {
        let mut obj = serde_json::json!({
            "kind": match self.kind {
                DescriptorKind::Input => "input",
                DescriptorKind::Observe => "observe",
            },
            "event": self.event.to_string(),
            "emit": self.emit.to_string(),
            "scope": self.scope,
            "payload": self.payload,
        });
        if !self.carries.is_empty() {
            obj["carries"] = self
                .carries
                .iter()
                .map(|(f, ty)| (f.to_string(), serde_json::Value::String(ty.clone())))
                .collect::<serde_json::Map<_, _>>()
                .into();
        }
        obj
    }
}

impl Node {
    pub fn to_json(&self) -> serde_json::Value {
        let mut obj = serde_json::json!({
            "key": self.key,
            "element": self.element.to_string(),
            "props": self
                .props
                .iter()
                .map(|(name, v)| (name.to_string(), v.to_json()))
                .collect::<serde_json::Map<_, _>>(),
        });
        if let Some(class) = &self.class {
            obj["class"] = serde_json::Value::String(class.clone());
        }
        if !self.children.is_empty() {
            obj["children"] = self.children.iter().map(Node::to_json).collect();
        }
        if !self.on.is_empty() {
            obj["on"] = self.on.iter().map(Descriptor::to_json).collect();
        }
        obj
    }
}

impl Snapshot {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "protocol": VIEW_PROTOCOL,
            "revision": self.revision,
            "page": {
                "route": self.page.route,
                "root": self.page.root.to_json(),
            },
            "surfaces": self
                .surfaces
                .iter()
                .map(|s| {
                    let mut obj = serde_json::json!({
                        "key": s.key,
                        "definition": s.definition,
                        "modality": s.modality,
                        "dismiss": s.dismiss.to_json(),
                        "root": s.root.to_json(),
                    });
                    if let Some(rf) = &s.restore_focus {
                        obj["restore-focus"] = serde_json::Value::String(rf.clone());
                    }
                    obj
                })
                .collect::<Vec<_>>(),
        })
    }

    /// Canonical byte form (sorted keys, no floats by construction).
    pub fn to_canonical_string(&self) -> String {
        to_canonical_json(&self.to_json())
    }

    /// The `v-hash` that traces and goldens pin (§7.5).
    pub fn v_hash(&self) -> String {
        hash_json(&self.to_json())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_shapes_match_the_design() {
        assert_eq!(
            VValue::Plain("hi".into()).to_json().to_string(),
            r#"{"t":"plain","v":"hi"}"#
        );
        assert_eq!(
            VValue::Image("avatar-mira".into()).to_json().to_string(),
            r#"{"asset":"avatar-mira","t":"image"}"#
        );
        assert_eq!(
            VValue::Text("list".into()).to_json().to_string(),
            r#""list""#
        );
    }
}
