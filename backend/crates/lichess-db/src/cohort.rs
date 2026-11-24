//! Following the same anonymised players across monthly dumps (EXP-0009, ADR 0114).
//!
//! The band reference (ADR-R2-11) samples players per band × time class cell and keeps 10 games
//! each, so a player who climbs leaves the cell. A longitudinal study needs the selection keyed on
//! the player's pseudonym alone: `h = sha256(salt ‖ lower(name))`
//! ([`crate::reference_select::player_hash`]), and a side is followed when
//! `hash_fraction(h) < player_rate`. With the same `--salt` every month, the same players are
//! followed in every month; the fraction does not depend on the player's games, ratings or
//! results, so the selection cannot pick players by how they did.
//!
//! What a month gives, for every followed player with a rapid game:
//! - `players.jsonl`: the number of rapid games, the number of those that carry Lichess's
//!   `[%eval]`, the number of bullet, blitz and classical games, and the first and last
//!   [`ENDS`] rapid games by time (`[utc, game id, rating before, rating after]`), sorted by the
//!   pseudonym;
//! - with `--games`, `games.jsonl`: every rapid game with `[%eval]` that a followed player played,
//!   in dump order, its PGN with the `White` and `Black` tags replaced by `?`
//!   ([`crate::reference_select::redact_pgn`]) and each followed side's pseudonym.
//!
//! No name is written. The pseudonym is the first 16 bytes of `h` in hex; the Lichess game id is
//! kept, as in the band reference, so the public game can be found on Lichess. The filter is the
//! band reference's ([`crate::reference_select::reference_filter`]): rated games with a real clock,
//! both ratings and `[%clk]`, no BOT, not abandoned and no rules infraction.

use std::collections::HashMap;
use std::io::Write;

use chess_core::TimeClass;
use serde::Serialize;

use crate::reference_select::{hash_fraction, player_hash, redact_pgn, reference_filter};
use crate::sample::{SampleStats, SampledGame, Sampler};

/// Games kept at each end of a player's month, for the rating at its start and its end.
pub const ENDS: usize = 5;

/// The first 16 bytes of a player's hash: the pseudonym written out.
pub type Pseudonym = [u8; 16];

/// One rapid game at an end of a player's month: `(utc seconds, game id, before, after)`.
pub type Point = (i64, String, i32, i32);

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Followed {
    pub rapid_games: u32,
    pub eval_games: u32,
    pub bullet: u32,
    pub blitz: u32,
    pub classical: u32,
    /// The first [`ENDS`] rapid games by `(utc, id)`, ascending.
    pub first: Vec<Point>,
    /// The last [`ENDS`] rapid games by `(utc, id)`, ascending.
    pub last: Vec<Point>,
}

fn key(p: &Point) -> (i64, &str) {
    (p.0, p.1.as_str())
}

impl Followed {
    fn add_rapid(&mut self, p: Point) {
        self.rapid_games += 1;
        let at = self.first.partition_point(|q| key(q) < key(&p));
        if at < ENDS {
            self.first.insert(at, p.clone());
            self.first.truncate(ENDS);
        }
        let at = self.last.partition_point(|q| key(q) < key(&p));
        self.last.insert(at, p);
        if self.last.len() > ENDS {
            self.last.remove(0);
        }
    }
}

/// One line of `games.jsonl`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EvalGame {
    pub id: String,
    pub utc: i64,
    pub base_seconds: u32,
    pub increment_seconds: u32,
    /// The followed side's pseudonym, or null for a side that is not followed.
    pub white: Option<String>,
    pub black: Option<String>,
    pub white_elo: Option<i32>,
    pub black_elo: Option<i32>,
    pub white_rating_diff: Option<i32>,
    pub black_rating_diff: Option<i32>,
    pub result: String,
    pub termination: Option<String>,
    pub pgn: String,
}

/// One line of `players.jsonl`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlayerLine {
    pub h: String,
    pub rapid_games: u32,
    pub eval_games: u32,
    pub bullet: u32,
    pub blitz: u32,
    pub classical: u32,
    pub first: Vec<Point>,
    pub last: Vec<Point>,
}

/// The month's followed players, built game by game in dump order.
pub struct CohortSink {
    salt: [u8; 32],
    player_rate: f64,
    sampler: Sampler,
    players: HashMap<Pseudonym, Followed>,
}

impl CohortSink {
    pub fn new(salt: [u8; 32], player_rate: f64) -> Self {
        Self {
            salt,
            player_rate,
            sampler: Sampler::new(reference_filter()),
            players: HashMap::new(),
        }
    }

    pub fn stats(&self) -> SampleStats {
        self.sampler.stats()
    }

    fn follow(&self, name: &str) -> Option<Pseudonym> {
        let h = player_hash(&self.salt, name);
        (hash_fraction(&h) < self.player_rate).then(|| {
            let mut p = [0u8; 16];
            p.copy_from_slice(&h[..16]);
            p
        })
    }

    /// Counts one dump game; returns its `games.jsonl` line when it is a rapid game with
    /// `[%eval]` and at least one followed side.
    pub fn offer(&mut self, pgn: String) -> Option<EvalGame> {
        let SampledGame {
            pgn,
            header: h,
            time_class,
        } = self.sampler.offer(pgn)?;
        let sides = [
            (self.follow(&h.white), h.white_elo, h.white_rating_diff),
            (self.follow(&h.black), h.black_elo, h.black_rating_diff),
        ];
        if sides.iter().all(|(p, _, _)| p.is_none()) {
            return None;
        }
        let utc = h.utc.unix_timestamp();
        for (p, elo, diff) in &sides {
            let Some(p) = p else { continue };
            let f = self.players.entry(*p).or_default();
            match time_class {
                TimeClass::Rapid => {
                    let before = elo.unwrap_or_default();
                    f.add_rapid((
                        utc,
                        h.lichess_game_id.clone(),
                        before,
                        before + diff.unwrap_or(0),
                    ));
                    if h.has_eval {
                        f.eval_games += 1;
                    }
                }
                TimeClass::Bullet => f.bullet += 1,
                TimeClass::Blitz => f.blitz += 1,
                _ => f.classical += 1,
            }
        }
        (time_class == TimeClass::Rapid && h.has_eval).then(|| EvalGame {
            id: h.lichess_game_id.clone(),
            utc,
            base_seconds: h.base_seconds.unwrap_or_default(),
            increment_seconds: h.increment_seconds.unwrap_or_default(),
            white: sides[0].0.map(hex::encode),
            black: sides[1].0.map(hex::encode),
            white_elo: h.white_elo,
            black_elo: h.black_elo,
            white_rating_diff: h.white_rating_diff,
            black_rating_diff: h.black_rating_diff,
            result: h.result.clone(),
            termination: h.termination.clone(),
            pgn: redact_pgn(&pgn),
        })
    }

    /// The followed players with a rapid game, sorted by pseudonym.
    pub fn players(&self) -> Vec<PlayerLine> {
        let mut out: Vec<PlayerLine> = self
            .players
            .iter()
            .filter(|(_, f)| f.rapid_games > 0)
            .map(|(p, f)| PlayerLine {
                h: hex::encode(p),
                rapid_games: f.rapid_games,
                eval_games: f.eval_games,
                bullet: f.bullet,
                blitz: f.blitz,
                classical: f.classical,
                first: f.first.clone(),
                last: f.last.clone(),
            })
            .collect();
        out.sort_by(|a, b| a.h.cmp(&b.h));
        out
    }
}

/// One JSON value per line.
pub fn write_line<W: Write, T: Serialize>(w: &mut W, v: &T) -> std::io::Result<()> {
    serde_json::to_writer(&mut *w, v)?;
    w.write_all(b"\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pgn(id: &str, white: &str, black: &str, tc: &str, utc: &str, eval: bool) -> String {
        let ev = if eval { " [%eval 0.2]" } else { "" };
        format!(
            "[Event \"Rated game\"]\n[Site \"https://lichess.org/{id}\"]\n[Date \"2026.08.01\"]\n\
             [White \"{white}\"]\n[Black \"{black}\"]\n[Result \"1-0\"]\n[UTCDate \"2026.08.01\"]\n\
             [UTCTime \"{utc}\"]\n[WhiteElo \"1500\"]\n[BlackElo \"1480\"]\n[WhiteRatingDiff \"+6\"]\n\
             [BlackRatingDiff \"-6\"]\n[TimeControl \"{tc}\"]\n[Termination \"Normal\"]\n\n\
             1. e4 {{{ev} [%clk 0:10:00] }} e5 {{ [%clk 0:10:00] }} 1-0\n"
        )
    }

    #[test]
    fn the_same_salt_follows_the_same_players_and_writes_no_name() {
        let salt = [7u8; 32];
        let mut a = CohortSink::new(salt, 1.0);
        let line = a
            .offer(pgn("g1", "Alice", "Bob", "600+0", "10:00:00", true))
            .expect("a rapid game with evals");
        assert!(!line.pgn.contains("Alice") && !line.pgn.contains("Bob"));
        assert_eq!(line.white.as_deref().map(str::len), Some(32));
        assert!(
            a.offer(pgn("g2", "alice", "Carol", "180+0", "11:00:00", true))
                .is_none()
        );
        let players = a.players();
        let alice = players
            .iter()
            .find(|p| Some(&p.h) == line.white.as_ref())
            .expect("alice");
        // Names are case-folded, so "Alice" and "alice" are one player.
        assert_eq!(
            (alice.rapid_games, alice.eval_games, alice.blitz),
            (1, 1, 1)
        );
        assert_eq!(alice.first[0].2, 1500);
        assert_eq!(alice.first[0].3, 1506);
        let mut b = CohortSink::new(salt, 1.0);
        b.offer(pgn("g9", "ALICE", "Dan", "600+0", "09:00:00", false));
        assert!(b.players().iter().any(|p| p.h == alice.h));
        // Carol played only blitz: no rapid game, no line.
        assert_eq!(players.len(), 2);
    }

    #[test]
    fn the_ends_are_the_first_and_last_games_by_time_whatever_the_dump_order() {
        let mut s = CohortSink::new([1u8; 32], 1.0);
        for (i, t) in [
            "12:00:00", "10:00:00", "11:00:00", "09:00:00", "13:00:00", "08:00:00", "14:00:00",
        ]
        .iter()
        .enumerate()
        {
            s.offer(pgn(&format!("g{i}"), "eve", "frank", "600+0", t, false));
        }
        let eve = &s.players()[0];
        assert_eq!(eve.rapid_games, 7);
        let ids = |v: &[Point]| v.iter().map(|p| p.1.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&eve.first), ["g5", "g3", "g1", "g2", "g0"]);
        assert_eq!(ids(&eve.last), ["g1", "g2", "g0", "g4", "g6"]);
    }

    #[test]
    fn a_player_rate_of_zero_follows_nobody() {
        let mut s = CohortSink::new([2u8; 32], 0.0);
        assert!(
            s.offer(pgn("g1", "a", "b", "600+0", "10:00:00", true))
                .is_none()
        );
        assert!(s.players().is_empty());
    }
}
