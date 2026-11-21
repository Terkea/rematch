//! Persistence of runs, reports, notes, chat and usage (runtime-checked sqlx, ADR-R3-3).
//!
//! Every repository function takes `&mut PgConnection`, so callers compose transactions. Enums
//! are bound as text and cast in SQL (`$1::report_run_status`), and read back as `col::text`.

pub mod batches;
pub mod chat;
pub mod interactive;
pub mod notes;
pub mod reports;
pub mod runs;
pub mod usage;
pub mod weekly;

/// A stored enum value that no Rust variant matches.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown {kind} value: {value:?}")]
pub struct ParseEnumError {
    pub kind: &'static str,
    pub value: String,
}

/// Declares a text-mapped enum: `as_str`, `FromStr`, `TryFrom<String>` (for
/// `#[sqlx(try_from = "String")]`), `Display` and snake_case serde.
macro_rules! text_enum {
    ($(#[$m:meta])* $name:ident, $pg:literal, { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
        pub enum $name {
            $(#[serde(rename = $text)] $variant),+
        }

        impl $name {
            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $text),+ }
            }
        }

        impl std::str::FromStr for $name {
            type Err = $crate::db::ParseEnumError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $($text => Ok(Self::$variant),)+
                    _ => Err($crate::db::ParseEnumError { kind: $pg, value: s.to_owned() }),
                }
            }
        }

        impl TryFrom<String> for $name {
            type Error = $crate::db::ParseEnumError;
            fn try_from(s: String) -> Result<Self, Self::Error> {
                s.parse()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}
pub(crate) use text_enum;

text_enum!(
    /// `ai_item_status`: weekly reviews, change notes (all four) and report sections (ready,
    /// dropped).
    ItemStatus, "ai_item_status", {
        Pending => "pending",
        Ready => "ready",
        Dropped => "dropped",
        Failed => "failed",
    }
);

text_enum!(
    /// How the Agent SDK session served an assistant turn (`coach_messages.session`, metrics;
    /// ADR-R3-6 amendment 1).
    SessionPath, "coach_messages.session", {
        New => "new",
        Resumed => "resumed",
        Rebuilt => "rebuilt",
    }
);

impl TryFrom<String> for crate::entitlements::Plan {
    type Error = ParseEnumError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        match s.as_str() {
            "free" => Ok(Self::Free),
            "pro" => Ok(Self::Pro),
            _ => Err(ParseEnumError {
                kind: "ai_plan",
                value: s,
            }),
        }
    }
}

pub(crate) fn plan_str(plan: crate::entitlements::Plan) -> &'static str {
    match plan {
        crate::entitlements::Plan::Free => "free",
        crate::entitlements::Plan::Pro => "pro",
    }
}

impl TryFrom<String> for crate::output::codes::SectionKey {
    type Error = ParseEnumError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse().map_err(|_| ParseEnumError {
            kind: "section",
            value: s,
        })
    }
}
