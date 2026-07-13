//! The closed port-contract type grammar (design §9.1):
//! `bool | int | text | id | asset | option<T> | list<T> | <declared>`.
//!
//! `id` and `asset` are ambient builtins (the normative §9.1 TOML uses them
//! bare); `kind = "id" | "opaque" | "asset"` additionally lets a contract
//! declare *named* nominal types of those kinds (`feed-cursor`). `tag` is
//! core-minted and can never appear in a contract.

use std::fmt;

use uhura_base::Ident;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeExpr {
    Bool,
    Int,
    Text,
    /// The ambient nominal-identity builtin.
    Id,
    /// The ambient asset-reference builtin.
    Asset,
    Option(Box<TypeExpr>),
    List(Box<TypeExpr>),
    /// A contract-declared type name, resolved against the port's `[types]`.
    Named(Ident),
}

impl TypeExpr {
    /// Parses the strict wire grammar. No interior whitespace; single-
    /// argument generics only.
    pub fn parse(s: &str) -> Result<TypeExpr, String> {
        match s {
            "bool" => return Ok(TypeExpr::Bool),
            "int" => return Ok(TypeExpr::Int),
            "text" => return Ok(TypeExpr::Text),
            "id" => return Ok(TypeExpr::Id),
            "asset" => return Ok(TypeExpr::Asset),
            _ => {}
        }
        if let Some(inner) = generic_arg(s, "option") {
            return Ok(TypeExpr::Option(Box::new(TypeExpr::parse(inner)?)));
        }
        if let Some(inner) = generic_arg(s, "list") {
            return Ok(TypeExpr::List(Box::new(TypeExpr::parse(inner)?)));
        }
        match Ident::new(s) {
            Ok(name) => Ok(TypeExpr::Named(name)),
            Err(_) => Err(format!(
                "`{s}` is not a type: expected `bool`, `int`, `text`, `id`, \
                 `asset`, `option<T>`, `list<T>`, or a declared type name"
            )),
        }
    }

    /// The declared type names this expression references (for resolution
    /// and cycle checks).
    pub fn named_refs<'a>(&'a self, out: &mut Vec<&'a Ident>) {
        match self {
            TypeExpr::Named(n) => out.push(n),
            TypeExpr::Option(t) | TypeExpr::List(t) => t.named_refs(out),
            _ => {}
        }
    }
}

fn generic_arg<'a>(s: &'a str, head: &str) -> Option<&'a str> {
    let rest = s.strip_prefix(head)?;
    let rest = rest.strip_prefix('<')?;
    rest.strip_suffix('>')
}

impl fmt::Display for TypeExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeExpr::Bool => write!(f, "bool"),
            TypeExpr::Int => write!(f, "int"),
            TypeExpr::Text => write!(f, "text"),
            TypeExpr::Id => write!(f, "id"),
            TypeExpr::Asset => write!(f, "asset"),
            TypeExpr::Option(t) => write!(f, "option<{t}>"),
            TypeExpr::List(t) => write!(f, "list<{t}>"),
            TypeExpr::Named(n) => write!(f, "{n}"),
        }
    }
}

/// Names a contract may not declare a type under (they are grammar).
pub const RESERVED_TYPE_NAMES: &[&str] = &[
    "bool", "int", "text", "id", "asset", "tag", "option", "list", "map", "none",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_builtins_and_generics() {
        assert_eq!(TypeExpr::parse("bool").unwrap(), TypeExpr::Bool);
        assert_eq!(
            TypeExpr::parse("option<feed-cursor>").unwrap().to_string(),
            "option<feed-cursor>"
        );
        assert_eq!(
            TypeExpr::parse("list<option<id>>").unwrap(),
            TypeExpr::List(Box::new(TypeExpr::Option(Box::new(TypeExpr::Id))))
        );
    }

    #[test]
    fn rejects_junk() {
        for bad in ["", "Bool", "list<", "list<>", "option<a b>", "map<id>"] {
            assert!(TypeExpr::parse(bad).is_err(), "{bad}");
        }
        // `map` alone parses as an ident-shaped name; the reserved-name check
        // in contract validation rejects declaring it.
        assert!(TypeExpr::parse("map").is_ok());
    }
}
