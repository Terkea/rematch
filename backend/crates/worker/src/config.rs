//! The worker's environment.
//!
//! `CONTACT_EMAIL` is not read: no outbound request carries an email address, and the platform
//! clients send the fixed project User-Agent (ADR 0043).

use std::path::PathBuf;

use anyhow::{Context, bail};
use scheduler::select::Plan;

/// `Debug` redacts the database password and the Lichess token, so the value can be logged.
#[derive(Clone, PartialEq, Eq)]
pub struct WorkerEnv {
    /// `DATABASE_URL` (required).
    pub database_url: String,
    /// `STOCKFISH_PATH` (required).
    pub stockfish_path: PathBuf,
    /// `LICHESS_TOKEN`: optional, sent to Lichess as a bearer token.
    pub lichess_token: Option<String>,
    /// `LICHESS_SINCE_LOOKBACK_DAYS`: overrides the incremental Lichess look-back (30 days).
    pub lichess_lookback_days: Option<u32>,
    /// `WORKER_POOL_SIZE`: Stockfish processes (and analyses at once); default
    /// `min(available_parallelism - 2, 8)`, at least 1.
    pub pool_size: Option<usize>,
    /// `SYZYGY_PATH`: the 3-4-5 Syzygy tables (compose: `/work/data/syzygy`). Unset: no
    /// tablebase probe; set but without tables: the probe is unavailable (ADR-R2-12).
    pub syzygy_path: Option<PathBuf>,
    /// `SYZYGY_STAGE_DIR`: a local directory (compose: a named volume) that the tables are copied
    /// to at start. The probe then reads the copy, because a slow mount makes every probe re-open
    /// the tables slowly (ADR-R2-25). Unset: the probe reads `SYZYGY_PATH` directly.
    pub syzygy_stage_dir: Option<PathBuf>,
    /// `DATA_DIR`: the git-ignored data directory (compose: `/data`, read-only; ADR-R2-1).
    pub data_dir: Option<PathBuf>,
    /// `MAIA_URL`: the Maia-2 service (`http://maia:8080` in the Maia overlay). Unset: every
    /// estimable move is `pending_maia` until a Maia answers (ADR-R2-13; never estimated).
    pub maia_url: Option<String>,
    /// `WORKER_DEFAULT_PLAN` (`beta`, `free` or `pro`): the plan of a schedule-analysis job that
    /// names none. Default `beta` (the private beta has no plan limit, ADR 0041, T29 ruling).
    pub plan_default: Plan,
}

impl WorkerEnv {
    pub fn from_env() -> anyhow::Result<WorkerEnv> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// [`from_env`](Self::from_env) over any variable source. An empty value counts as unset.
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> anyhow::Result<WorkerEnv> {
        let get = |name: &str| get(name).filter(|v| !v.trim().is_empty());
        let required = |name: &str| get(name).with_context(|| format!("{name} is not set"));
        Ok(WorkerEnv {
            database_url: required("DATABASE_URL")?,
            stockfish_path: PathBuf::from(required("STOCKFISH_PATH")?),
            lichess_token: get("LICHESS_TOKEN"),
            lichess_lookback_days: parse(&get, "LICHESS_SINCE_LOOKBACK_DAYS", 0)?,
            pool_size: parse(&get, "WORKER_POOL_SIZE", 1)?,
            syzygy_path: get("SYZYGY_PATH").map(PathBuf::from),
            syzygy_stage_dir: get("SYZYGY_STAGE_DIR").map(PathBuf::from),
            data_dir: get("DATA_DIR").map(PathBuf::from),
            maia_url: get("MAIA_URL").map(|u| u.trim().to_owned()),
            plan_default: match get("WORKER_DEFAULT_PLAN") {
                None => Plan::Beta,
                Some(raw) => match raw.trim().to_ascii_lowercase().as_str() {
                    "beta" => Plan::Beta,
                    "free" => Plan::Free,
                    "pro" => Plan::Pro,
                    _ => bail!("WORKER_DEFAULT_PLAN must be beta, free or pro, got {raw:?}"),
                },
            },
        })
    }
}

impl std::fmt::Debug for WorkerEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerEnv")
            .field("database_url", &redact_url(&self.database_url))
            .field("stockfish_path", &self.stockfish_path)
            .field(
                "lichess_token",
                &self.lichess_token.as_ref().map(|_| REDACTED),
            )
            .field("lichess_lookback_days", &self.lichess_lookback_days)
            .field("pool_size", &self.pool_size)
            .field("syzygy_path", &self.syzygy_path)
            .field("syzygy_stage_dir", &self.syzygy_stage_dir)
            .field("data_dir", &self.data_dir)
            .field("maia_url", &self.maia_url)
            .field("plan_default", &self.plan_default)
            .finish()
    }
}

const REDACTED: &str = "***";

/// `scheme://user:password@host/...` with the password replaced by `***`. A URL without a
/// password is returned as it is.
pub fn redact_url(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_owned();
    };
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(authority_end);
    // The last `@` ends the userinfo (a password may itself contain `@`).
    let Some((userinfo, host)) = authority.rsplit_once('@') else {
        return url.to_owned();
    };
    match userinfo.split_once(':') {
        Some((user, _password)) => format!("{scheme}://{user}:{REDACTED}@{host}{tail}"),
        None => url.to_owned(),
    }
}

fn parse<T>(get: &impl Fn(&str) -> Option<String>, name: &str, min: T) -> anyhow::Result<Option<T>>
where
    T: std::str::FromStr + PartialOrd + std::fmt::Display,
{
    let Some(raw) = get(name) else {
        return Ok(None);
    };
    match raw.trim().parse::<T>() {
        Ok(v) if v >= min => Ok(Some(v)),
        _ => bail!("{name} must be an integer of at least {min}, got {raw:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(vars: &[(&str, &str)]) -> anyhow::Result<WorkerEnv> {
        let map: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        WorkerEnv::from_lookup(|name| map.get(name).cloned())
    }

    const BASE: [(&str, &str); 2] = [
        ("DATABASE_URL", "postgres://db/chess"),
        ("STOCKFISH_PATH", "/usr/local/bin/stockfish"),
    ];

    #[test]
    fn reads_the_required_and_optional_variables() {
        let mut vars = BASE.to_vec();
        vars.extend([
            ("LICHESS_TOKEN", "lip_x"),
            ("LICHESS_SINCE_LOOKBACK_DAYS", "7"),
            ("WORKER_POOL_SIZE", "3"),
            ("SYZYGY_PATH", "/work/data/syzygy"),
            ("CONTACT_EMAIL", "someone@example.com"),
        ]);
        assert_eq!(
            env(&vars).unwrap(),
            WorkerEnv {
                database_url: "postgres://db/chess".to_owned(),
                stockfish_path: PathBuf::from("/usr/local/bin/stockfish"),
                lichess_token: Some("lip_x".to_owned()),
                lichess_lookback_days: Some(7),
                pool_size: Some(3),
                syzygy_path: Some(PathBuf::from("/work/data/syzygy")),
                syzygy_stage_dir: None,
                data_dir: None,
                maia_url: None,
                plan_default: Plan::Beta,
            }
        );
        let plain = env(&BASE).unwrap();
        assert_eq!(plain.lichess_token, None);
        assert_eq!(plain.pool_size, None);
        assert_eq!(plain.syzygy_path, None);
    }

    #[test]
    fn reads_the_profile_engine_variables() {
        let mut vars = BASE.to_vec();
        vars.extend([
            ("DATA_DIR", "/data"),
            ("SYZYGY_STAGE_DIR", "/var/lib/chess-improver/syzygy"),
            ("MAIA_URL", "http://maia:8080"),
            ("WORKER_DEFAULT_PLAN", "free"),
        ]);
        let e = env(&vars).unwrap();
        assert_eq!(e.data_dir, Some(PathBuf::from("/data")));
        assert_eq!(
            e.syzygy_stage_dir,
            Some(PathBuf::from("/var/lib/chess-improver/syzygy"))
        );
        assert_eq!(e.maia_url.as_deref(), Some("http://maia:8080"));
        assert_eq!(e.plan_default, Plan::Free);
        for (raw, plan) in [
            ("beta", Plan::Beta),
            ("pro", Plan::Pro),
            (" Free ", Plan::Free),
        ] {
            let mut vars = BASE.to_vec();
            vars.push(("WORKER_DEFAULT_PLAN", raw));
            assert_eq!(env(&vars).unwrap().plan_default, plan, "{raw}");
        }
        // Unset: the private-beta plan (T29 ruling, ADR 0041), no data dir, no Maia.
        let plain = env(&BASE).unwrap();
        assert_eq!(
            (plain.data_dir, plain.maia_url, plain.plan_default),
            (None, None, Plan::Beta)
        );
        let shown = format!("{e:?}");
        assert!(
            shown.contains("maia_url: Some(\"http://maia:8080\")"),
            "{shown}"
        );
        assert!(shown.contains("plan_default: Free"), "{shown}");
    }

    #[test]
    fn rejects_an_unknown_plan() {
        let mut vars = BASE.to_vec();
        vars.push(("WORKER_DEFAULT_PLAN", "gold"));
        let err = env(&vars).unwrap_err().to_string();
        assert_eq!(
            err,
            "WORKER_DEFAULT_PLAN must be beta, free or pro, got \"gold\""
        );
    }

    #[test]
    fn names_the_missing_variable() {
        let err = env(&[("STOCKFISH_PATH", "/sf")]).unwrap_err();
        assert_eq!(err.to_string(), "DATABASE_URL is not set");
        let err = env(&[("DATABASE_URL", "postgres://x"), ("STOCKFISH_PATH", " ")]).unwrap_err();
        assert_eq!(err.to_string(), "STOCKFISH_PATH is not set");
    }

    #[test]
    fn rejects_bad_numbers() {
        for (name, value) in [
            ("WORKER_POOL_SIZE", "0"),
            ("WORKER_POOL_SIZE", "many"),
            ("LICHESS_SINCE_LOOKBACK_DAYS", "-1"),
        ] {
            let mut vars = BASE.to_vec();
            vars.push((name, value));
            let err = env(&vars).unwrap_err().to_string();
            assert!(err.starts_with(name), "{err}");
        }
    }

    #[test]
    fn debug_redacts_the_database_password_and_the_lichess_token() {
        let env = WorkerEnv {
            database_url: "postgres://postgres:s3cr@t@db:5432/chess?sslmode=disable".to_owned(),
            stockfish_path: PathBuf::from("/sf"),
            lichess_token: Some("lip_secret_token".to_owned()),
            lichess_lookback_days: None,
            pool_size: Some(2),
            syzygy_path: None,
            syzygy_stage_dir: None,
            data_dir: None,
            maia_url: None,
            plan_default: Plan::Beta,
        };
        let shown = format!("{env:?}");
        assert!(!shown.contains("s3cr"), "{shown}");
        assert!(!shown.contains("lip_secret_token"), "{shown}");
        assert!(
            shown.contains("postgres://postgres:***@db:5432/chess?sslmode=disable"),
            "{shown}"
        );
        assert!(shown.contains("lichess_token: Some(\"***\")"), "{shown}");
        assert!(shown.contains("pool_size: Some(2)"), "{shown}");
        assert_eq!(redact_url("postgres://db/chess"), "postgres://db/chess");
        assert_eq!(
            redact_url("postgres://user@db/chess"),
            "postgres://user@db/chess"
        );
        assert_eq!(redact_url("not a url"), "not a url");
    }

    #[test]
    fn does_not_read_contact_email() {
        let mut vars = BASE.to_vec();
        vars.push(("CONTACT_EMAIL", "someone@example.com"));
        assert!(!format!("{:?}", env(&vars).unwrap()).contains('@'));
    }
}
