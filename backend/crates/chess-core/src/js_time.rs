//! Timestamps in the exact shape of JavaScript's `Date.prototype.toISOString()`.
//!
//! The TS reference writes `YYYY-MM-DDTHH:mm:ss.sssZ`: always UTC, always exactly three
//! fractional digits (`.000Z` for whole seconds), sub-millisecond digits dropped. Rust output
//! that must match TS JSON byte for byte uses [`iso`] (`#[serde(with = "chess_core::js_time::iso")]`)
//! or [`iso::option`]. `time::serde::rfc3339` is not used for output because it omits the fraction
//! on whole seconds and prints up to nine digits. Parsing accepts any RFC 3339 timestamp.

use time::format_description::FormatItem;
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;
use time::{OffsetDateTime, UtcOffset};

const JS_ISO: &[FormatItem<'static>] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z");

/// Formats like `new Date(t).toISOString()`: converted to UTC, milliseconds truncated.
pub fn to_iso_string(t: OffsetDateTime) -> String {
    t.to_offset(UtcOffset::UTC)
        .format(JS_ISO)
        .expect("a UTC timestamp with a four-digit year always formats")
}

/// Parses any RFC 3339 timestamp (including `toISOString()` output).
pub fn parse_iso(s: &str) -> Result<OffsetDateTime, time::error::Parse> {
    OffsetDateTime::parse(s, &Rfc3339)
}

/// Serde adapter for `OffsetDateTime` as a JS ISO string.
pub mod iso {
    use serde::{Deserialize, Deserializer, Serializer};
    use time::OffsetDateTime;

    pub fn serialize<S: Serializer>(t: &OffsetDateTime, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&super::to_iso_string(*t))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<OffsetDateTime, D::Error> {
        let s = String::deserialize(deserializer)?;
        super::parse_iso(&s).map_err(serde::de::Error::custom)
    }

    /// Serde adapter for `Option<OffsetDateTime>` (`null` for `None`).
    pub mod option {
        use serde::{Deserialize, Deserializer, Serializer};
        use time::OffsetDateTime;

        pub fn serialize<S: Serializer>(
            t: &Option<OffsetDateTime>,
            serializer: S,
        ) -> Result<S::Ok, S::Error> {
            match t {
                Some(t) => serializer.serialize_str(&super::super::to_iso_string(*t)),
                None => serializer.serialize_none(),
            }
        }

        pub fn deserialize<'de, D: Deserializer<'de>>(
            deserializer: D,
        ) -> Result<Option<OffsetDateTime>, D::Error> {
            Option::<String>::deserialize(deserializer)?
                .map(|s| super::super::parse_iso(&s).map_err(serde::de::Error::custom))
                .transpose()
        }
    }
}
