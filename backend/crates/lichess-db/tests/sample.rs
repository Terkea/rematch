//! Deterministic sampling of dump games.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use chess_core::TimeClass;
use futures::StreamExt;
use lichess_db::download::DumpError;
use lichess_db::headers::parse_dump_headers;
use lichess_db::sample::{
    SampleFilter, SampledGame, Sampler, filter_sample, fnv1a32, keep, sample_fraction,
};
use lichess_db::split::split_games;

fn games() -> Vec<String> {
    split_games(testkit::fixture("lichess-db/sample.pgn").lines())
}

async fn run(filter: SampleFilter) -> Vec<SampledGame> {
    let mut source = futures::stream::iter(games().into_iter().map(Ok::<String, DumpError>));
    filter_sample(&mut source, filter)
        .map(|r| r.expect("sampled game"))
        .collect()
        .await
}

fn ids(kept: &[SampledGame]) -> Vec<&str> {
    kept.iter()
        .map(|k| k.header.lichess_game_id.as_str())
        .collect()
}

/// Independent FNV-1a reference (offset basis 2166136261, prime 16777619).
fn fnv_reference(s: &str) -> u32 {
    s.bytes().fold(2_166_136_261u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(16_777_619)
    })
}

#[test]
fn fnv1a32_matches_the_published_test_vectors() {
    assert_eq!(fnv1a32(b""), 0x811c_9dc5);
    assert_eq!(fnv1a32(b"a"), 0xe40c_292c);
    assert_eq!(fnv1a32(b"foobar"), 0xbf9c_f968);
}

#[test]
fn sample_fraction_hashes_site_then_decimal_seed() {
    let site = "https://lichess.org/Fx1aB2cD";
    let want = f64::from(fnv_reference(&format!("{site}7"))) / 4_294_967_296.0;
    assert_eq!(sample_fraction(site, 7), want);
    assert!((0.0..1.0).contains(&sample_fraction(site, u32::MAX)));
}

#[tokio::test]
async fn rate_one_keeps_every_fixture_game_with_its_time_class() {
    assert_eq!(SampleFilter::default().sample_rate, 1.0);
    let kept = run(SampleFilter::default()).await;
    let classes: Vec<TimeClass> = kept.iter().map(|k| k.time_class).collect();
    assert_eq!(
        classes,
        [
            TimeClass::Blitz,
            TimeClass::Rapid,
            TimeClass::Bullet,
            TimeClass::Blitz,
            TimeClass::Classical
        ]
    );
    assert_eq!(kept[0].pgn, games()[0]);
}

#[tokio::test]
async fn require_clock_drops_the_clockless_game() {
    let kept = run(SampleFilter {
        require_clock: true,
        ..SampleFilter::default()
    })
    .await;
    assert_eq!(ids(&kept), ["Fx1aB2cD", "Gy2bC3dE", "Hz3cD4eF", "Lm5eF6gH"]);
}

#[tokio::test]
async fn rate_zero_keeps_none() {
    let kept = run(SampleFilter {
        sample_rate: 0.0,
        ..SampleFilter::default()
    })
    .await;
    assert!(kept.is_empty());
}

#[tokio::test]
async fn seed_7_gives_the_same_subset_twice() {
    let filter = SampleFilter {
        sample_rate: 0.5,
        seed: 7,
        ..SampleFilter::default()
    };
    let a = run(filter.clone()).await;
    let b = run(filter).await;
    assert_eq!(ids(&a), ids(&b));
    let want: Vec<String> = games()
        .iter()
        .map(|g| parse_dump_headers(g).unwrap())
        .filter(|h| f64::from(fnv_reference(&format!("{}7", h.site))) / 4_294_967_296.0 < 0.5)
        .map(|h| h.lichess_game_id)
        .collect();
    assert_eq!(ids(&a), want);
    assert!(
        !a.is_empty() && a.len() < 5,
        "seed 7 at rate 0.5 should keep a strict subset: {:?}",
        ids(&a)
    );
}

#[tokio::test]
async fn max_games_2_stops_after_2() {
    let pulled = Arc::new(AtomicUsize::new(0));
    let counter = pulled.clone();
    let mut source = futures::stream::iter(games()).map(move |g| {
        counter.fetch_add(1, Ordering::Relaxed);
        Ok::<String, DumpError>(g)
    });
    let kept: Vec<_> = filter_sample(
        &mut source,
        SampleFilter {
            max_games: Some(2),
            ..SampleFilter::default()
        },
    )
    .collect()
    .await;
    assert_eq!(kept.len(), 2);
    // The sampler stopped pulling: the caller gets the rest back (to drain it, R-9).
    assert_eq!(pulled.load(Ordering::Relaxed), 2);
    assert_eq!(source.count().await, 3);
}

#[test]
fn keep_skips_correspondence_abandoned_rules_infraction_and_missing_elo() {
    let base = games()[0].clone();
    let f = SampleFilter::default();
    let keeps = |pgn: &str| keep(&parse_dump_headers(pgn).unwrap(), &f);
    assert_eq!(keeps(&base), Some(TimeClass::Blitz));
    for (from, to) in [
        ("[TimeControl \"180+2\"]", "[TimeControl \"-\"]"),
        ("[Termination \"Normal\"]", "[Termination \"Abandoned\"]"),
        (
            "[Termination \"Normal\"]",
            "[Termination \"Rules infraction\"]",
        ),
        ("[WhiteElo \"1500\"]", "[WhiteElo \"?\"]"),
        ("[BlackElo \"1480\"]\n", ""),
    ] {
        let g = base.replace(from, to);
        assert_ne!(g, base);
        assert_eq!(keeps(&g), None, "{to:?} must be skipped");
    }
    let g = base.replace("[Termination \"Normal\"]", "[Termination \"Time forfeit\"]");
    assert_eq!(keeps(&g), Some(TimeClass::Blitz));
}

#[tokio::test]
async fn time_classes_filter_keeps_only_listed_classes() {
    let kept = run(SampleFilter {
        time_classes: Some(vec![TimeClass::Blitz]),
        ..SampleFilter::default()
    })
    .await;
    assert_eq!(ids(&kept), ["Fx1aB2cD", "Jk4dE5fG"]);
}

#[test]
fn sampler_counts_seen_kept_and_malformed() {
    let mut s = Sampler::new(SampleFilter {
        max_games: Some(3),
        require_clock: true,
        ..SampleFilter::default()
    });
    assert!(
        s.offer("[Event \"broken\"]\n\n1. e4 *\n".to_owned())
            .is_none()
    );
    for g in games() {
        if s.is_full() {
            break;
        }
        s.offer(g);
    }
    let st = s.stats();
    // The malformed game and the first three fixture games were offered; the third filled it.
    assert_eq!((st.seen, st.kept, st.malformed), (4, 3, 1));
    assert!(s.is_full());
}
