//! The worker's environment.
//!
//! `CONTACT_EMAIL` is not read: no outbound request carries an email address, and the platform
//! clients send the fixed project User-Agent (ADR 0043).

use std::net::SocketAddr;
use std::path::PathBuf;

use ai::client::batches::ApiKey;
use ai::client::sidecar::SidecarToken;
use ai::client::tool_server::DEFAULT_TOOL_BIND;
use ai::jobs::budget::BudgetConfig;
use anyhow::{Context, bail};

/// `AI_SIDECAR_URL` and `AI_SIDECAR_TOKEN` (ADR 0062). `Debug` redacts the token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidecarEnv {
    pub url: String,
    pub token: SidecarToken,
}

/// `Debug` redacts the database password, the Lichess token, the Anthropic key and the sidecar
/// token, so the value can be logged.
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
    /// `ANTHROPIC_API_KEY`: in Rust only the Message Batches client uses it (interactive calls
    /// go through the sidecar, ADR 0062). Unset: batches are off.
    pub anthropic_api_key: Option<ApiKey>,
    /// `AI_BUDGET_FREE_MICRO_USD`, `AI_BUDGET_PRO_MICRO_USD` and
    /// `AI_GLOBAL_MONTHLY_BUDGET_MICRO_USD` (ADR 0061).
    pub ai_budgets: BudgetConfig,
    /// The sidecar. `None` (no `AI_SIDECAR_URL`): AI is off and reports are computed-only.
    pub ai_sidecar: Option<SidecarEnv>,
    /// `AI_TOOL_BIND`: where the chat tool endpoint listens (default `0.0.0.0:7311`).
    pub ai_tool_bind: SocketAddr,
    /// `AI_TOOL_PUBLIC_URL`: how the sidecar reaches the tool endpoint. Unset: chat offers no
    /// tools.
    pub ai_tool_public_url: Option<String>,
    /// `AI_PRICING_FILE`: the model price table. Unset: the dated seed (ADR 0061).
    pub ai_pricing_file: Option<PathBuf>,
}

impl WorkerEnv {
    /// The required variables, with every optional one unset.
    pub fn new(database_url: impl Into<String>, stockfish_path: impl Into<PathBuf>) -> WorkerEnv {
        WorkerEnv {
            database_url: database_url.into(),
            stockfish_path: stockfish_path.into(),
            lichess_token: None,
            lichess_lookback_days: None,
            pool_size: None,
            syzygy_path: None,
            anthropic_api_key: None,
            ai_budgets: BudgetConfig::default(),
            ai_sidecar: None,
            ai_tool_bind: default_tool_bind(),
            ai_tool_public_url: None,
            ai_pricing_file: None,
        }
    }

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
            anthropic_api_key: get("ANTHROPIC_API_KEY").map(|k| ApiKey::new(k.trim())),
            ai_budgets: BudgetConfig::from_vars(&get)?,
            ai_sidecar: match (get("AI_SIDECAR_URL"), get("AI_SIDECAR_TOKEN")) {
                (None, _) => None,
                (Some(_), None) => bail!("AI_SIDECAR_URL is set but AI_SIDECAR_TOKEN is not"),
                (Some(url), Some(token)) => Some(SidecarEnv {
                    url: url.trim().to_owned(),
                    token: SidecarToken::new(token.trim()),
                }),
            },
            ai_tool_bind: match get("AI_TOOL_BIND") {
                None => default_tool_bind(),
                Some(raw) => raw
                    .trim()
                    .parse()
                    .with_context(|| format!("AI_TOOL_BIND must be host:port, got {raw:?}"))?,
            },
            ai_tool_public_url: get("AI_TOOL_PUBLIC_URL").map(|v| v.trim().to_owned()),
            ai_pricing_file: get("AI_PRICING_FILE").map(PathBuf::from),
        })
    }
}

fn default_tool_bind() -> SocketAddr {
    DEFAULT_TOOL_BIND
        .parse()
        .expect("DEFAULT_TOOL_BIND is a socket address")
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
            .field("anthropic_api_key", &self.anthropic_api_key)
            .field("ai_budgets", &self.ai_budgets)
            .field("ai_sidecar", &self.ai_sidecar)
            .field("ai_tool_bind", &self.ai_tool_bind)
            .field("ai_tool_public_url", &self.ai_tool_public_url)
            .field("ai_pricing_file", &self.ai_pricing_file)
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
                lichess_token: Some("lip_x".to_owned()),
                lichess_lookback_days: Some(7),
                pool_size: Some(3),
                syzygy_path: Some(PathBuf::from("/work/data/syzygy")),
                ..WorkerEnv::new("postgres://db/chess", "/usr/local/bin/stockfish")
            }
        );
        let plain = env(&BASE).unwrap();
        assert_eq!(plain.lichess_token, None);
        assert_eq!(plain.pool_size, None);
        assert_eq!(plain.syzygy_path, None);
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
            lichess_token: Some("lip_secret_token".to_owned()),
            pool_size: Some(2),
            anthropic_api_key: Some(ApiKey::new("sk-ant-secret-key")),
            ai_sidecar: Some(SidecarEnv {
                url: "http://ai-sidecar:7310".to_owned(),
                token: SidecarToken::new("sidecar-secret-token"),
            }),
            ..WorkerEnv::new(
                "postgres://postgres:s3cr@t@db:5432/chess?sslmode=disable",
                "/sf",
            )
        };
        let shown = format!("{env:?}");
        assert!(!shown.contains("s3cr"), "{shown}");
        assert!(!shown.contains("lip_secret_token"), "{shown}");
        assert!(!shown.contains("sk-ant-secret-key"), "{shown}");
        assert!(!shown.contains("sidecar-secret-token"), "{shown}");
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
    fn reads_the_ai_variables() {
        let mut vars = BASE.to_vec();
        vars.extend([
            ("ANTHROPIC_API_KEY", " sk-ant-x "),
            ("AI_SIDECAR_URL", "http://ai-sidecar:7310"),
            ("AI_SIDECAR_TOKEN", "tok"),
            ("AI_TOOL_BIND", "0.0.0.0:7400"),
            ("AI_TOOL_PUBLIC_URL", "http://rust-worker:7400"),
            ("AI_BUDGET_PRO_MICRO_USD", "123"),
            ("AI_GLOBAL_MONTHLY_BUDGET_MICRO_USD", "999"),
            ("AI_PRICING_FILE", "/etc/pricing.json"),
        ]);
        let e = env(&vars).unwrap();
        assert_eq!(e.anthropic_api_key, Some(ApiKey::new("sk-ant-x")));
        assert_eq!(
            e.ai_sidecar,
            Some(SidecarEnv {
                url: "http://ai-sidecar:7310".to_owned(),
                token: SidecarToken::new("tok"),
            })
        );
        assert_eq!(e.ai_tool_bind, "0.0.0.0:7400".parse().unwrap());
        assert_eq!(
            e.ai_tool_public_url.as_deref(),
            Some("http://rust-worker:7400")
        );
        assert_eq!(e.ai_budgets.per_plan.pro, 123);
        assert_eq!(e.ai_budgets.global_monthly, Some(999));
        assert_eq!(e.ai_pricing_file, Some(PathBuf::from("/etc/pricing.json")));
        let plain = env(&BASE).unwrap();
        assert_eq!(plain.anthropic_api_key, None);
        assert_eq!(plain.ai_sidecar, None);
        assert_eq!(plain.ai_tool_bind, "0.0.0.0:7311".parse().unwrap());
        assert_eq!(plain.ai_budgets, BudgetConfig::default());
    }

    #[test]
    fn rejects_bad_ai_variables() {
        for (name, value, needle) in [
            (
                "AI_SIDECAR_URL",
                "http://ai-sidecar:7310",
                "AI_SIDECAR_TOKEN",
            ),
            (
                "AI_BUDGET_FREE_MICRO_USD",
                "lots",
                "AI_BUDGET_FREE_MICRO_USD",
            ),
            ("AI_TOOL_BIND", "nowhere", "AI_TOOL_BIND"),
        ] {
            let mut vars = BASE.to_vec();
            vars.push((name, value));
            let err = format!("{:#}", env(&vars).unwrap_err());
            assert!(err.contains(needle), "{err}");
        }
    }

    #[test]
    fn does_not_read_contact_email() {
        let mut vars = BASE.to_vec();
        vars.push(("CONTACT_EMAIL", "someone@example.com"));
        assert!(!format!("{:?}", env(&vars).unwrap()).contains('@'));
    }
}
