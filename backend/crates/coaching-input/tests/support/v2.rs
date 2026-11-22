//! A v2 colour split for the contract tests (ADR 0084): White with enough games, a compared
//! and an uncompared weakness, and Black below the minimum (every number null).

use coaching_input::{
    AreaId, Benchmark, BenchmarkSource, ByColor, ColorMetric, ColorSummary, ColorWeakness,
    MetricUnit,
};

pub fn metric(value: Option<(f64, f64, f64)>, unit: MetricUnit, n: u32, min: u32) -> ColorMetric {
    let sufficient = value.is_some();
    ColorMetric {
        value: value.map(|v| v.0),
        unit,
        n,
        low: value.map(|v| v.1),
        high: value.map(|v| v.2),
        sufficient,
        need_more: (!sufficient).then(|| min.saturating_sub(n).max(1)),
    }
}

pub fn white() -> ColorSummary {
    ColorSummary {
        games: 42,
        sufficient: true,
        need_more: None,
        wins: 20,
        draws: 4,
        losses: 18,
        score: metric(Some((0.524, 0.44, 0.6)), MetricUnit::Rate, 42, 10),
        accuracy: metric(Some((78.4, 76.9, 79.8)), MetricUnit::Pct, 40, 20),
        blunders_per_game: metric(Some((0.9, 0.7, 1.1)), MetricUnit::PerGame, 40, 20),
        weaknesses: vec![
            ColorWeakness {
                series_key: "W4.missed".into(),
                id: "W4".into(),
                variant: Some("missed".into()),
                label: "Missed mates".into(),
                area: AreaId::Tactics,
                rate: 0.14,
                low: 0.09,
                high: 0.2,
                events: 9,
                opportunities: 61,
                peer_rate: Some(0.096),
                peer_percentile: Some(28.0),
                peer_players: Some(14),
            },
            ColorWeakness {
                series_key: "W1".into(),
                id: "W1".into(),
                variant: None,
                label: "Hanging pieces".into(),
                area: AreaId::Tactics,
                rate: 0.05,
                low: 0.03,
                high: 0.07,
                events: 12,
                opportunities: 240,
                peer_rate: None,
                peer_percentile: None,
                peer_players: None,
            },
        ],
    }
}

pub fn black() -> ColorSummary {
    ColorSummary {
        games: 7,
        sufficient: false,
        need_more: Some(3),
        wins: 3,
        draws: 0,
        losses: 4,
        score: metric(None, MetricUnit::Rate, 7, 10),
        accuracy: metric(None, MetricUnit::Pct, 7, 20),
        blunders_per_game: metric(None, MetricUnit::PerGame, 7, 20),
        weaknesses: Vec::new(),
    }
}

pub fn by_color() -> ByColor {
    ByColor {
        white: white(),
        black: black(),
    }
}

pub fn benchmark() -> Benchmark {
    Benchmark {
        source: BenchmarkSource::Peers,
        rating_low: Some(675),
        rating_high: Some(975),
        players: Some(25),
        games: Some(61),
    }
}
