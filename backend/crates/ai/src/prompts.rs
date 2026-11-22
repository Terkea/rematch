//! Versioned prompts (Task 13). Every prompt is a Markdown file under `prompts/`, named
//! `<id>.v<version>.md`, embedded with `include_str!` and registered in [`PROMPTS`]. A prompt
//! never changes in place: a new wording is a new version file. Each prompt's sha256 and the
//! report prompt set's hash are stored with the generated report (`prompt_hash`), so a report
//! records exactly which copy wrote it.
//!
//! Templates: a placeholder is `{{name}}` with `name` in `[a-z_]+`. Reference tokens such as
//! `{{num:basis.analysedGames|int}}` contain `:` and are never placeholders.

use std::sync::OnceLock;

use sha2::{Digest, Sha256};

/// One embedded prompt file.
#[derive(Debug)]
pub struct Prompt {
    pub id: &'static str,
    pub version: u32,
    pub text: &'static str,
    sha: OnceLock<String>,
}

impl Prompt {
    const fn new(id: &'static str, version: u32, text: &'static str) -> Self {
        Prompt {
            id,
            version,
            text,
            sha: OnceLock::new(),
        }
    }

    /// The sha256 of `text`, lowercase hex, computed once.
    pub fn sha256(&'static self) -> &'static str {
        self.sha
            .get_or_init(|| hex::encode(Sha256::digest(self.text.as_bytes())))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemplateVarProblem {
    /// The template has `{{name}}` but no variable was given for it.
    Missing,
    /// A variable was given that the template does not use.
    Unused,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PromptError {
    #[error("prompt {id}@{version} not found")]
    NotFound { id: String, version: u32 },
    #[error("template variable {name:?}: {problem:?}")]
    TemplateVar {
        name: String,
        problem: TemplateVarProblem,
    },
}

macro_rules! prompt {
    ($id:literal, $v:literal) => {
        Prompt::new(
            $id,
            $v,
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/prompts/",
                $id,
                ".v",
                $v,
                ".md"
            )),
        )
    };
}

static TABLE: [Prompt; 27] = [
    prompt!("core/rules", 1),
    prompt!("core/tokens", 1),
    prompt!("core/tokens", 2),
    prompt!("report/report", 1),
    prompt!("report/output", 1),
    prompt!("report/section-overview", 1),
    prompt!("report/section-weaknesses", 1),
    prompt!("report/section-openings", 1),
    prompt!("report/section-climb_plan", 1),
    prompt!("report/section-areas", 1),
    prompt!("tone/b0", 1),
    prompt!("tone/b1", 1),
    prompt!("tone/b2", 1),
    prompt!("tone/b3", 1),
    prompt!("tone/b4", 1),
    prompt!("tone/b5", 1),
    prompt!("repair/repair", 1),
    prompt!("weekly-review/system", 1),
    prompt!("weekly-review/task", 1),
    prompt!("weekly-review/output", 1),
    prompt!("weakness-change/system", 1),
    prompt!("weakness-change/task", 1),
    prompt!("weakness-change/output", 1),
    prompt!("chat/system", 1),
    prompt!("chat/repair", 1),
    prompt!("chat-summary/system", 1),
    prompt!("chat-summary/task", 1),
];

/// Every prompt file. `tests/prompts.rs` checks this against the files on disk.
pub static PROMPTS: &[Prompt] = &TABLE;

/// The prompt with this id and version.
pub fn load_prompt(id: &str, version: u32) -> Result<&'static Prompt, PromptError> {
    PROMPTS
        .iter()
        .find(|p| p.id == id && p.version == version)
        .ok_or_else(|| PromptError::NotFound {
            id: id.to_owned(),
            version,
        })
}

/// The prompts a report section call can use, in hash order.
pub const REPORT_PROMPT_SET: &[(&str, u32)] = &[
    ("core/rules", 1),
    ("core/tokens", 2),
    ("report/report", 1),
    ("report/output", 1),
    ("tone/b0", 1),
    ("tone/b1", 1),
    ("tone/b2", 1),
    ("tone/b3", 1),
    ("tone/b4", 1),
    ("tone/b5", 1),
    ("report/section-overview", 1),
    ("report/section-weaknesses", 1),
    ("report/section-openings", 1),
    ("report/section-climb_plan", 1),
    ("report/section-areas", 1),
    ("repair/repair", 1),
];

/// The report's `prompt_version` (the AI disclosure and the report row).
pub const REPORT_PROMPT_VERSION: &str = "report@1";

/// The prompts a chat turn uses (Task 23), in hash order: the shared rules block, the chat's
/// prose-output prompt and its repair prompt.
pub const CHAT_PROMPT_SET: &[(&str, u32)] = &[
    ("core/rules", 1),
    ("core/tokens", 2),
    ("chat/system", 1),
    ("chat/repair", 1),
];

/// The chat's prompt version (stored with the chat's AI disclosure).
pub const CHAT_PROMPT_VERSION: &str = "chat@1";

/// The prompts of a thread title and summary (Task 23).
pub const CHAT_SUMMARY_PROMPT_SET: &[(&str, u32)] =
    &[("chat-summary/system", 1), ("chat-summary/task", 1)];

/// The sha256 (lowercase hex) of the set's file hashes, concatenated in order. Panics on an
/// unregistered prompt: sets are constants, and `tests/prompts.rs` loads each one.
pub fn prompt_set_hash(set: &[(&str, u32)]) -> String {
    let mut h = Sha256::new();
    for (id, v) in set {
        let p = load_prompt(id, *v).unwrap_or_else(|e| panic!("prompt set: {e}"));
        h.update(p.sha256().as_bytes());
    }
    hex::encode(h.finalize())
}

/// The placeholder name at the start of `s` (just after `{{`) and its length with `}}`, when
/// `s` starts with `[a-z_]+}}`.
fn placeholder(s: &str) -> Option<(&str, usize)> {
    let n = s
        .bytes()
        .take_while(|b| b.is_ascii_lowercase() || *b == b'_')
        .count();
    (n > 0 && s[n..].starts_with("}}")).then(|| (&s[..n], n + 2))
}

/// Replaces each `{{name}}` with its value, in one pass (values are not re-scanned). A
/// placeholder without a variable, or a variable no placeholder uses, is an error.
pub fn render_template(text: &str, vars: &[(&str, &str)]) -> Result<String, PromptError> {
    let mut used = vec![false; vars.len()];
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("{{") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 2..];
        match placeholder(after) {
            Some((name, len)) => {
                let k = vars.iter().position(|(n, _)| *n == name).ok_or_else(|| {
                    PromptError::TemplateVar {
                        name: name.to_owned(),
                        problem: TemplateVarProblem::Missing,
                    }
                })?;
                used[k] = true;
                out.push_str(vars[k].1);
                rest = &after[len..];
            }
            None => {
                out.push_str("{{");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    if let Some(k) = used.iter().position(|u| !u) {
        return Err(PromptError::TemplateVar {
            name: vars[k].0.to_owned(),
            problem: TemplateVarProblem::Unused,
        });
    }
    Ok(out)
}

/// The prompts a weekly review call uses (Task 15), in hash order: the shared rules block, the
/// weekly prompt, its JSON output instructions, its task and the shared repair prompt.
pub const WEEKLY_PROMPT_SET: &[(&str, u32)] = &[
    ("core/rules", 1),
    ("core/tokens", 2),
    ("weekly-review/system", 1),
    ("weekly-review/output", 1),
    ("weekly-review/task", 1),
    ("repair/repair", 1),
];

/// The weekly review's prompt version (stored with the review).
pub const WEEKLY_PROMPT_VERSION: &str = "weekly@1";

/// The prompts a change-note call uses (Task 15), in hash order.
pub const CHANGE_PROMPT_SET: &[(&str, u32)] = &[
    ("core/rules", 1),
    ("core/tokens", 2),
    ("weakness-change/system", 1),
    ("weakness-change/output", 1),
    ("weakness-change/task", 1),
    ("repair/repair", 1),
];

/// The change note's prompt version (stored with the note).
pub const CHANGE_PROMPT_VERSION: &str = "change@1";
