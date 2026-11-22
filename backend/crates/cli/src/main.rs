//! The `cli` entry point. `db migrate` works; `analyse` needs the `worker` crate as a dependency
//! (a coordinator-owned Cargo change, see task-23-report.md) and is not wired yet.

#[tokio::main]
async fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.first().map(String::as_str) == Some("db") {
        let code = cli::db_cmd::run_db(&argv, &|l| println!("{l}"), &|l| eprintln!("{l}")).await;
        std::process::exit(code);
    }
    // Argument errors behave as in the final CLI (exit 1 plus usage); a valid command is not
    // runnable until `worker` is a dependency.
    eprintln!("cli: analyse is not wired to the workers yet");
    std::process::exit(2);
}
