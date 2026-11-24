//! The prior rule of the weakness estimate, and the variants the lab compares (EXP-0006).
//!
//! Production runs [`PriorRule::CHAMPION`], the rule of ADR-R2-5: the recency-weighted counts
//! `(sum w k, sum w n)` and the band prior `{pooled_rate, n0[key]}`. A tool can select another rule
//! for the whole process with [`set`] (the `cli` does it for `ALGO_VARIANT`, in `lab` and `bench`
//! only); nothing in the worker or the web calls it, so their output never depends on it.
//! [`crate::stats::priority::estimate_weakness_with`] takes the rule explicitly, which is what
//! the tests use.
//!
//! The variants (`parse`):
//! - `champion`: the production rule;
//! - `challenger`: the rule EXP-0006 registered, frozen on its branch ([`PriorRule::CHALLENGER`]);
//! - `dev:<options>`: a rule for development runs only, `games=G` (the prior is worth at most `G`
//!   of the player's own games), `ess` (effective counts) and `disp` (the dispersion of the
//!   per-game counts), comma-separated, e.g. `dev:games=2,ess`.

use std::sync::RwLock;

/// Largest dispersion factor the rule divides the effective counts by. A game-to-game spread ten
/// times the binomial one is far past anything seen (design effects of 1.3 to 2,
/// `docs/research/improvement-engine.md` section 5.2); the cap keeps one wild series from making
/// an interval useless.
pub const MAX_DISPERSION: f64 = 10.0;

/// How the estimate counts the data and weighs the band prior.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PriorRule {
    /// The prior's strength is at most this many of the player's own games, counted as the data
    /// are: `n0 = min(n0[key], games * n / games_in_window)`. `None`: `n0[key]` alone.
    pub prior_games: Option<f64>,
    /// Count the recency-weighted data at their effective sample size (Kish): the weighted rate
    /// `p = sum w k / sum w n` over `n_eff = (sum w n)^2 / sum w^2 n` chances, instead of
    /// treating the weighted sums as real counts.
    pub effective_counts: bool,
    /// Divide `n_eff` by the Pearson dispersion of the per-game counts, clamped to
    /// `[1, MAX_DISPERSION]`. Only with `effective_counts`.
    pub dispersion: bool,
}

impl PriorRule {
    /// The production rule (ADR-R2-5).
    pub const CHAMPION: PriorRule = PriorRule {
        prior_games: None,
        effective_counts: false,
        dispersion: false,
    };

    /// The challenger EXP-0006 registered (`lab/experiments/EXP-0006/preregistration.md`), chosen
    /// on development seeds only (`dev:games=2,ess,disp`, `lab/experiments/EXP-0006/dev/`). It does
    /// not change after the registration.
    pub const CHALLENGER: PriorRule = PriorRule {
        prior_games: Some(2.0),
        effective_counts: true,
        dispersion: true,
    };
}

static CURRENT: RwLock<PriorRule> = RwLock::new(PriorRule::CHAMPION);

/// The rule every [`crate::stats::priority::estimate_weakness`] in this process uses.
pub fn current() -> PriorRule {
    match CURRENT.read() {
        Ok(r) => *r,
        Err(poisoned) => *poisoned.into_inner(),
    }
}

/// Selects the rule for the whole process (a lab or bench tool only).
pub fn set(rule: PriorRule) {
    match CURRENT.write() {
        Ok(mut r) => *r = rule,
        Err(poisoned) => *poisoned.into_inner() = rule,
    }
}

/// The rule of a variant name: `champion`, `challenger` or `dev:<options>`.
pub fn parse(name: &str) -> Result<PriorRule, String> {
    let name = name.trim();
    match name {
        "champion" => return Ok(PriorRule::CHAMPION),
        "challenger" => return Ok(PriorRule::CHALLENGER),
        _ => {}
    }
    let Some(opts) = name.strip_prefix("dev:") else {
        return Err(format!(
            "unknown algorithm variant {name:?}: champion, challenger or dev:<options>"
        ));
    };
    let mut rule = PriorRule::CHAMPION;
    for opt in opts.split(',').map(str::trim).filter(|o| !o.is_empty()) {
        match opt.split_once('=') {
            Some(("games", g)) => match g.parse::<f64>() {
                Ok(g) if g.is_finite() && g > 0.0 => rule.prior_games = Some(g),
                _ => return Err(format!("games must be a positive number, not {g:?}")),
            },
            None if opt == "ess" => rule.effective_counts = true,
            None if opt == "disp" => rule.dispersion = true,
            _ => {
                return Err(format!(
                    "unknown variant option {opt:?}: games=G, ess or disp"
                ));
            }
        }
    }
    if rule.dispersion && !rule.effective_counts {
        return Err("disp needs ess".to_owned());
    }
    Ok(rule)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_parse() {
        assert_eq!(parse("champion"), Ok(PriorRule::CHAMPION));
        assert_eq!(parse("challenger"), Ok(PriorRule::CHALLENGER));
        assert_eq!(
            parse("dev:games=2,ess,disp"),
            Ok(PriorRule {
                prior_games: Some(2.0),
                effective_counts: true,
                dispersion: true,
            })
        );
        assert_eq!(parse("dev:"), Ok(PriorRule::CHAMPION));
        assert!(parse("dev:games=0").is_err());
        assert!(parse("dev:games=x").is_err());
        assert!(parse("dev:disp").is_err());
        assert!(parse("dev:fast").is_err());
        assert!(parse("lighter").is_err());
    }

    #[test]
    fn production_is_the_champion() {
        // No test in this crate calls `set`, so the process default is visible here.
        assert_eq!(current(), PriorRule::CHAMPION);
    }
}
