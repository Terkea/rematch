//! `cli profile` and `cli eta` (R2 Task 35, ADR-R2-25): read-only views of an account's profile.
//! They connect to `DATABASE_URL` only; no worker is started and no account is created.

use clap::{Arg, ArgAction, Command};
use profile::coaching::build_coaching_input_report;
use profile::coaching_input::{AreaScore, CoachingInput};
use scheduler::eta::{account_eta, format_eta};
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::format::to_fixed;

pub const PROFILE_USAGE: &str = "usage: cli profile --platform <lichess|chesscom> --username <name> --time-class <bullet|blitz|rapid|classical|daily> [--json]";
pub const ETA_USAGE: &str = "usage: cli eta --platform <lichess|chesscom> --username <name>";

const PLATFORMS: [&str; 2] = ["lichess", "chesscom"];

fn command(with_profile_args: bool) -> Command {
    let cmd = Command::new("cli")
        .no_binary_name(true)
        .disable_help_flag(true)
        .disable_version_flag(true)
        .disable_help_subcommand(true)
        .arg(Arg::new("pos").num_args(0..))
        .arg(
            Arg::new("platform")
                .long("platform")
                .allow_hyphen_values(true),
        )
        .arg(
            Arg::new("username")
                .long("username")
                .allow_hyphen_values(true),
        );
    if with_profile_args {
        cmd.arg(
            Arg::new("time-class")
                .long("time-class")
                .allow_hyphen_values(true),
        )
        .arg(Arg::new("json").long("json").action(ArgAction::SetTrue))
    } else {
        cmd
    }
}

struct Who {
    platform: String,
    username: String,
}

fn who(m: &clap::ArgMatches, usage: &str) -> Result<Who, String> {
    let platform = match m.get_one::<String>("platform").map(String::as_str) {
        Some(p) if PLATFORMS.contains(&p) => p.to_owned(),
        _ => return Err(format!("--platform must be lichess or chesscom\n{usage}")),
    };
    let username = m
        .get_one::<String>("username")
        .map(|u| u.trim().to_owned())
        .unwrap_or_default();
    if username.is_empty() {
        return Err(format!("--username is required\n{usage}"));
    }
    Ok(Who { platform, username })
}

fn matches(
    argv: &[String],
    first: &str,
    profile: bool,
    usage: &str,
) -> Result<clap::ArgMatches, String> {
    let m = command(profile)
        .try_get_matches_from(argv)
        .map_err(|e| format!("{}\n{usage}", e.render().to_string().trim_end()))?;
    let pos = m
        .get_many::<String>("pos")
        .map(|p| p.map(String::as_str).collect::<Vec<_>>())
        .unwrap_or_default();
    if pos != [first] {
        return Err(usage.to_owned());
    }
    Ok(m)
}

fn parse_time_class(s: &str) -> Option<chess_core::TimeClass> {
    use chess_core::TimeClass;
    Some(match s {
        "bullet" => TimeClass::Bullet,
        "blitz" => TimeClass::Blitz,
        "rapid" => TimeClass::Rapid,
        "classical" => TimeClass::Classical,
        "daily" => TimeClass::Daily,
        _ => return None,
    })
}

/// The account for (platform, lower-cased username), or `None`. Never creates one.
async fn find_account(pool: &PgPool, w: &Who) -> anyhow::Result<Option<Uuid>> {
    Ok(sqlx::query_scalar(
        "SELECT id FROM linked_accounts WHERE platform = $1::platform AND username = $2",
    )
    .bind(&w.platform)
    .bind(w.username.to_lowercase())
    .fetch_optional(pool)
    .await?)
}

fn not_found(w: &Who) -> String {
    format!("no account \"{}\" on {}", w.username, w.platform)
}

/// `area score`, `area need N more`, or `area no benchmark`.
fn area(a: &AreaScore) -> String {
    let name = a.area.as_str();
    match (a.score, a.sufficient, a.need_more) {
        (Some(score), _, _) => format!("{name} {}", to_fixed(score, 0)),
        (None, false, Some(n)) => format!("{name} need {n} more"),
        (None, _, _) => format!("{name} no benchmark"),
    }
}

/// `band B2  analysed 214/3912  focus-ready weaknesses N  areas: openings 54  ...`.
pub fn summary_line(input: &CoachingInput) -> String {
    let areas: Vec<String> = input.area_scores.iter().map(area).collect();
    format!(
        "band {}  analysed {}/{}  focus-ready weaknesses {}  areas: {}",
        input.band.as_str(),
        input.basis.analysed_games,
        input.basis.imported_games,
        input.weaknesses.len(),
        areas.join("  "),
    )
}

async fn band_reference_active(pool: &PgPool) -> anyhow::Result<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM reference_builds \
         WHERE kind = 'band_reference' AND status = 'active')",
    )
    .fetch_one(pool)
    .await?)
}

/// `cli profile ...`: the account × time class `CoachingInput` (ADR-R2-18), as the summary line
/// or, with `--json`, the whole validated input. Returns the exit code.
pub async fn run_profile(
    argv: &[String],
    pool: &PgPool,
    now: OffsetDateTime,
    print: &dyn Fn(&str),
    print_error: &dyn Fn(&str),
) -> i32 {
    let parsed = matches(argv, "profile", true, PROFILE_USAGE).and_then(|m| {
        let w = who(&m, PROFILE_USAGE)?;
        let tc = m
            .get_one::<String>("time-class")
            .and_then(|t| parse_time_class(t.trim()))
            .ok_or_else(|| {
                format!(
                    "--time-class must be bullet, blitz, rapid, classical or daily\n{PROFILE_USAGE}"
                )
            })?;
        Ok((w, tc, m.get_flag("json")))
    });
    let (w, tc, json) = match parsed {
        Ok(p) => p,
        Err(message) => {
            print_error(&message);
            return 1;
        }
    };
    let run = async {
        let Some(account) = find_account(pool, &w).await? else {
            return Ok(Err(not_found(&w)));
        };
        let report = build_coaching_input_report(pool, account, tc, now).await?;
        if json {
            return Ok(Ok(vec![serde_json::to_string_pretty(&report.input)?]));
        }
        let mut lines = vec![summary_line(&report.input)];
        if !band_reference_active(pool).await? {
            lines.push(
                "note: no band-reference build is active, so areas and weaknesses have no \
                 benchmark yet (run lichess-db build --band-reference)"
                    .to_owned(),
            );
        }
        if let Some(at) = report.snapshots_computed_at {
            lines.push(format!("snapshots computed at {at}"));
        }
        anyhow::Ok(Ok(lines))
    };
    match run.await {
        Ok(Ok(lines)) => {
            for l in lines {
                print(&l);
            }
            0
        }
        Ok(Err(message)) => {
            print_error(&message);
            1
        }
        Err(e) => {
            print_error(&format!("{e:#}"));
            1
        }
    }
}

/// `cli eta ...`: the account's analysis ETA sentence (ADR-R2-17). Returns the exit code.
pub async fn run_eta(
    argv: &[String],
    pool: &PgPool,
    print: &dyn Fn(&str),
    print_error: &dyn Fn(&str),
) -> i32 {
    let w = match matches(argv, "eta", false, ETA_USAGE).and_then(|m| who(&m, ETA_USAGE)) {
        Ok(w) => w,
        Err(message) => {
            print_error(&message);
            return 1;
        }
    };
    let run = async {
        let Some(account) = find_account(pool, &w).await? else {
            return Ok(Err(not_found(&w)));
        };
        anyhow::Ok(Ok(format_eta(&account_eta(pool, account).await?)))
    };
    match run.await {
        Ok(Ok(line)) => {
            print(&line);
            0
        }
        Ok(Err(message)) => {
            print_error(&message);
            1
        }
        Err(e) => {
            print_error(&format!("{e:#}"));
            1
        }
    }
}
