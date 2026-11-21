//! Contract enums shared with profile-core (final in R2 Task 1).
//!
//! Wire strings are exactly the M3 zod enums (`AREA_IDS`, `BAND_IDS`, `WEAKNESS_STATES` in
//! `packages/ai/src/input/schema.ts`). `Ord` is declaration order, which is the zod order.

use std::fmt;
use std::str::FromStr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Returned by the `FromStr` impls of this module for an unknown wire string.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown {kind}: {value:?}")]
pub struct UnknownVariant {
    pub kind: &'static str,
    pub value: String,
}

macro_rules! wire_enum {
    ($(#[$meta:meta])* $name:ident, $kind:literal, $rename:literal, { $($variant:ident => $wire:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
            JsonSchema,
        )]
        #[serde(rename_all = $rename)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            /// Every variant, in declaration (wire) order.
            pub const ALL: [$name; [$($wire),+].len()] = [$($name::$variant),+];

            /// The wire string.
            pub fn as_str(self) -> &'static str {
                match self {
                    $($name::$variant => $wire),+
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = UnknownVariant;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $($wire => Ok($name::$variant),)+
                    _ => Err(UnknownVariant { kind: $kind, value: s.to_owned() }),
                }
            }
        }
    };
}

wire_enum!(
    /// One of the seven coaching areas (zod `AreaId`).
    AreaId, "area id", "lowercase", {
        Openings => "openings",
        Middlegame => "middlegame",
        Endgame => "endgame",
        Tactics => "tactics",
        Calculation => "calculation",
        Time => "time",
        Mindset => "mindset",
    }
);

wire_enum!(
    /// Rating band on the Lichess-equivalent scale (zod `BandId`).
    BandId, "band id", "UPPERCASE", {
        B0 => "B0",
        B1 => "B1",
        B2 => "B2",
        B3 => "B3",
        B4 => "B4",
        B5 => "B5",
    }
);

wire_enum!(
    /// Weakness timeline state (zod `WeaknessState`).
    WeaknessState, "weakness state", "snake_case", {
        NotAnIssue => "not_an_issue",
        Emerging => "emerging",
        Active => "active",
        Improving => "improving",
        Resolved => "resolved",
        Relapsed => "relapsed",
    }
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_matches_as_str() {
        for a in AreaId::ALL {
            assert_eq!(serde_json::to_value(a).unwrap(), a.as_str());
            assert_eq!(a.as_str().parse::<AreaId>().unwrap(), a);
        }
        for b in BandId::ALL {
            assert_eq!(serde_json::to_value(b).unwrap(), b.as_str());
            assert_eq!(b.as_str().parse::<BandId>().unwrap(), b);
        }
        for s in WeaknessState::ALL {
            assert_eq!(serde_json::to_value(s).unwrap(), s.as_str());
            assert_eq!(s.as_str().parse::<WeaknessState>().unwrap(), s);
        }
        assert!("B6".parse::<BandId>().is_err());
    }
}
