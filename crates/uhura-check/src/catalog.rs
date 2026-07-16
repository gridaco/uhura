//! The semantic element catalog as data (design §10): the model, the TOML
//! loader, the meta-schema (input events only on interactive elements;
//! observation events only on viewports), and the canonical-form hash the
//! IR pins.

use std::collections::{BTreeMap, BTreeSet};

use uhura_base::{Ident, hash_json};

/// One loaded, meta-schema-validated element catalog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Catalog {
    pub name: Ident,
    pub version: String,
    pub elements: BTreeMap<Ident, ElementDecl>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElementClass {
    Layout,
    Content,
    Interactive,
}

impl ElementClass {
    pub fn as_str(self) -> &'static str {
        match self {
            ElementClass::Layout => "layout",
            ElementClass::Content => "content",
            ElementClass::Interactive => "interactive",
        }
    }
}

/// What an element accepts as children (§10 children models).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChildrenModel {
    /// No children ever (`img`, `icon`, `textfield`).
    None,
    /// Any markup (`view`, `scroll`).
    Any,
    /// Content-class elements only (`button` — icon/text/img).
    Content,
    /// Exactly one child element (`region`).
    One,
    /// Children come from exactly one keyed `{#each}` (`pager`).
    KeyedEach,
    /// Literal text and `{expr}` interpolation only (`text`).
    Text,
}

impl ChildrenModel {
    pub fn as_str(self) -> &'static str {
        match self {
            ChildrenModel::None => "none",
            ChildrenModel::Any => "any",
            ChildrenModel::Content => "content",
            ChildrenModel::One => "one",
            ChildrenModel::KeyedEach => "keyed-each",
            ChildrenModel::Text => "text",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PropType {
    Text,
    Bool,
    Int,
    /// A closed token set; values typecheck as enum values (§4.3).
    Enum(BTreeSet<Ident>),
    /// An asset reference (`img src`).
    Asset,
    /// A name from the selected icon family's glyph registry.
    Icon,
    /// A statically selected icon family alias.
    IconFamily,
}

impl PropType {
    pub fn describe(&self) -> String {
        match self {
            PropType::Text => "text".into(),
            PropType::Bool => "bool".into(),
            PropType::Int => "int".into(),
            PropType::Enum(values) => {
                let list: Vec<&str> = values.iter().map(Ident::as_str).collect();
                format!("one of {}", list.join(" | "))
            }
            PropType::Asset => "an asset reference".into(),
            PropType::Icon => "an icon name".into(),
            PropType::IconFamily => "an icon family name".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropDecl {
    pub ty: PropType,
    pub required: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    /// User input; interactive elements only.
    Input,
    /// A semantic observation; viewport elements only.
    Observe,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventDecl {
    pub kind: EventKind,
    /// Renderer-carried payload fields (`change { value: text }`, §4.2).
    /// Carried fields are `text`/`bool`/`int` only.
    pub carries: BTreeMap<Ident, PropType>,
    /// `near-end`: integer percentage of one viewport extent (§8.2).
    pub threshold_percent: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElementDecl {
    pub class: ElementClass,
    /// Layout elements that own scrollable extent (`scroll`, `pager`).
    pub viewport: bool,
    pub children: ChildrenModel,
    pub props: BTreeMap<Ident, PropDecl>,
    pub events: BTreeMap<Ident, EventDecl>,
    /// Prop groups where exactly one member must be bound (`alt` xor
    /// `decorative`).
    pub exactly_one_of: Vec<Vec<Ident>>,
    /// Controlled promotion: binding `prop` obligates handling `event`.
    pub controlled: Option<(Ident, Ident)>,
}

/// A catalog-level problem, located by TOML key path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogIssue {
    pub path: String,
    pub message: String,
}

impl Catalog {
    /// The canonical JSON form — the byte form whose SHA-256 the checked IR
    /// pins (§10 "versioned + hash-pinned").
    pub fn to_canonical_json(&self) -> serde_json::Value {
        use serde_json::{Map, Value as J, json};
        let elements: Map<String, J> = self
            .elements
            .iter()
            .map(|(name, el)| {
                let props: Map<String, J> = el
                    .props
                    .iter()
                    .map(|(p, decl)| {
                        (
                            p.to_string(),
                            json!({
                                "type": prop_type_json(&decl.ty),
                                "required": decl.required,
                            }),
                        )
                    })
                    .collect();
                let events: Map<String, J> = el
                    .events
                    .iter()
                    .map(|(e, decl)| {
                        (
                            e.to_string(),
                            json!({
                                "kind": match decl.kind {
                                    EventKind::Input => "input",
                                    EventKind::Observe => "observe",
                                },
                                "carries": decl
                                    .carries
                                    .iter()
                                    .map(|(f, ty)| (f.to_string(), prop_type_json(ty)))
                                    .collect::<Map<String, J>>(),
                                "threshold-percent": decl.threshold_percent,
                            }),
                        )
                    })
                    .collect();
                (
                    name.to_string(),
                    json!({
                        "class": el.class.as_str(),
                        "viewport": el.viewport,
                        "children": el.children.as_str(),
                        "props": props,
                        "events": events,
                        "exactly-one-of": el
                            .exactly_one_of
                            .iter()
                            .map(|group| group.iter().map(ToString::to_string).collect::<Vec<_>>())
                            .collect::<Vec<_>>(),
                        "controlled": el
                            .controlled
                            .as_ref()
                            .map(|(p, e)| json!({ "prop": p.to_string(), "event": e.to_string() })),
                    }),
                )
            })
            .collect();
        json!({
            "catalog": "uhura-catalog/0",
            "name": self.name.to_string(),
            "version": self.version,
            "elements": elements,
        })
    }

    pub fn canonical_hash(&self) -> String {
        hash_json(&self.to_canonical_json())
    }

    /// The meta-schema (§10): event-kind eligibility by class, controlled
    /// and exactly-one-of references, carried-field types, icon-prop
    /// backing.
    pub fn meta_schema(&self) -> Vec<CatalogIssue> {
        let mut issues = Vec::new();
        let mut push = |path: String, message: String| issues.push(CatalogIssue { path, message });

        for (name, el) in &self.elements {
            let path = format!("elements.{name}");
            if el.viewport && el.class != ElementClass::Layout {
                push(
                    path.clone(),
                    "only layout elements can be viewports".to_string(),
                );
            }
            for (event, decl) in &el.events {
                let epath = format!("{path}.events.{event}");
                match decl.kind {
                    EventKind::Input if el.class != ElementClass::Interactive => push(
                        epath.clone(),
                        "input events are declarable only on interactive elements".to_string(),
                    ),
                    EventKind::Observe if !el.viewport => push(
                        epath.clone(),
                        "observation events are declarable only on viewports".to_string(),
                    ),
                    _ => {}
                }
                for (field, ty) in &decl.carries {
                    if !matches!(ty, PropType::Text | PropType::Bool | PropType::Int) {
                        push(
                            format!("{epath}.carries.{field}"),
                            "carried fields are text | bool | int".to_string(),
                        );
                    }
                }
                if decl
                    .threshold_percent
                    .is_some_and(|t| !(1..=200).contains(&t))
                {
                    push(
                        epath,
                        "`threshold-percent` must be an integer in 1..=200".to_string(),
                    );
                }
            }
            for group in &el.exactly_one_of {
                for prop in group {
                    if !el.props.contains_key(prop) {
                        push(
                            format!("{path}.exactly-one-of"),
                            format!("`{prop}` is not a prop of `{name}`"),
                        );
                    }
                }
                if group.len() < 2 {
                    push(
                        format!("{path}.exactly-one-of"),
                        "a group needs at least two props".to_string(),
                    );
                }
            }
            if let Some((prop, event)) = &el.controlled {
                if !el.props.contains_key(prop) {
                    push(
                        format!("{path}.controlled"),
                        format!("`{prop}` is not a prop of `{name}`"),
                    );
                }
                match el.events.get(event) {
                    None => push(
                        format!("{path}.controlled"),
                        format!("`{event}` is not an event of `{name}`"),
                    ),
                    Some(decl) if decl.kind != EventKind::Input => push(
                        format!("{path}.controlled"),
                        "the controlling event must be an input event".to_string(),
                    ),
                    Some(_) => {}
                }
            }
            for (prop, prop_decl) in &el.props {
                if prop.as_str() == "class" {
                    push(
                        format!("{path}.props.class"),
                        "`class` is universal and may not be redeclared".to_string(),
                    );
                }
                if matches!(prop_decl.ty, PropType::Icon)
                    && !(name.as_str() == "icon" && prop.as_str() == "name")
                {
                    push(
                        format!("{path}.props.{prop}"),
                        "the `icon` type is reserved for `<icon>`'s `name` prop".to_string(),
                    );
                }
                if matches!(prop_decl.ty, PropType::IconFamily)
                    && !(name.as_str() == "icon" && prop.as_str() == "family")
                {
                    push(
                        format!("{path}.props.{prop}"),
                        "the `icon-family` type is reserved for `<icon>`'s `family` prop"
                            .to_string(),
                    );
                }
            }
            if name.as_str() == "icon" {
                if el.class != ElementClass::Content {
                    push(
                        path.clone(),
                        "`<icon>` must be a content element".to_string(),
                    );
                }
                if el.children != ChildrenModel::None {
                    push(path.clone(), "`<icon>` cannot accept children".to_string());
                }
                if !el.events.is_empty() {
                    push(path.clone(), "`<icon>` cannot declare events".to_string());
                }
                match el.props.iter().find(|(prop, _)| prop.as_str() == "name") {
                    Some((_, decl)) if matches!(decl.ty, PropType::Icon) && decl.required => {}
                    _ => push(
                        format!("{path}.props.name"),
                        "`<icon>` requires a `name` prop of type `icon`".to_string(),
                    ),
                }
                match el.props.iter().find(|(prop, _)| prop.as_str() == "family") {
                    Some((_, decl))
                        if matches!(decl.ty, PropType::IconFamily) && !decl.required => {}
                    _ => push(
                        format!("{path}.props.family"),
                        "`<icon>` requires an optional `family` prop of type `icon-family`"
                            .to_string(),
                    ),
                }
            }
        }
        issues
    }
}

fn prop_type_json(ty: &PropType) -> serde_json::Value {
    use serde_json::json;
    match ty {
        PropType::Text => json!("text"),
        PropType::Bool => json!("bool"),
        PropType::Int => json!("int"),
        PropType::Asset => json!("asset"),
        PropType::Icon => json!("icon"),
        PropType::IconFamily => json!("icon-family"),
        PropType::Enum(values) => json!({
            "enum": values.iter().map(ToString::to_string).collect::<Vec<_>>(),
        }),
    }
}

/// Loads and validates a catalog. `Err` carries every issue (structural
/// and meta-schema); `Ok` catalogs are clean.
pub fn load_catalog(text: &str) -> Result<Catalog, Vec<CatalogIssue>> {
    let table: toml::Table = match text.parse() {
        Ok(t) => t,
        Err(e) => {
            return Err(vec![CatalogIssue {
                path: String::new(),
                message: format!("invalid TOML: {e}"),
            }]);
        }
    };
    let mut issues = Vec::new();
    let catalog = walk_catalog(&table, &mut issues);
    match catalog {
        Some(c) if issues.is_empty() => {
            let meta = c.meta_schema();
            if meta.is_empty() { Ok(c) } else { Err(meta) }
        }
        _ => Err(issues),
    }
}

fn walk_catalog(table: &toml::Table, issues: &mut Vec<CatalogIssue>) -> Option<Catalog> {
    let mut push = |path: &str, message: String| {
        issues.push(CatalogIssue {
            path: path.to_string(),
            message,
        });
    };

    for key in table.keys() {
        if !["catalog", "elements"].contains(&key.as_str()) {
            push(key, format!("unknown key `{key}`"));
        }
    }

    let head = match table.get("catalog").and_then(toml::Value::as_table) {
        Some(t) => t,
        None => {
            push("catalog", "missing `[catalog]` section".to_string());
            return None;
        }
    };
    for key in head.keys() {
        if !["name", "version"].contains(&key.as_str()) {
            push(&format!("catalog.{key}"), format!("unknown key `{key}`"));
        }
    }
    let name = ident_at("catalog.name", head.get("name"), issues)?;
    let version = match head.get("version").and_then(toml::Value::as_str) {
        Some(v) => v.to_string(),
        None => {
            issues.push(CatalogIssue {
                path: "catalog.version".into(),
                message: "missing required string".into(),
            });
            return None;
        }
    };
    let mut elements = BTreeMap::new();
    if let Some(section) = table.get("elements") {
        let Some(section) = section.as_table() else {
            issues.push(CatalogIssue {
                path: "elements".into(),
                message: "expected a table".into(),
            });
            return None;
        };
        for (el_name, decl) in section {
            let path = format!("elements.{el_name}");
            let Some(el_ident) = ident_key(&path, el_name, issues) else {
                continue;
            };
            let Some(decl) = decl.as_table() else {
                issues.push(CatalogIssue {
                    path,
                    message: "expected a table".into(),
                });
                continue;
            };
            if let Some(el) = walk_element(&path, decl, issues) {
                elements.insert(el_ident, el);
            }
        }
    }

    Some(Catalog {
        name,
        version,
        elements,
    })
}

fn walk_element(
    path: &str,
    table: &toml::Table,
    issues: &mut Vec<CatalogIssue>,
) -> Option<ElementDecl> {
    for key in table.keys() {
        if ![
            "class",
            "viewport",
            "children",
            "props",
            "events",
            "exactly-one-of",
            "controlled",
        ]
        .contains(&key.as_str())
        {
            issues.push(CatalogIssue {
                path: format!("{path}.{key}"),
                message: format!("unknown key `{key}`"),
            });
        }
    }

    let class = match table.get("class").and_then(toml::Value::as_str) {
        Some("layout") => ElementClass::Layout,
        Some("content") => ElementClass::Content,
        Some("interactive") => ElementClass::Interactive,
        Some(other) => {
            issues.push(CatalogIssue {
                path: format!("{path}.class"),
                message: format!("`{other}` is not a class (layout | content | interactive)"),
            });
            return None;
        }
        None => {
            issues.push(CatalogIssue {
                path: format!("{path}.class"),
                message: "missing required `class`".into(),
            });
            return None;
        }
    };

    let viewport = table
        .get("viewport")
        .and_then(toml::Value::as_bool)
        .unwrap_or(false);

    let children = match table.get("children").and_then(toml::Value::as_str) {
        Some("none") => ChildrenModel::None,
        Some("any") => ChildrenModel::Any,
        Some("content") => ChildrenModel::Content,
        Some("one") => ChildrenModel::One,
        Some("keyed-each") => ChildrenModel::KeyedEach,
        Some("text") => ChildrenModel::Text,
        Some(other) => {
            issues.push(CatalogIssue {
                path: format!("{path}.children"),
                message: format!(
                    "`{other}` is not a children model \
                     (none | any | content | one | keyed-each | text)"
                ),
            });
            return None;
        }
        None => {
            issues.push(CatalogIssue {
                path: format!("{path}.children"),
                message: "missing required `children`".into(),
            });
            return None;
        }
    };

    let mut props = BTreeMap::new();
    if let Some(toml::Value::Table(section)) = table.get("props") {
        for (prop_name, decl) in section {
            let ppath = format!("{path}.props.{prop_name}");
            let Some(prop_ident) = ident_key(&ppath, prop_name, issues) else {
                continue;
            };
            let Some(decl) = decl.as_table() else {
                issues.push(CatalogIssue {
                    path: ppath,
                    message: "expected a table".into(),
                });
                continue;
            };
            if let Some(prop) = walk_prop(&ppath, decl, issues) {
                props.insert(prop_ident, prop);
            }
        }
    }

    let mut events = BTreeMap::new();
    if let Some(toml::Value::Table(section)) = table.get("events") {
        for (event_name, decl) in section {
            let epath = format!("{path}.events.{event_name}");
            let Some(event_ident) = ident_key(&epath, event_name, issues) else {
                continue;
            };
            let Some(decl) = decl.as_table() else {
                issues.push(CatalogIssue {
                    path: epath,
                    message: "expected a table".into(),
                });
                continue;
            };
            if let Some(event) = walk_event(&epath, decl, issues) {
                events.insert(event_ident, event);
            }
        }
    }

    let mut exactly_one_of = Vec::new();
    if let Some(toml::Value::Array(groups)) = table.get("exactly-one-of") {
        for (i, group) in groups.iter().enumerate() {
            let gpath = format!("{path}.exactly-one-of[{i}]");
            let Some(members) = group.as_array() else {
                issues.push(CatalogIssue {
                    path: gpath,
                    message: "expected an array of prop names".into(),
                });
                continue;
            };
            let mut names = Vec::new();
            for member in members {
                match member.as_str().map(Ident::new) {
                    Some(Ok(name)) => names.push(name),
                    _ => issues.push(CatalogIssue {
                        path: gpath.clone(),
                        message: "expected a prop name".into(),
                    }),
                }
            }
            exactly_one_of.push(names);
        }
    }

    let controlled = match table.get("controlled") {
        None => None,
        Some(toml::Value::Table(t)) => {
            let prop = ident_at(&format!("{path}.controlled.prop"), t.get("prop"), issues);
            let event = ident_at(&format!("{path}.controlled.event"), t.get("event"), issues);
            match (prop, event) {
                (Some(p), Some(e)) => Some((p, e)),
                _ => None,
            }
        }
        Some(_) => {
            issues.push(CatalogIssue {
                path: format!("{path}.controlled"),
                message: "expected `{ prop = …, event = … }`".into(),
            });
            None
        }
    };

    Some(ElementDecl {
        class,
        viewport,
        children,
        props,
        events,
        exactly_one_of,
        controlled,
    })
}

fn walk_prop(path: &str, table: &toml::Table, issues: &mut Vec<CatalogIssue>) -> Option<PropDecl> {
    for key in table.keys() {
        if !["type", "values", "required"].contains(&key.as_str()) {
            issues.push(CatalogIssue {
                path: format!("{path}.{key}"),
                message: format!("unknown key `{key}`"),
            });
        }
    }
    let required = table
        .get("required")
        .and_then(toml::Value::as_bool)
        .unwrap_or(false);
    let ty = match table.get("type").and_then(toml::Value::as_str) {
        Some("text") => PropType::Text,
        Some("bool") => PropType::Bool,
        Some("int") => PropType::Int,
        Some("asset") => PropType::Asset,
        Some("icon") => PropType::Icon,
        Some("icon-family") => PropType::IconFamily,
        Some("enum") => {
            let mut values = BTreeSet::new();
            match table.get("values") {
                Some(toml::Value::Array(items)) => {
                    for item in items {
                        match item.as_str().map(Ident::new) {
                            Some(Ok(v)) => {
                                values.insert(v);
                            }
                            _ => issues.push(CatalogIssue {
                                path: format!("{path}.values"),
                                message: "enum values are kebab-case strings".into(),
                            }),
                        }
                    }
                }
                _ => issues.push(CatalogIssue {
                    path: format!("{path}.values"),
                    message: "an enum prop needs a `values` array".into(),
                }),
            }
            if values.is_empty() {
                return None;
            }
            PropType::Enum(values)
        }
        Some(other) => {
            issues.push(CatalogIssue {
                path: format!("{path}.type"),
                message: format!(
                    "`{other}` is not a prop type (text | bool | int | enum | asset | icon | icon-family)"
                ),
            });
            return None;
        }
        None => {
            issues.push(CatalogIssue {
                path: format!("{path}.type"),
                message: "missing required `type`".into(),
            });
            return None;
        }
    };
    Some(PropDecl { ty, required })
}

fn walk_event(
    path: &str,
    table: &toml::Table,
    issues: &mut Vec<CatalogIssue>,
) -> Option<EventDecl> {
    for key in table.keys() {
        if !["kind", "carries", "threshold-percent"].contains(&key.as_str()) {
            issues.push(CatalogIssue {
                path: format!("{path}.{key}"),
                message: format!("unknown key `{key}`"),
            });
        }
    }
    let kind = match table.get("kind").and_then(toml::Value::as_str) {
        Some("input") => EventKind::Input,
        Some("observe") => EventKind::Observe,
        Some(other) => {
            issues.push(CatalogIssue {
                path: format!("{path}.kind"),
                message: format!("`{other}` is not an event kind (input | observe)"),
            });
            return None;
        }
        None => {
            issues.push(CatalogIssue {
                path: format!("{path}.kind"),
                message: "missing required `kind`".into(),
            });
            return None;
        }
    };
    let mut carries = BTreeMap::new();
    if let Some(toml::Value::Table(section)) = table.get("carries") {
        for (field, ty) in section {
            let fpath = format!("{path}.carries.{field}");
            let Some(field_ident) = ident_key(&fpath, field, issues) else {
                continue;
            };
            let ty = match ty.as_str() {
                Some("text") => PropType::Text,
                Some("bool") => PropType::Bool,
                Some("int") => PropType::Int,
                _ => {
                    issues.push(CatalogIssue {
                        path: fpath,
                        message: "carried fields are text | bool | int".into(),
                    });
                    continue;
                }
            };
            carries.insert(field_ident, ty);
        }
    }
    let threshold_percent = match table.get("threshold-percent") {
        None => None,
        Some(toml::Value::Integer(n)) => Some(*n),
        Some(_) => {
            issues.push(CatalogIssue {
                path: format!("{path}.threshold-percent"),
                message: "`threshold-percent` must be an integer".into(),
            });
            None
        }
    };
    Some(EventDecl {
        kind,
        carries,
        threshold_percent,
    })
}

fn ident_at(
    path: &str,
    value: Option<&toml::Value>,
    issues: &mut Vec<CatalogIssue>,
) -> Option<Ident> {
    match value.and_then(toml::Value::as_str) {
        Some(s) => ident_key(path, s, issues),
        None => {
            issues.push(CatalogIssue {
                path: path.to_string(),
                message: "missing required string".into(),
            });
            None
        }
    }
}

fn ident_key(path: &str, s: &str, issues: &mut Vec<CatalogIssue>) -> Option<Ident> {
    match Ident::new(s) {
        Ok(i) => Some(i),
        Err(e) => {
            issues.push(CatalogIssue {
                path: path.to_string(),
                message: e.to_string(),
            });
            None
        }
    }
}
