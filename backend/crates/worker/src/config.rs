//! The worker's environment.
//!
//! `CONTACT_EMAIL` is not read: no outbound request carries an email address, and the platform
//! clients send the fixed project User-Agent (ADR 0043).

use std::path::PathBuf;

use anyhow::{Context, bail};

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
            }
        );
        let plain = env(&BASE).unwrap();
        assert_eq!(plain.lichess_token, None);
        assert_eq!(plain.pool_size, None);
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
