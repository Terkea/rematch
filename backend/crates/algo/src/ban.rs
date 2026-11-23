//! The randomness and clock ban for the maths modules (ADR 0104, issue #53).
//!
//! Every random draw in layers 1 to 3 is mulberry32 seeded from the data
//! (`profile_core::stats::rng`), and time enters as a parameter (`BuildProfileDeps::now`). This
//! scan finds the calls that would break that: an unseeded RNG or a read of the wall clock.

use std::path::Path;

/// The banned calls.
pub const BANNED: [&str; 4] = [
    "rand::",
    "thread_rng",
    "SystemTime::now",
    "OffsetDateTime::now_utc",
];

/// The crates (or modules) scanned, relative to the repository root.
pub const SCANNED: [&str; 4] = [
    "backend/crates/profile-core/src",
    "backend/crates/enrich/src",
    "backend/crates/coaching-input/src",
    "backend/crates/profile/src",
];

/// The legitimate uses: (file, banned call, reason).
pub const ALLOWED: [(&str, &str, &str); 1] = [(
    "backend/crates/profile/src/hook_impl.rs",
    "OffsetDateTime::now_utc",
    "the job scheduler's run_at for the next build-profile job; a queue time, never a fact",
)];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation {
    pub file: String,
    pub line: usize,
    pub call: &'static str,
    pub text: String,
}

/// The code of a line without its `//` comment (doc comments included).
fn code_of(line: &str) -> &str {
    line.find("//").map_or(line, |i| &line[..i])
}

/// Every banned call in the `.rs` files under `dirs` (relative to `root`) that `allowed` does
/// not name.
pub fn scan(
    root: &Path,
    dirs: &[&str],
    allowed: &[(&str, &str, &str)],
) -> anyhow::Result<Vec<Violation>> {
    let mut files = Vec::new();
    for d in dirs {
        crate::files_under(root, d, &mut files)?;
    }
    let mut out = Vec::new();
    for f in files.iter().filter(|f| f.ends_with(".rs")) {
        let text = std::fs::read_to_string(root.join(f))?;
        for (i, line) in text.lines().enumerate() {
            let code = code_of(line);
            for call in BANNED {
                if code.contains(call) && !allowed.iter().any(|(af, ac, _)| af == f && *ac == call)
                {
                    out.push(Violation {
                        file: f.clone(),
                        line: i + 1,
                        call,
                        text: line.trim().to_owned(),
                    });
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_are_not_code() {
        assert_eq!(code_of("let x = 1; // SystemTime::now"), "let x = 1; ");
        assert_eq!(code_of("/// uses OffsetDateTime::now_utc"), "");
    }
}
