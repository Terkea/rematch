//! Port of apps/worker/test/cli.test.ts, plus the paused-run and newest-run cases.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cli::analyse::ListRuns;
use cli::{AnalyseDeps, CliDeps, Signal, Started, run_analyse, run_cli, to_fixed};
use db::import_runs::{ImportRunRow, ImportRunStatus};
use futures::FutureExt;
use sqlx::PgPool;
use testkit::{GameSeed, insert_game};
use time::OffsetDateTime;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Clone, Copy)]
struct Fake {
    outcome: &'static str,
    color: &'static str,
    acc: f32,
}

#[derive(Clone)]
enum Mode {
    Games(Vec<Fake>),
    NotFound,
    ImportFail,
}

fn run_row(
    account: Uuid,
    status: ImportRunStatus,
    code: Option<&str>,
    err: Option<&str>,
) -> ImportRunRow {
    let now = OffsetDateTime::now_utc();
    ImportRunRow {
        id: Uuid::new_v4(),
        account_id: account,
        status,
        imported: 0,
        started_at: now,
        heartbeat_at: now,
        finished_at: None,
        last_error: err.map(str::to_owned),
        error_code: code.map(str::to_owned),
    }
}

async fn seed_run(pool: &PgPool, account: Uuid, status: &str) {
    sqlx::query("INSERT INTO import_runs (account_id, status) VALUES ($1, $2::import_run_status)")
        .bind(account)
        .bind(status)
        .execute(pool)
        .await
        .unwrap();
}

struct Harness {
    deps: AnalyseDeps,
    out: Arc<Mutex<Vec<String>>>,
    idle: Arc<AtomicBool>,
}

fn harness(pool: &PgPool, mode: Mode) -> Harness {
    let out = Arc::new(Mutex::new(Vec::new()));
    let idle = Arc::new(AtomicBool::new(false));
    let o1 = out.clone();
    let o2 = out.clone();
    let idle_enqueue = idle.clone();
    let idle_q = idle.clone();
    let idle_list = idle.clone();
    let pool_e = pool.clone();
    let mode_e = mode.clone();
    let mode_l = mode.clone();
    let list_pool = pool.clone();
    let list_runs: ListRuns = Box::new(move |account| {
        let idle = idle_list.load(Ordering::SeqCst);
        let mode = mode_l.clone();
        let pool = list_pool.clone();
        async move {
            Ok(match (&mode, idle) {
                (Mode::NotFound, true) => vec![run_row(
                    account,
                    ImportRunStatus::Failed,
                    Some("not_found"),
                    Some("Not found: x"),
                )],
                (Mode::ImportFail, true) => vec![run_row(
                    account,
                    ImportRunStatus::Failed,
                    Some("unknown"),
                    Some("boom"),
                )],
                _ => db::import_runs::list_runs(&pool, account).await?,
            })
        }
        .boxed()
    });
    let deps = AnalyseDeps {
        pool: pool.clone(),
        print: Box::new(move |l| o1.lock().unwrap().push(l.to_owned())),
        print_error: Box::new(move |l| o2.lock().unwrap().push(l.to_owned())),
        poll: Duration::from_millis(5),
        grace: Duration::from_secs(60),
        cancel: CancellationToken::new(),
        queues_idle: Box::new(move |_| {
            let v = idle_q.load(Ordering::SeqCst);
            async move { Ok(v) }.boxed()
        }),
        enqueue_import: Box::new(move |account, _max| {
            let pool = pool_e.clone();
            let mode = mode_e.clone();
            let idle = idle_enqueue.clone();
            async move {
                if let Mode::Games(list) = mode {
                    for (i, f) in list.iter().enumerate() {
                        let game = insert_game(
                            &pool,
                            account,
                            GameSeed {
                                platform_game_id: format!("g{i}-{}", Uuid::new_v4()),
                                player_color: f.color,
                                status: f.outcome,
                                ..GameSeed::default()
                            },
                        )
                        .await;
                        let (w, b) = if f.color == "white" { (f.acc, 50.0) } else { (50.0, f.acc) };
                        sqlx::query("UPDATE games SET white_accuracy = $2, black_accuracy = $3 WHERE id = $1")
                            .bind(game)
                            .bind(w)
                            .bind(b)
                            .execute(&pool)
                            .await?;
                        if f.outcome == "done" {
                            sqlx::query(
                                "INSERT INTO moves (game_id, ply, color, san, uci, fen_before, win_before, \
                                 win_after, delta_w, accuracy, judgement, phase, decided) \
                                 VALUES ($1, 1, $2::color, 'e4', 'e2e4', 'x', 0.5, 0.1, -0.4, 10, \
                                 'blunder', 'opening', false)",
                            )
                            .bind(game)
                            .bind(f.color)
                            .execute(&pool)
                            .await?;
                        }
                    }
                    seed_run(&pool, account, "done").await;
                }
                idle.store(true, Ordering::SeqCst);
                Ok(())
            }
            .boxed()
        }),
        list_runs: Some(list_runs),
    };
    Harness { deps, out, idle }
}

fn args(name: &str) -> Vec<String> {
    ["analyse", "--platform", "lichess", "--username", name]
        .map(str::to_owned)
        .to_vec()
}

fn lines(h: &Harness) -> Vec<String> {
    h.out.lock().unwrap().clone()
}

fn fake(outcome: &'static str, color: &'static str, acc: f32) -> Fake {
    Fake {
        outcome,
        color,
        acc,
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn prints_the_exact_summary_line(pool: PgPool) {
    let h = harness(
        &pool,
        Mode::Games(vec![
            fake("done", "white", 80.0),
            fake("done", "black", 91.0),
            fake("skipped", "white", 0.0),
            fake("failed", "white", 0.0),
        ]),
    );
    assert_eq!(run_analyse(&args("Hero"), &h.deps).await, 0);
    assert_eq!(
        lines(&h).last().unwrap(),
        "games: 4  analysed: 2  skipped: 1  failed: 1  avg accuracy: 85.5  blunders/game: 1.00"
    );
    let display: String =
        sqlx::query_scalar("SELECT display_name FROM linked_accounts WHERE username = 'hero'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(display, "Hero");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn exits_1_when_every_game_failed(pool: PgPool) {
    let h = harness(&pool, Mode::Games(vec![fake("failed", "white", 0.0)]));
    assert_eq!(run_analyse(&args("Allfail"), &h.deps).await, 1);
    assert!(lines(&h).last().unwrap().contains("failed: 1"));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn not_found_message(pool: PgPool) {
    let h = harness(&pool, Mode::NotFound);
    assert_eq!(run_analyse(&args("Ghost"), &h.deps).await, 1);
    assert!(lines(&h).contains(&"User \"Ghost\" not found on lichess".to_owned()));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn import_failure_message(pool: PgPool) {
    let h = harness(&pool, Mode::ImportFail);
    assert_eq!(run_analyse(&args("Broken"), &h.deps).await, 1);
    assert!(lines(&h).contains(&"import failed: boom".to_owned()));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_message_without_the_code_is_not_not_found(pool: PgPool) {
    let mut h = harness(&pool, Mode::ImportFail);
    h.deps.list_runs = Some(Box::new(|account| {
        async move {
            Ok(vec![run_row(
                account,
                ImportRunStatus::Failed,
                Some("unknown"),
                Some("Not found: x"),
            )])
        }
        .boxed()
    }));
    assert_eq!(run_analyse(&args("Msgonly"), &h.deps).await, 1);
    assert!(!lines(&h).iter().any(|l| l.contains("not found on")));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn bad_arguments_exit_1_with_usage(pool: PgPool) {
    let h = harness(&pool, Mode::Games(vec![]));
    let bad = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    for (argv, msg) in [
        (
            bad(&["analyse", "--platform", "x", "--username", "a"]),
            "--platform must be lichess or chesscom",
        ),
        (
            bad(&["analyse", "--platform", "lichess"]),
            "--username is required",
        ),
        (
            bad(&[
                "analyse",
                "--platform",
                "lichess",
                "--username",
                "a",
                "--max",
                "0",
            ]),
            "--max must be a positive number",
        ),
        (
            bad(&[
                "analyse",
                "--platform",
                "lichess",
                "--username",
                "a",
                "--timeout",
                "-1",
            ]),
            "--timeout must be a positive number",
        ),
        (
            bad(&[
                "analyse",
                "--platform",
                "lichess",
                "--username",
                "a",
                "--bogus",
            ]),
            "usage: cli analyse",
        ),
    ] {
        h.out.lock().unwrap().clear();
        assert_eq!(run_analyse(&argv, &h.deps).await, 1, "{argv:?}");
        let all = lines(&h).join("\n");
        assert!(all.contains(msg), "{all}");
        if !msg.starts_with("usage") && !msg.contains("--max") && !msg.contains("--timeout") {
            assert!(all.contains("usage: cli analyse --platform <lichess|chesscom> --username <name> [--max N] [--timeout MINUTES]"));
        }
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn no_games_found(pool: PgPool) {
    let h = harness(&pool, Mode::Games(vec![]));
    assert_eq!(run_analyse(&args("Nogames"), &h.deps).await, 0);
    assert_eq!(
        lines(&h).last().unwrap(),
        "no standard games found for Nogames on lichess"
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn ignores_stale_games_and_warns(pool: PgPool) {
    let account = testkit::insert_account(&pool, "lichess", "Stale").await;
    insert_game(
        &pool,
        account,
        GameSeed {
            status: "analysing",
            ..GameSeed::default()
        },
    )
    .await;
    let h = harness(&pool, Mode::Games(vec![fake("done", "white", 70.0)]));
    assert_eq!(run_analyse(&args("Stale"), &h.deps).await, 0);
    let l = lines(&h);
    assert!(l.contains(&"warning: 1 game(s) left over from an earlier run are still pending/analysing and were ignored".to_owned()), "{l:?}");
    assert!(l.last().unwrap().contains("analysed: 1"));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn unselected_imported_games_do_not_stall_the_run(pool: PgPool) {
    // R2 (ADR-R2-16): games the scheduler did not select stay `imported`; the run ends as soon as
    // the queues are idle, without waiting out the grace or warning about them.
    let h = harness(
        &pool,
        Mode::Games(vec![
            fake("done", "white", 70.0),
            fake("imported", "white", 0.0),
        ]),
    );
    let started = std::time::Instant::now();
    assert_eq!(run_analyse(&args("Unselected"), &h.deps).await, 0);
    assert!(started.elapsed() < Duration::from_secs(30));
    let l = lines(&h);
    assert!(l.iter().all(|x| !x.starts_with("warning")), "{l:?}");
    assert!(l.last().unwrap().contains("analysed: 1"), "{l:?}");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn gives_up_after_the_idle_grace(pool: PgPool) {
    let mut h = harness(&pool, Mode::Games(vec![fake("done", "white", 70.0)]));
    let inner = h.deps.enqueue_import;
    let p = pool.clone();
    h.deps.enqueue_import = Box::new(move |a, m| {
        let fut = inner(a, m);
        let p = p.clone();
        async move {
            fut.await?;
            sqlx::query("UPDATE games SET analysis_status = 'pending' WHERE account_id = $1")
                .bind(a)
                .execute(&p)
                .await?;
            Ok(())
        }
        .boxed()
    });
    h.deps.grace = Duration::from_millis(30);
    assert_eq!(run_analyse(&args("Stuck"), &h.deps).await, 0);
    assert!(lines(&h).iter().any(|l| {
        l.starts_with("warning: queues are idle but 1 game(s) are not finished; giving up (")
    }));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn times_out_with_the_message(pool: PgPool) {
    let mut h = harness(&pool, Mode::Games(vec![]));
    h.deps.queues_idle = Box::new(|_| async { Ok(false) }.boxed());
    let mut a = args("Slow");
    a.extend(["--timeout".into(), "0.001".into()]);
    assert_eq!(run_analyse(&a, &h.deps).await, 1);
    assert!(lines(&h).contains(&"timed out after 0 min (0/0 games settled)".to_owned()));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn max_is_floored_and_passed(pool: PgPool) {
    let seen = Arc::new(Mutex::new(None));
    let mut h = harness(&pool, Mode::Games(vec![]));
    let inner = h.deps.enqueue_import;
    let s = seen.clone();
    h.deps.enqueue_import = Box::new(move |a, m| {
        *s.lock().unwrap() = Some(m);
        inner(a, m)
    });
    let mut a = args("Maxer");
    a.extend(["--max".into(), "7.9".into()]);
    run_analyse(&a, &h.deps).await;
    assert_eq!(*seen.lock().unwrap(), Some(Some(7)));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn waits_through_paused_run(pool: PgPool) {
    let mut h = harness(&pool, Mode::Games(vec![fake("done", "white", 70.0)]));
    let polls = Arc::new(AtomicUsize::new(0));
    let p = polls.clone();
    let list_pool = pool.clone();
    h.deps.list_runs = Some(Box::new(move |account| {
        let n = p.fetch_add(1, Ordering::SeqCst);
        let pool = list_pool.clone();
        async move {
            // Call 0 is the "known runs" snapshot (none); then paused for 3 polls, then done.
            let _ = pool;
            Ok(match n {
                0 => vec![],
                1..=3 => vec![run_row(account, ImportRunStatus::Paused, None, None)],
                _ => vec![run_row(account, ImportRunStatus::Done, None, None)],
            })
        }
        .boxed()
    }));
    h.deps.queues_idle = Box::new(|_| async { Ok(true) }.boxed());
    assert_eq!(run_analyse(&args("Pauser"), &h.deps).await, 0);
    let l = lines(&h);
    assert!(!l.iter().any(|x| x.contains("import failed")), "{l:?}");
    assert!(l.last().unwrap().starts_with("games: 1  analysed: 1"));
    assert!(polls.load(Ordering::SeqCst) >= 5);
    let _ = h.idle;
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn follows_the_newest_run_only(pool: PgPool) {
    let account = testkit::insert_account(&pool, "lichess", "Older").await;
    seed_run(&pool, account, "failed").await; // from before this invocation
    let h = harness(&pool, Mode::Games(vec![fake("done", "white", 70.0)]));
    assert_eq!(run_analyse(&args("Older"), &h.deps).await, 0);
    let l = lines(&h);
    assert!(!l.iter().any(|x| x.contains("import failed")), "{l:?}");
    assert!(l.last().unwrap().starts_with("games: 1"));
}

#[test]
fn to_fixed_matches_js_on_ties() {
    assert_eq!(to_fixed(1.125, 2), "1.13");
    assert_eq!(to_fixed(0.125, 2), "0.13");
    assert_eq!(to_fixed(85.5, 1), "85.5");
    assert_eq!(to_fixed(65.05, 1), "65.0");
    assert_eq!(to_fixed(0.0, 2), "0.00");
}

// ---- signals ----

struct Sig {
    tx: mpsc::Sender<Signal>,
    cli: CliDeps,
    closes: Arc<AtomicUsize>,
    exits: Arc<Mutex<Vec<i32>>>,
}

fn sig_harness(pool: &PgPool, close_sends: Option<Signal>) -> Sig {
    let (tx, rx) = mpsc::channel(8);
    let closes = Arc::new(AtomicUsize::new(0));
    let exits = Arc::new(Mutex::new(Vec::new()));
    let mut h = harness(pool, Mode::Games(vec![]));
    h.deps.queues_idle = Box::new(|_| async { Ok(false) }.boxed());
    let (c, t) = (closes.clone(), tx.clone());
    let e = exits.clone();
    let cli = CliDeps {
        start: async move {
            Ok(Started {
                analyse: h.deps,
                close: async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    if let Some(s) = close_sends {
                        t.try_send(s).unwrap();
                    }
                    0
                }
                .boxed(),
            })
        }
        .boxed(),
        signals: rx,
        exit: Box::new(move |code| e.lock().unwrap().push(code)),
        print_error: Box::new(|_| {}),
    };
    Sig {
        tx,
        cli,
        closes,
        exits,
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn sigint_gives_130_and_sigterm_143(pool: PgPool) {
    for (sig, code) in [(Signal::Int, 130), (Signal::Term, 143)] {
        let s = sig_harness(&pool, None);
        let tx = s.tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            tx.send(sig).await.unwrap();
        });
        assert_eq!(run_cli(&args("Sigger"), s.cli).await, code);
        assert_eq!(s.closes.load(Ordering::SeqCst), 1);
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_second_signal_exits_immediately(pool: PgPool) {
    let s = sig_harness(&pool, Some(Signal::Int));
    let tx = s.tx.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(30)).await;
        tx.send(Signal::Int).await.unwrap();
    });
    assert_eq!(run_cli(&args("Twice"), s.cli).await, 130);
    assert_eq!(*s.exits.lock().unwrap(), vec![130]);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_failed_close_turns_success_into_1(pool: PgPool) {
    let mut h = harness(&pool, Mode::Games(vec![]));
    h.deps.queues_idle = Box::new(|_| async { Ok(true) }.boxed());
    let (_tx, rx) = mpsc::channel(1);
    let cli = CliDeps {
        start: async move {
            Ok(Started {
                analyse: h.deps,
                close: async { 1 }.boxed(),
            })
        }
        .boxed(),
        signals: rx,
        exit: Box::new(|_| {}),
        print_error: Box::new(|_| {}),
    };
    assert_eq!(run_cli(&args("Closer"), cli).await, 1);
}
