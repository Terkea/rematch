// `sqlx::migrate!` embeds db/migrations at compile time, but cargo does not know about that
// directory. Rebuild this crate whenever a migration is added, changed or removed.
// The path is relative to this crate's directory (backend/crates/db).
fn main() {
    println!("cargo:rerun-if-changed=../../../db/migrations");
}
