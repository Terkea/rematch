//! R2 Task 1: bands, constants and key strings match TS M2 Task 1.

use chess_core::TimeClass::{Blitz, Rapid};
use profile_core::bands::{band_of, rating_target, to_lichess_equivalent};
use profile_core::constants::{RANKABLE, min_evidence, n0, trainability};
use profile_core::types::Band::{B0, B1, B2, B3, B4, B5};
use profile_core::types::MotifTag::{BackRankMate, Fork, MateIn4Plus};
use profile_core::types::PlatformId::{Chesscom, Lichess};
use profile_core::types::WeaknessKey::{
    self, W1, W3Deep, W3Shallow, W4Missed, W7Prep, W9a, W9d, W10, W12,
};
use profile_core::types::{AREAS, MetricId, SeriesKey};

#[test]
fn bands_and_constants_match_ts_m2() {
    let r = [899., 900., 1299., 1300., 1700., 1999., 2000., 2299., 2300.].map(band_of);
    assert_eq!(r, [B0, B1, B1, B2, B3, B3, B4, B4, B5]);
    assert_eq!(
        to_lichess_equivalent(Lichess, Blitz, Some(1500)),
        Some(1500)
    );
    assert_eq!(
        to_lichess_equivalent(Chesscom, Rapid, Some(1000)),
        Some(1350)
    );
    assert_eq!(to_lichess_equivalent(Chesscom, Rapid, None), None);
    assert_eq!(rating_target(Some(1450)), Some(1650));
    assert_eq!(trainability(W1, B0), 1.0);
    assert_eq!(trainability(W10, B0), 0.0);
    assert_eq!(trainability(W7Prep, B5), 1.0);
    assert_eq!(trainability(W3Deep, B0), 0.2);
    assert_eq!(trainability(W12, B3), 0.0);
    assert_eq!(n0(W1), 300.0);
    assert_eq!(min_evidence(W1), 1000.0);
    assert_eq!(min_evidence(W4Missed), 40.0);
    assert!(!RANKABLE.contains(&W12) && !RANKABLE.contains(&W9d));
    assert_eq!(AREAS.len(), 7);
}

#[test]
fn key_strings_round_trip() {
    for k in WeaknessKey::ALL {
        assert_eq!(k.as_str().parse::<WeaknessKey>().unwrap(), k);
    }
    assert_eq!(W3Shallow.contract_id(), ("W3", Some("shallow")));
    assert_eq!(W9a.contract_id(), ("W9a", None));
    assert_eq!(
        "motif.missed:fork".parse::<SeriesKey>().unwrap(),
        SeriesKey::MotifMissed(Fork)
    );
    assert_eq!(
        MetricId::MotifMissRate(BackRankMate).to_string(),
        "motif_miss_rate:backRankMate"
    );
    assert_eq!(MateIn4Plus.contract_tag(), "mate_in_4_plus");
}
