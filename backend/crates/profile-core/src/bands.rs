//! Rating bands on the Lichess-equivalent scale (final in R2 Task 1).
//!
//! B0 < 900, B1 900-1299, B2 1300-1699, B3 1700-1999, B4 2000-2299, B5 >= 2300.

use chess_core::TimeClass;

use crate::constants::chesscom_to_lichess_offset;
use crate::types::{Band, PlatformId};

/// Band of a Lichess-equivalent rating. A NaN rating falls into B0.
pub fn band_of(lichess_eq: f64) -> Band {
    if lichess_eq.is_nan() || lichess_eq < 900.0 {
        Band::B0
    } else if lichess_eq < 1300.0 {
        Band::B1
    } else if lichess_eq < 1700.0 {
        Band::B2
    } else if lichess_eq < 2000.0 {
        Band::B3
    } else if lichess_eq < 2300.0 {
        Band::B4
    } else {
        Band::B5
    }
}

/// Lichess-equivalent rating: Lichess ratings unchanged, chess.com ratings plus the provisional
/// per-time-class offset.
pub fn to_lichess_equivalent(p: PlatformId, tc: TimeClass, rating: Option<i32>) -> Option<i32> {
    let r = rating?;
    Some(match p {
        PlatformId::Lichess => r,
        PlatformId::Chesscom => r + chesscom_to_lichess_offset(tc),
    })
}

/// Target rating for "rating + 200" comparisons (spec section 16.5).
pub fn rating_target(rating: Option<i32>) -> Option<i32> {
    rating.map(|r| r + 200)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nan_is_b0() {
        assert_eq!(band_of(f64::NAN), Band::B0);
        assert_eq!(band_of(f64::INFINITY), Band::B5);
    }
}
