//! Batch `custom_id` is `<schema_id>__v<version>__<reference>` (port of `custom-id.ts`). The
//! API allows only `[A-Za-z0-9_-]{1,64}`, so `@` and `:` are not usable. `schema_id` has no
//! underscores and `version` is digits, so the first two `__` delimit unambiguously;
//! `reference` may contain anything allowed, `__` included.

use std::sync::LazyLock;

use regex::Regex;

pub const CUSTOM_ID_MAX: usize = 64;

static SCHEMA_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("^[A-Za-z0-9-]+$").expect("valid schema id pattern"));
static REFERENCE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("^[A-Za-z0-9_-]+$").expect("valid reference pattern"));
static CUSTOM_ID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([A-Za-z0-9-]+)__v(\d+)__([A-Za-z0-9_-]+)$").expect("valid custom id pattern")
});

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CustomIdParts {
    pub schema_id: String,
    pub version: u32,
    pub reference: String,
}

pub fn encode_custom_id(p: &CustomIdParts) -> Result<String, String> {
    if !SCHEMA_ID.is_match(&p.schema_id) {
        return Err("schemaId must match [A-Za-z0-9-]+".into());
    }
    if !REFERENCE.is_match(&p.reference) {
        return Err("ref must match [A-Za-z0-9_-]+".into());
    }
    let id = format!("{}__v{}__{}", p.schema_id, p.version, p.reference);
    if id.len() > CUSTOM_ID_MAX {
        return Err(format!("custom_id exceeds {CUSTOM_ID_MAX} characters"));
    }
    Ok(id)
}

pub fn decode_custom_id(id: &str) -> Option<CustomIdParts> {
    if id.len() > CUSTOM_ID_MAX {
        return None;
    }
    let c = CUSTOM_ID.captures(id)?;
    Some(CustomIdParts {
        schema_id: c.get(1)?.as_str().to_owned(),
        version: c.get(2)?.as_str().parse().ok()?,
        reference: c.get(3)?.as_str().to_owned(),
    })
}
