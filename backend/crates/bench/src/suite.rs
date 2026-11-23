//! The suites: which synthetic players are generated, with how many games, at which checkpoints.

use std::sync::Arc;

use chess_core::Color;
use profile_core::stats::rng::Rng;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::macros::datetime;

use crate::flaws::{Flaw, flaw};
use crate::generate::GameSpec;
use crate::policy::uniform;

/// The benchmark's clock: every build and every CoachingInput is computed at the time of the
/// player's latest game in the checkpoint (see `harness`), and the games end here.
pub const ANCHOR: OffsetDateTime = datetime!(2026-11-01 12:00:00 UTC);
/// Two games a day, 12 hours apart.
pub const GAME_SPACING_HOURS: i64 = 12;
/// Every synthetic player and opponent is rated around this (the peer band is +-150).
pub const RATING: i32 = 1500;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PlayerDef {
    pub name: String,
    /// `(flaw id, rate)`; `None` is a clean control.
    pub flaw: Option<(String, f64)>,
    pub games: usize,
    pub seed: u32,
}

impl PlayerDef {
    pub fn flaw(&self) -> Option<Arc<dyn Flaw>> {
        self.flaw.as_ref().and_then(|(id, rate)| flaw(id, *rate))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Suite {
    pub name: String,
    pub players: Vec<PlayerDef>,
    /// Game counts at which the profile is built and scored (data efficiency); the last is the
    /// final result.
    pub checkpoints: Vec<usize>,
    /// Stockfish nodes per search.
    pub nodes: u64,
    /// Distinct opponents per player (the peer benchmark needs 10 with evidence).
    pub opponents: usize,
    /// Rebuild each player from 90% of its games for the stability score.
    pub perturb: bool,
}

fn player(name: &str, flaw: Option<(&str, f64)>, games: usize, seed: u32) -> PlayerDef {
    PlayerDef {
        name: name.to_owned(),
        flaw: flaw.map(|(id, r)| (id.to_owned(), r)),
        games,
        seed,
    }
}

/// The CI suite: one player per main flaw type and one clean control, 80 games each.
pub fn smoke() -> Suite {
    let g = 80;
    Suite {
        name: "smoke".to_owned(),
        players: vec![
            player("bench-hang", Some(("hang", 0.05)), g, 11),
            player("bench-missmate", Some(("miss_mate", 0.5)), g, 12),
            player("bench-opening", Some(("opening_blunder", 0.3)), g, 13),
            player("bench-lowclock", Some(("low_clock_blunder", 0.25)), g, 14),
            player("bench-noconvert", Some(("no_convert", 0.5)), g, 15),
            player("bench-clean", None, g, 16),
        ],
        checkpoints: vec![30, 45, 60, 80],
        nodes: 12_000,
        opponents: 16,
        perturb: true,
    }
}

/// The weekly suite: two seeds of every smoke player at 160 games, a second clean control, and
/// a sweep of injection rates for the detection curves.
pub fn full() -> Suite {
    let g = 160;
    let mut players = Vec::new();
    let mut seed = 100;
    let mut add = |name: String, flaw: Option<(&str, f64)>| {
        seed += 1;
        players.push(player(&name, flaw, g, seed));
    };
    let sweep: [(&str, &[f64]); 5] = [
        ("hang", &[0.02, 0.05, 0.1]),
        ("miss_mate", &[0.25, 0.5, 0.75]),
        ("opening_blunder", &[0.15, 0.3, 0.5]),
        ("low_clock_blunder", &[0.1, 0.25, 0.4]),
        ("no_convert", &[0.5]),
    ];
    for (id, rates) in sweep {
        for r in rates {
            add(
                format!("bench-{}-{}", id.replace('_', ""), (r * 100.0).round()),
                Some((id, *r)),
            );
        }
    }
    add("bench-clean-a".to_owned(), None);
    add("bench-clean-b".to_owned(), None);
    Suite {
        name: "full".to_owned(),
        players,
        checkpoints: vec![30, 45, 60, 80, 120, 160],
        nodes: 12_000,
        opponents: 20,
        perturb: true,
    }
}

pub fn by_name(name: &str) -> Option<Suite> {
    match name {
        "smoke" => Some(smoke()),
        "full" => Some(full()),
        _ => None,
    }
}

/// A suite read from JSON (the [`Suite`] shape), for an experiment that needs players the named
/// suites do not have (`cli bench run --suite-file`, ADR 0106). It is checked before anything
/// runs: at least one player, unique names, known flaws with a rate in `(0, 1]`, at least one
/// game each, ascending checkpoints of at least one game, and positive nodes and opponents.
pub fn from_json(json: &str) -> anyhow::Result<Suite> {
    let s: Suite = serde_json::from_str(json)?;
    anyhow::ensure!(!s.name.trim().is_empty(), "the suite has no name");
    anyhow::ensure!(!s.players.is_empty(), "the suite has no players");
    anyhow::ensure!(s.nodes > 0, "nodes must be positive");
    anyhow::ensure!(s.opponents > 0, "opponents must be positive");
    anyhow::ensure!(
        !s.checkpoints.is_empty() && s.checkpoints[0] > 0,
        "checkpoints must be positive game counts"
    );
    anyhow::ensure!(
        s.checkpoints.windows(2).all(|w| w[0] < w[1]),
        "checkpoints must be strictly ascending"
    );
    let mut names = std::collections::BTreeSet::new();
    for p in &s.players {
        anyhow::ensure!(
            names.insert(p.name.as_str()),
            "player {} is listed twice",
            p.name
        );
        anyhow::ensure!(p.games > 0, "player {} has no games", p.name);
        if let Some((id, rate)) = &p.flaw {
            anyhow::ensure!(
                *rate > 0.0 && *rate <= 1.0,
                "player {}: rate {rate} is not in (0, 1]",
                p.name
            );
            anyhow::ensure!(p.flaw().is_some(), "player {}: no flaw {id:?}", p.name);
        }
    }
    Ok(s)
}

/// The games of a player: colours alternate, opponents are drawn from `opponents` fixed
/// identities rated `RATING +- 100`, games are [`GAME_SPACING_HOURS`] apart and the last one is
/// at [`ANCHOR`] minus one spacing. Seeds derive from the player's seed.
pub fn game_specs(p: &PlayerDef, opponents: usize) -> Vec<GameSpec> {
    let mut rng = Rng::new(p.seed.wrapping_mul(2_654_435_761));
    let ratings: Vec<i32> = (0..opponents.max(1))
        .map(|_| RATING + uniform(&mut rng, -100.0, 100.0).round() as i32)
        .collect();
    let first = ANCHOR - time::Duration::hours(GAME_SPACING_HOURS * p.games as i64);
    (0..p.games)
        .map(|i| {
            let o = (rng.next_f64() * ratings.len() as f64) as usize % ratings.len();
            let minutes = (rng.next_f64() * 120.0) as i64;
            GameSpec {
                platform_game_id: format!("{}-{i:04}", p.name),
                player_name: p.name.clone(),
                player_color: if i % 2 == 0 {
                    Color::White
                } else {
                    Color::Black
                },
                player_rating: RATING,
                opponent_name: format!("{}-opp{o:02}", p.name),
                opponent_rating: ratings[o],
                played_at: first
                    + time::Duration::hours(GAME_SPACING_HOURS * i as i64)
                    + time::Duration::minutes(minutes),
                seed: p.seed.wrapping_mul(100_003).wrapping_add(i as u32),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_has_one_clean_control_and_every_flaw() {
        let s = smoke();
        assert_eq!(s.players.len(), 6);
        assert_eq!(s.players.iter().filter(|p| p.flaw.is_none()).count(), 1);
        for p in &s.players {
            if p.flaw.is_some() {
                assert!(p.flaw().is_some(), "{}", p.name);
            }
        }
        assert_eq!(s.checkpoints.last(), Some(&80));
    }

    #[test]
    fn game_specs_are_deterministic_and_spread_over_opponents() {
        let p = &smoke().players[0];
        let a = game_specs(p, 16);
        let b = game_specs(p, 16);
        assert_eq!(a.len(), 80);
        assert_eq!(
            a.iter()
                .map(|g| (&g.opponent_name, g.seed))
                .collect::<Vec<_>>(),
            b.iter()
                .map(|g| (&g.opponent_name, g.seed))
                .collect::<Vec<_>>()
        );
        let distinct: std::collections::HashSet<_> = a.iter().map(|g| &g.opponent_name).collect();
        assert!(distinct.len() >= 12, "{}", distinct.len());
        assert!(a.iter().all(|g| g.played_at < ANCHOR));
        assert!(a.windows(2).all(|w| w[0].played_at < w[1].played_at));
        assert!(a.iter().all(|g| (g.opponent_rating - RATING).abs() <= 100));
    }

    #[test]
    fn a_suite_file_round_trips_and_is_checked() {
        let json = serde_json::to_string(&smoke()).unwrap();
        assert_eq!(from_json(&json).unwrap(), smoke());
        let mut bad = smoke();
        bad.players[1].name = bad.players[0].name.clone();
        let e = from_json(&serde_json::to_string(&bad).unwrap()).unwrap_err();
        assert!(e.to_string().contains("twice"), "{e}");
        let mut bad = smoke();
        bad.players[0].flaw = Some(("juggle".to_owned(), 0.1));
        assert!(from_json(&serde_json::to_string(&bad).unwrap()).is_err());
        let mut bad = smoke();
        bad.players[0].flaw = Some(("hang".to_owned(), 0.0));
        assert!(from_json(&serde_json::to_string(&bad).unwrap()).is_err());
        let mut bad = smoke();
        bad.checkpoints = vec![45, 30];
        assert!(from_json(&serde_json::to_string(&bad).unwrap()).is_err());
        let mut bad = smoke();
        bad.players.clear();
        assert!(from_json(&serde_json::to_string(&bad).unwrap()).is_err());
        assert!(from_json("{}").is_err());
    }

    #[test]
    fn full_sweeps_rates() {
        let f = full();
        assert!(f.players.len() > smoke().players.len());
        assert!(f.players.iter().any(|p| p.name == "bench-missmate-75"));
    }
}
