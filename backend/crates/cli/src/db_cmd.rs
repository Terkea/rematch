//! `cli db migrate [--database-url URL]`: apply the migrations, adopting a Drizzle baseline first.

use clap::{Arg, Command};
use db::baseline::AdoptOutcome;

pub const USAGE: &str = "usage: cli db migrate [--database-url URL]";

/// Runs `db migrate`. `argv` starts at `db`. The URL defaults to `DATABASE_URL`.
pub async fn run_db(argv: &[String], print: &dyn Fn(&str), print_error: &dyn Fn(&str)) -> i32 {
    let matches = Command::new("cli")
        .no_binary_name(true)
        .disable_help_flag(true)
        .disable_version_flag(true)
        .arg(Arg::new("pos").num_args(0..))
        .arg(
            Arg::new("database-url")
                .long("database-url")
                .allow_hyphen_values(true),
        )
        .try_get_matches_from(argv);
    let m = match matches {
        Ok(m) => m,
        Err(e) => {
            print_error(&format!("{}\n{USAGE}", e.render().to_string().trim_end()));
            return 1;
        }
    };
    let pos: Vec<&str> = m
        .get_many::<String>("pos")
        .map(|p| p.map(String::as_str).collect())
        .unwrap_or_default();
    if pos != ["db", "migrate"] {
        print_error(USAGE);
        return 1;
    }
    let url = m
        .get_one::<String>("database-url")
        .cloned()
        .or_else(|| std::env::var("DATABASE_URL").ok())
        .filter(|u| !u.trim().is_empty());
    let Some(url) = url else {
        print_error(&format!(
            "--database-url or DATABASE_URL is required\n{USAGE}"
        ));
        return 1;
    };
    match migrate(&url).await {
        Ok(AdoptOutcome::Adopted) => {
            print("adopted the Drizzle baseline; migrations applied");
            0
        }
        Ok(_) => {
            print("migrations applied");
            0
        }
        Err(e) => {
            print_error(&format!("migrate failed: {e:#}"));
            1
        }
    }
}

/// Adopts the Drizzle baseline when needed, then migrates.
pub async fn migrate(url: &str) -> anyhow::Result<AdoptOutcome> {
    let pool = db::pool::connect_for_migrations(url).await?;
    let result = async {
        let outcome = db::baseline::adopt_drizzle_baseline(&pool).await?;
        db::pool::migrate(&pool).await?;
        anyhow::Ok(outcome)
    }
    .await;
    pool.close().await;
    result
}
