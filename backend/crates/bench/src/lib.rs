//! The pipeline benchmark, Tier 1 (ADR 0100, `docs/benchmark.md`): synthetic players with
//! injected flaws of known rates, run through the real analysis, enrichment, profile engine and
//! CoachingInput, then scored on diagnosis, calibration, plan targeting, data efficiency and
//! stability.

pub mod flaws;
pub mod generate;
pub mod harness;
pub mod lab;
pub mod memo;
pub mod policy;
pub mod results;
pub mod run;
pub mod suite;

/// The prior rules a run can select (`ALGO_VARIANT`, EXP-0006), re-exported for the `cli`.
pub use profile_core::stats::variant;

/// The variant named by `ALGO_VARIANT`, or `None` when it is unset or empty.
pub fn variant_from_env(
    value: Option<String>,
) -> Result<Option<(String, variant::PriorRule)>, String> {
    match value.map(|v| v.trim().to_owned()) {
        None => Ok(None),
        Some(v) if v.is_empty() => Ok(None),
        Some(v) => variant::parse(&v).map(|rule| Some((v, rule))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn algo_variant_from_the_environment() {
        assert_eq!(variant_from_env(None), Ok(None));
        assert_eq!(variant_from_env(Some("  ".into())), Ok(None));
        assert_eq!(
            variant_from_env(Some("challenger".into())),
            Ok(Some((
                "challenger".to_owned(),
                variant::PriorRule::CHALLENGER
            )))
        );
        assert!(variant_from_env(Some("bogus".into())).is_err());
    }
}
