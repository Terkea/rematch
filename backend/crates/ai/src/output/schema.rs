//! Output schemas inside the structured-output JSON Schema subset (ADR-R3-4).
//!
//! `api_schema` generates draft 2020-12 with every subschema inlined (the output format must be
//! ref-free), applies R2's `contract_transform` (every property required, an `Option` stays
//! `T | null`), then `sanitize` removes what the API does not accept. `assert_api_subset` is the
//! guard: it allows only `type`, `properties`, `required` (all property names),
//! `additionalProperties: false`, `items`, `enum`, `const`, `anyOf` and `description`.
//!
//! The bounds the API cannot carry (text length, array sizes) are checked client-side by each
//! schema's `check`, after a strict `serde_path_to_error` deserialisation.

use schemars::JsonSchema;
use schemars::generate::SchemaSettings;
use schemars::transform::RecursiveTransform;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use serde_path_to_error::Segment;

use super::change::WeaknessChangeOutput;
use super::report::{Item, ReportSectionEnvelope};
use super::weekly::WeeklyReviewOutput;
use crate::client::port::{OutputSchema, ParseIssue};
use crate::input::types::contract_transform;

/// Client-side cap on every narrative text (ADR-R3-2 deviation 6), in Unicode code points.
pub const ITEM_TEXT_MAX_CHARS: usize = 2000;
/// `overview.strengths`.
pub const MAX_STRENGTHS: usize = 4;
/// `climbPlan.focuses`.
pub const MAX_FOCUSES: usize = 3;
/// `climbPlan.focuses[].drills`: 1 to 3.
pub const MIN_DRILLS: usize = 1;
pub const MAX_DRILLS: usize = 3;
/// Weekly `outcomes`.
pub const MAX_WEEKLY_OUTCOMES: usize = 3;
/// Weekly `exemplars`.
pub const MAX_WEEKLY_EXEMPLARS: usize = 2;

/// Keywords the structured-output subset rejects or that carry no meaning for it. Their bounds
/// are enforced by the `check` functions instead.
const DROPPED: &[&str] = &[
    "$schema",
    "title",
    "format",
    "minLength",
    "maxLength",
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "multipleOf",
    "pattern",
    "minItems",
    "maxItems",
    "default",
];

/// The only keywords `assert_api_subset` accepts.
const ALLOWED: &[&str] = &[
    "type",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "enum",
    "const",
    "anyOf",
    "description",
];

const TYPES: &[&str] = &[
    "object", "array", "string", "integer", "number", "boolean", "null",
];

/// The sanitized, ref-free API schema of `T`.
pub fn api_schema<T: JsonSchema>() -> Value {
    let mut settings = SchemaSettings::draft2020_12();
    settings.inline_subschemas = true;
    let mut v = settings
        .with_transform(RecursiveTransform(contract_transform))
        .into_generator()
        .into_root_schema_for::<T>()
        .to_value();
    sanitize(&mut v);
    v
}

/// Rewrites a schemars schema into the subset, in place (see [`DROPPED`]). Also:
/// `type: [X, "null"]` becomes `anyOf: [{…X}, {"type": "null"}]`; `type` beside `enum` is
/// dropped (the enum fixes the type, and the zod export has none); every object gets
/// `additionalProperties: false`. Recurses through `properties`, `items`, `anyOf`, `oneOf`,
/// `allOf` and `$defs` only, so a property named like a keyword (`title`) is kept.
pub fn sanitize(node: &mut Value) {
    let Value::Object(o) = node else {
        return;
    };
    for k in DROPPED {
        o.remove(*k);
    }
    if let Some(Value::Array(types)) = o.get("type")
        && types.len() == 2
        && types.contains(&json!("null"))
    {
        let other = types
            .iter()
            .find(|t| *t != "null")
            .cloned()
            .unwrap_or_default();
        let description = o.remove("description");
        let mut member = std::mem::take(o);
        member.insert("type".into(), other);
        if let Some(Value::Array(e)) = member.get_mut("enum") {
            e.retain(|x| !x.is_null());
        }
        o.insert(
            "anyOf".into(),
            json!([Value::Object(member), { "type": "null" }]),
        );
        if let Some(d) = description {
            o.insert("description".into(), d);
        }
    }
    if o.contains_key("enum") && o.get("type").is_some_and(Value::is_string) {
        o.remove("type");
    }
    if let Some(Value::Object(props)) = o.get_mut("properties") {
        props.values_mut().for_each(sanitize);
    }
    if let Some(Value::Object(defs)) = o.get_mut("$defs") {
        defs.values_mut().for_each(sanitize);
    }
    if let Some(items) = o.get_mut("items") {
        sanitize(items);
    }
    for key in ["anyOf", "oneOf", "allOf"] {
        if let Some(Value::Array(members)) = o.get_mut(key) {
            members.iter_mut().for_each(sanitize);
        }
    }
    if o.get("type") == Some(&json!("object")) || o.contains_key("properties") {
        o.insert("additionalProperties".into(), Value::Bool(false));
    }
}

/// `Ok` when every node of `v` is inside the subset ([`ALLOWED`]); `Err` names the first node
/// that is not.
pub fn assert_api_subset(v: &Value) -> Result<(), String> {
    guard(v, "$")
}

fn guard(node: &Value, path: &str) -> Result<(), String> {
    let o = node
        .as_object()
        .ok_or_else(|| format!("{path}: a schema must be an object"))?;
    if let Some(k) = o.keys().find(|k| !ALLOWED.contains(&k.as_str())) {
        return Err(format!("{path}: keyword {k:?} is outside the subset"));
    }
    if let Some(t) = o.get("type") {
        match t.as_str() {
            Some(t) if TYPES.contains(&t) => {}
            _ => return Err(format!("{path}: type {t} must be one basic type")),
        }
    }
    if let Some(ap) = o.get("additionalProperties")
        && ap != &Value::Bool(false)
    {
        return Err(format!("{path}: additionalProperties must be false"));
    }
    let is_object = o.get("type") == Some(&json!("object"));
    if is_object && o.get("additionalProperties").is_none() {
        return Err(format!(
            "{path}: object without additionalProperties: false"
        ));
    }
    match (o.get("properties"), o.get("required")) {
        (None, None) if is_object => {
            return Err(format!("{path}: object without properties"));
        }
        (None, None) => {}
        (Some(Value::Object(props)), Some(Value::Array(req))) => {
            let mut names: Vec<&str> = props.keys().map(String::as_str).collect();
            let mut req: Vec<&str> = req.iter().filter_map(Value::as_str).collect();
            names.sort_unstable();
            req.sort_unstable();
            if names != req {
                return Err(format!(
                    "{path}: required {req:?} is not every property {names:?}"
                ));
            }
            for (k, p) in props {
                guard(p, &format!("{path}.{k}"))?;
            }
        }
        _ => {
            return Err(format!(
                "{path}: properties and required must come together"
            ));
        }
    }
    if let Some(items) = o.get("items") {
        guard(items, &format!("{path}[]"))?;
    }
    if let Some(any) = o.get("anyOf") {
        let members = any
            .as_array()
            .filter(|a| !a.is_empty())
            .ok_or_else(|| format!("{path}: anyOf must be a non-empty array"))?;
        for (i, m) in members.iter().enumerate() {
            guard(m, &format!("{path}|{i}"))?;
        }
    }
    if let Some(e) = o.get("enum")
        && !e.as_array().is_some_and(|a| !a.is_empty())
    {
        return Err(format!("{path}: enum must be a non-empty array"));
    }
    if let Some(d) = o.get("description")
        && !d.is_string()
    {
        return Err(format!("{path}: description must be a string"));
    }
    Ok(())
}

// --- Client-side checks ---

/// `a.0.b`, as `parse_coaching_input` writes paths (zod's dotted form).
fn dotted(path: &serde_path_to_error::Path) -> String {
    path.iter()
        .filter_map(|seg| match seg {
            Segment::Seq { index } => Some(index.to_string()),
            Segment::Map { key } => Some(key.clone()),
            Segment::Enum { variant } => Some(variant.clone()),
            Segment::Unknown => None,
        })
        .collect::<Vec<_>>()
        .join(".")
}

fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_owned()
    } else {
        format!("{path}.{key}")
    }
}

/// Strict deserialisation; a shape error becomes one issue with a path and a code, never the
/// offending value (model content stays out of issues). A missing key's path names the key.
pub fn deserialize_checked<T: DeserializeOwned>(v: &Value) -> Result<T, Vec<ParseIssue>> {
    serde_path_to_error::deserialize::<_, T>(v).map_err(|e| {
        let path = dotted(e.path());
        let message = e.inner().to_string();
        let issue = |path: String, code: &str| ParseIssue {
            path,
            code: code.to_owned(),
        };
        let found = if let Some(key) = message
            .strip_prefix("missing field `")
            .and_then(|m| m.split('`').next())
        {
            issue(join(&path, key), "missing_field")
        } else if message.starts_with("unknown field") {
            issue(path, "unknown_field")
        } else if message.starts_with("unknown variant") || message.starts_with("invalid value") {
            issue(path, "invalid_value")
        } else {
            issue(path, "invalid_type")
        };
        vec![found]
    })
}

#[derive(Default)]
struct Bounds(Vec<ParseIssue>);

impl Bounds {
    fn push(&mut self, path: String, code: &str) {
        self.0.push(ParseIssue {
            path,
            code: code.to_owned(),
        });
    }

    fn text(&mut self, path: &str, text: &str) {
        let n = text.chars().count();
        if n == 0 {
            self.push(join(path, "text"), "too_small");
        } else if n > ITEM_TEXT_MAX_CHARS {
            self.push(join(path, "text"), "too_big");
        }
    }

    fn item(&mut self, path: &str, item: &Item) {
        self.text(path, &item.text);
    }

    fn len(&mut self, path: &str, len: usize, min: usize, max: usize) {
        if len < min {
            self.push(path.to_owned(), "too_small");
        } else if len > max {
            self.push(path.to_owned(), "too_big");
        }
    }

    fn done(self) -> Result<(), Vec<ParseIssue>> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(self.0)
        }
    }
}

/// The bounds of a deserialised envelope, in field order.
pub fn report_bounds(e: &ReportSectionEnvelope) -> Result<(), Vec<ParseIssue>> {
    let mut b = Bounds::default();
    if let Some(ov) = &e.overview {
        b.item("overview.headline", &ov.headline);
        b.item("overview.summary", &ov.summary);
        b.len("overview.strengths", ov.strengths.len(), 0, MAX_STRENGTHS);
        for (i, s) in ov.strengths.iter().enumerate() {
            b.text(&format!("overview.strengths.{i}"), &s.text);
        }
    }
    if let Some(w) = &e.weaknesses {
        for (i, it) in w.items.iter().enumerate() {
            b.text(&format!("weaknesses.items.{i}"), &it.text);
        }
    }
    if let Some(o) = &e.openings {
        for (i, it) in o.items.iter().enumerate() {
            b.text(&format!("openings.items.{i}"), &it.text);
        }
    }
    if let Some(cp) = &e.climb_plan {
        b.item("climbPlan.intro", &cp.intro);
        b.len("climbPlan.focuses", cp.focuses.len(), 0, MAX_FOCUSES);
        for (i, f) in cp.focuses.iter().enumerate() {
            let p = format!("climbPlan.focuses.{i}");
            b.item(&join(&p, "why"), &f.why);
            b.len(&join(&p, "drills"), f.drills.len(), MIN_DRILLS, MAX_DRILLS);
            b.item(&join(&p, "habit"), &f.habit);
        }
        b.item("climbPlan.weeklyRhythm", &cp.weekly_rhythm);
    }
    if let Some(a) = &e.areas {
        for (i, it) in a.items.iter().enumerate() {
            b.text(&format!("areas.items.{i}"), &it.text);
        }
    }
    b.done()
}

/// The bounds of a deserialised weekly review, in field order.
pub fn weekly_bounds(w: &WeeklyReviewOutput) -> Result<(), Vec<ParseIssue>> {
    let mut b = Bounds::default();
    b.item("headline", &w.headline);
    b.item("process", &w.process);
    b.len("outcomes", w.outcomes.len(), 0, MAX_WEEKLY_OUTCOMES);
    for (i, it) in w.outcomes.iter().enumerate() {
        b.item(&format!("outcomes.{i}"), it);
    }
    b.len("exemplars", w.exemplars.len(), 0, MAX_WEEKLY_EXEMPLARS);
    for (i, it) in w.exemplars.iter().enumerate() {
        b.item(&format!("exemplars.{i}"), it);
    }
    b.item("nextWeek", &w.next_week);
    b.done()
}

/// The bounds of a deserialised change note.
pub fn change_bounds(c: &WeaknessChangeOutput) -> Result<(), Vec<ParseIssue>> {
    let mut b = Bounds::default();
    b.text("", &c.text);
    b.done()
}

/// Deserialises and checks a report-section envelope.
pub fn parse_report_section(v: &Value) -> Result<ReportSectionEnvelope, Vec<ParseIssue>> {
    let e = deserialize_checked::<ReportSectionEnvelope>(v)?;
    report_bounds(&e).map(|()| e)
}

/// Deserialises and checks a weekly review.
pub fn parse_weekly_review(v: &Value) -> Result<WeeklyReviewOutput, Vec<ParseIssue>> {
    let w = deserialize_checked::<WeeklyReviewOutput>(v)?;
    weekly_bounds(&w).map(|()| w)
}

/// Deserialises and checks a change note.
pub fn parse_weakness_change(v: &Value) -> Result<WeaknessChangeOutput, Vec<ParseIssue>> {
    let c = deserialize_checked::<WeaknessChangeOutput>(v)?;
    change_bounds(&c).map(|()| c)
}

/// The report-section envelope (one schema for all five section calls).
pub static REPORT_SECTION_SCHEMA: OutputSchema = OutputSchema {
    id: "report-section",
    version: 1,
    json: api_schema::<ReportSectionEnvelope>,
    check: |v| parse_report_section(v).map(|_| ()),
};

pub static WEEKLY_REVIEW_SCHEMA: OutputSchema = OutputSchema {
    id: "weekly-review",
    version: 1,
    json: api_schema::<WeeklyReviewOutput>,
    check: |v| parse_weekly_review(v).map(|_| ()),
};

pub static WEAKNESS_CHANGE_SCHEMA: OutputSchema = OutputSchema {
    id: "weakness-change",
    version: 1,
    json: api_schema::<WeaknessChangeOutput>,
    check: |v| parse_weakness_change(v).map(|_| ()),
};

static ALL_SCHEMAS: [&OutputSchema; 3] = [
    &REPORT_SECTION_SCHEMA,
    &WEEKLY_REVIEW_SCHEMA,
    &WEAKNESS_CHANGE_SCHEMA,
];

/// The schema with this id and version (batch results resolve their schema by custom id).
pub fn schema_by_id(id: &str, version: u32) -> Option<&'static OutputSchema> {
    ALL_SCHEMAS
        .iter()
        .copied()
        .find(|s| s.id == id && s.version == version)
}
