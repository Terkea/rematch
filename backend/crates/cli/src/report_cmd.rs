//! `cli report-rebuild` (ADR 0086): the operator's way to rebuild an account's report now, outside
//! the player's manual-report quota. It stores a fresh coaching-input snapshot of the current
//! profile (so the coach is current at once), then queues a sync report run (`trigger =
//! scheduled`) for a running worker. The free plan's report interval still applies.

use clap::{Arg, Command};
use profile::coaching_input::TimeClass as ContractTimeClass;
use queue::queue::Queue;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;
use worker::config::AiPlan;

pub const REPORT_REBUILD_USAGE: &str = "usage: cli report-rebuild (--account <uuid> | --platform <lichess|chesscom> --username <name>) --time-class <bullet|blitz|rapid|classical|daily>";

struct Args {
    account: Who,
    time_class: ContractTimeClass,
}

enum Who {
    Id(Uuid),
    Name { platform: String, username: String },
}

fn parse(argv: &[String]) -> Result<Args, String> {
    let usage = || REPORT_REBUILD_USAGE.to_owned();
    let m = Command::new("cli")
        .no_binary_name(true)
        .disable_help_flag(true)
        .disable_version_flag(true)
        .disable_help_subcommand(true)
        .arg(Arg::new("pos").num_args(0..))
        .arg(Arg::new("account").long("account"))
        .arg(Arg::new("platform").long("platform"))
        .arg(Arg::new("username").long("username"))
        .arg(Arg::new("time-class").long("time-class"))
        .try_get_matches_from(argv)
        .map_err(|e| format!("{}\n{}", e.render().to_string().trim_end(), usage()))?;
    let pos: Vec<&str> = m
        .get_many::<String>("pos")
        .map(|p| p.map(String::as_str).collect())
        .unwrap_or_default();
    if pos != ["report-rebuild"] {
        return Err(usage());
    }
    let get = |k: &str| m.get_one::<String>(k).map(|v| v.trim().to_owned());
    let account = match (get("account"), get("platform"), get("username")) {
        (Some(id), None, None) => Who::Id(
            id.parse()
                .map_err(|_| format!("--account must be a uuid\n{}", usage()))?,
        ),
        (None, Some(platform), Some(username))
            if ["lichess", "chesscom"].contains(&platform.as_str()) && !username.is_empty() =>
        {
            Who::Name { platform, username }
        }
        _ => return Err(usage()),
    };
    let time_class = get("time-class")
        .and_then(|t| t.parse::<ContractTimeClass>().ok())
        .ok_or_else(usage)?;
    Ok(Args {
        account,
        time_class,
    })
}

async fn account_id(pool: &PgPool, who: &Who) -> anyhow::Result<Option<Uuid>> {
    Ok(match who {
        Who::Id(id) => {
            sqlx::query_scalar("SELECT id FROM linked_accounts WHERE id = $1")
                .bind(id)
                .fetch_optional(pool)
                .await?
        }
        Who::Name { platform, username } => {
            sqlx::query_scalar(
                "SELECT id FROM linked_accounts WHERE platform = $1::platform AND username = $2",
            )
            .bind(platform)
            .bind(username.to_lowercase())
            .fetch_optional(pool)
            .await?
        }
    })
}

/// Runs the command with the deployment's AI plan; returns the exit code.
pub async fn run_report_rebuild(
    argv: &[String],
    pool: &PgPool,
    plan: AiPlan,
    print: &dyn Fn(&str),
    print_error: &dyn Fn(&str),
) -> i32 {
    let args = match parse(argv) {
        Ok(a) => a,
        Err(e) => {
            print_error(&e);
            return 1;
        }
    };
    let run = async {
        let Some(account) = account_id(pool, &args.account).await? else {
            anyhow::bail!("no such account");
        };
        let tc = args.time_class;
        let core_tc = match tc {
            ContractTimeClass::Bullet => chess_core::TimeClass::Bullet,
            ContractTimeClass::Blitz => chess_core::TimeClass::Blitz,
            ContractTimeClass::Rapid => chess_core::TimeClass::Rapid,
            ContractTimeClass::Classical => chess_core::TimeClass::Classical,
            ContractTimeClass::Daily => chess_core::TimeClass::Daily,
        };
        match profile::coaching::snapshot::store_coaching_snapshot(
            pool,
            account,
            core_tc,
            OffsetDateTime::now_utc(),
        )
        .await?
        {
            Some(id) => print(&format!("coaching-input snapshot {id} stored")),
            None => print("no analysed games: no coaching-input snapshot"),
        }
        let queue = Queue::new(pool.clone());
        let run =
            worker::coach_triggers::request_report_rebuild(pool, &queue, account, tc, plan).await?;
        print(&format!(
            "report run {run} queued for account {account} ({}); a running worker generates it",
            tc.as_str()
        ));
        anyhow::Ok(())
    };
    match run.await {
        Ok(()) => 0,
        Err(e) => {
            print_error(&format!("{e:#}"));
            1
        }
    }
}
