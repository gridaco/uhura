//! `ports/*.port.toml` → `PortContract` (feature `toml`; core never
//! compiles this — the purity suite proves it). Strict walking: unknown
//! keys are errors, every issue carries its TOML key path.

use std::collections::{BTreeMap, BTreeSet};

use uhura_base::Ident;

use crate::contract::{CommandDecl, ContractIssue, PortContract, ProjectionDecl, TypeDecl};
use crate::types::TypeExpr;

/// Loads and fully validates one port contract. `Err` carries every issue
/// found (structural and well-formedness); `Ok` contracts are clean.
pub fn load_port_contract(text: &str) -> Result<PortContract, Vec<ContractIssue>> {
    let table: toml::Table = match text.parse() {
        Ok(t) => t,
        Err(e) => {
            return Err(vec![ContractIssue {
                path: String::new(),
                message: format!("invalid TOML: {e}"),
            }]);
        }
    };
    let mut issues = Vec::new();
    let mut walker = Walker {
        issues: &mut issues,
    };
    let contract = walker.contract(&table);
    match contract {
        Some(c) if issues.is_empty() => {
            let wf = c.well_formedness();
            if wf.is_empty() { Ok(c) } else { Err(wf) }
        }
        _ => Err(issues),
    }
}

struct Walker<'a> {
    issues: &'a mut Vec<ContractIssue>,
}

impl Walker<'_> {
    fn push(&mut self, path: &str, message: impl Into<String>) {
        self.issues.push(ContractIssue {
            path: path.to_string(),
            message: message.into(),
        });
    }

    fn contract(&mut self, table: &toml::Table) -> Option<PortContract> {
        self.expect_keys(
            "",
            table,
            &["port", "types", "projections", "refusals", "commands"],
        );

        let port = self.table_at("port", table.get("port"))?;
        self.expect_keys("port", port, &["name", "version"]);
        let name = self.ident("port.name", port.get("name"))?;
        let version = self.string("port.version", port.get("version"))?;

        let mut types = BTreeMap::new();
        if let Some(v) = table.get("types") {
            for (type_name, decl) in self.named_tables("types", v) {
                let path = format!("types.{type_name}");
                let Some(type_ident) = self.ident_key(&path, &type_name) else {
                    continue;
                };
                if let Some(decl) = self.type_decl(&path, decl) {
                    types.insert(type_ident, decl);
                }
            }
        }

        let mut projections = BTreeMap::new();
        if let Some(v) = table.get("projections") {
            for (proj_name, decl) in self.named_tables("projections", v) {
                let path = format!("projections.{proj_name}");
                let Some(proj_ident) = self.ident_key(&path, &proj_name) else {
                    continue;
                };
                if let Some(decl) = self.projection_decl(&path, decl) {
                    projections.insert(proj_ident, decl);
                }
            }
        }

        let mut refusals = BTreeSet::new();
        if let Some(v) = table.get("refusals") {
            for (refusal_name, decl) in self.named_tables("refusals", v) {
                let path = format!("refusals.{refusal_name}");
                if !decl.is_empty() {
                    self.push(&path, "refusals carry no fields in the spike (§9.1)");
                }
                if let Some(ident) = self.ident_key(&path, &refusal_name) {
                    refusals.insert(ident);
                }
            }
        }

        let mut commands = BTreeMap::new();
        if let Some(v) = table.get("commands") {
            for (cmd_name, decl) in self.named_tables("commands", v) {
                let path = format!("commands.{cmd_name}");
                let Some(cmd_ident) = self.ident_key(&path, &cmd_name) else {
                    continue;
                };
                if let Some(decl) = self.command_decl(&path, decl) {
                    commands.insert(cmd_ident, decl);
                }
            }
        }

        Some(PortContract {
            name,
            version,
            types,
            projections,
            refusals,
            commands,
        })
    }

    fn type_decl(&mut self, path: &str, table: &toml::Table) -> Option<TypeDecl> {
        let kind = self.string(&format!("{path}.kind"), table.get("kind"))?;
        match kind.as_str() {
            "record" => {
                self.expect_keys(path, table, &["kind", "fields"]);
                let fields = match table.get("fields") {
                    Some(v) => self.type_map(&format!("{path}.fields"), Some(v))?,
                    None => BTreeMap::new(),
                };
                Some(TypeDecl::Record { fields })
            }
            "union" => {
                self.expect_keys(path, table, &["kind", "variants"]);
                let mut variants = BTreeMap::new();
                if let Some(v) = table.get("variants") {
                    for (variant_name, fields) in self.named_tables(&format!("{path}.variants"), v)
                    {
                        let vpath = format!("{path}.variants.{variant_name}");
                        let Some(variant_ident) = self.ident_key(&vpath, &variant_name) else {
                            continue;
                        };
                        let fields_value = toml::Value::Table(fields.clone());
                        if let Some(fields) = self.type_map(&vpath, Some(&fields_value)) {
                            variants.insert(variant_ident, fields);
                        }
                    }
                }
                Some(TypeDecl::Union { variants })
            }
            "enum" => {
                self.expect_keys(path, table, &["kind", "values"]);
                let mut values = BTreeSet::new();
                match table.get("values") {
                    Some(toml::Value::Array(items)) => {
                        for (i, item) in items.iter().enumerate() {
                            let vpath = format!("{path}.values[{i}]");
                            if let Some(s) = self.string_value(&vpath, item)
                                && let Some(ident) = self.ident_key(&vpath, &s)
                            {
                                values.insert(ident);
                            }
                        }
                    }
                    _ => self.push(path, "an enum needs a `values` array"),
                }
                Some(TypeDecl::Enum { values })
            }
            "id" => {
                self.expect_keys(path, table, &["kind"]);
                Some(TypeDecl::Id)
            }
            "opaque" => {
                self.expect_keys(path, table, &["kind"]);
                Some(TypeDecl::Opaque)
            }
            "asset" => {
                self.expect_keys(path, table, &["kind"]);
                Some(TypeDecl::Asset)
            }
            other => {
                self.push(
                    path,
                    format!(
                        "`{other}` is not a type kind \
                         (record | union | enum | id | opaque | asset)"
                    ),
                );
                None
            }
        }
    }

    fn projection_decl(&mut self, path: &str, table: &toml::Table) -> Option<ProjectionDecl> {
        self.expect_keys(path, table, &["type", "key", "boot"]);
        let ty = self.type_expr(&format!("{path}.type"), table.get("type"))?;
        let key = match table.get("key") {
            None => None,
            some => Some(self.type_expr(&format!("{path}.key"), some)?),
        };
        let boot = match table.get("boot") {
            None => false,
            Some(toml::Value::Boolean(b)) => *b,
            Some(_) => {
                self.push(&format!("{path}.boot"), "`boot` must be a bool");
                false
            }
        };
        Some(ProjectionDecl { ty, key, boot })
    }

    fn command_decl(&mut self, path: &str, table: &toml::Table) -> Option<CommandDecl> {
        self.expect_keys(path, table, &["payload", "refusals"]);
        let payload = self.type_map(&format!("{path}.payload"), table.get("payload"))?;
        let mut refusals = BTreeSet::new();
        match table.get("refusals") {
            None => {}
            Some(toml::Value::Array(items)) => {
                for (i, item) in items.iter().enumerate() {
                    let rpath = format!("{path}.refusals[{i}]");
                    if let Some(s) = self.string_value(&rpath, item)
                        && let Some(ident) = self.ident_key(&rpath, &s)
                        && !refusals.insert(ident)
                    {
                        self.push(&rpath, format!("duplicate refusal `{s}`"));
                    }
                }
            }
            Some(_) => self.push(&format!("{path}.refusals"), "`refusals` must be an array"),
        }
        Some(CommandDecl { payload, refusals })
    }

    fn type_map(
        &mut self,
        path: &str,
        value: Option<&toml::Value>,
    ) -> Option<BTreeMap<Ident, TypeExpr>> {
        let table = self.table_at(path, value)?;
        let mut out = BTreeMap::new();
        for (field, v) in table {
            let fpath = format!("{path}.{field}");
            let Some(field_ident) = self.ident_key(&fpath, field) else {
                continue;
            };
            if let Some(ty) = self.type_expr(&fpath, Some(v)) {
                out.insert(field_ident, ty);
            }
        }
        Some(out)
    }

    fn type_expr(&mut self, path: &str, value: Option<&toml::Value>) -> Option<TypeExpr> {
        let s = self.string(path, value)?;
        match TypeExpr::parse(&s) {
            Ok(ty) => Some(ty),
            Err(e) => {
                self.push(path, e);
                None
            }
        }
    }

    /// The (name, table) entries of a table-of-tables section.
    fn named_tables<'v>(
        &mut self,
        path: &str,
        value: &'v toml::Value,
    ) -> Vec<(String, &'v toml::Table)> {
        let Some(table) = value.as_table() else {
            self.push(path, "expected a table");
            return Vec::new();
        };
        let mut out = Vec::new();
        for (name, v) in table {
            match v.as_table() {
                Some(t) => out.push((name.clone(), t)),
                None => self.push(&format!("{path}.{name}"), "expected a table"),
            }
        }
        out
    }

    fn table_at<'v>(
        &mut self,
        path: &str,
        value: Option<&'v toml::Value>,
    ) -> Option<&'v toml::Table> {
        match value {
            Some(toml::Value::Table(t)) => Some(t),
            Some(_) => {
                self.push(path, "expected a table");
                None
            }
            None => {
                self.push(path, "missing required section");
                None
            }
        }
    }

    fn string(&mut self, path: &str, value: Option<&toml::Value>) -> Option<String> {
        match value {
            Some(v) => self.string_value(path, v),
            None => {
                self.push(path, "missing required string");
                None
            }
        }
    }

    fn string_value(&mut self, path: &str, value: &toml::Value) -> Option<String> {
        match value.as_str() {
            Some(s) => Some(s.to_string()),
            None => {
                self.push(path, "expected a string");
                None
            }
        }
    }

    fn ident(&mut self, path: &str, value: Option<&toml::Value>) -> Option<Ident> {
        let s = self.string(path, value)?;
        self.ident_key(path, &s)
    }

    fn ident_key(&mut self, path: &str, s: &str) -> Option<Ident> {
        match Ident::new(s) {
            Ok(i) => Some(i),
            Err(e) => {
                self.push(path, e.to_string());
                None
            }
        }
    }

    fn expect_keys(&mut self, path: &str, table: &toml::Table, allowed: &[&str]) {
        for key in table.keys() {
            if !allowed.contains(&key.as_str()) {
                let full = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                self.push(&full, format!("unknown key `{key}`"));
            }
        }
    }
}
