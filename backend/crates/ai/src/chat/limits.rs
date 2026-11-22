//! The chat's limits, the allowance check before a turn, and the fixed server copy.
//!
//! The copy is server text, never model output: it is shown as it is (one text segment), is not
//! validated as token text, and carries no upsell (spec §14).

use std::time::Duration;

use time::OffsetDateTime;

pub use super::execute::ENGINE_WAIT_INLINE;
use crate::entitlements::{Plan, Quota, QuotaWindow, chat_quotas, exhausted};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChatLimits {
    /// Tool rounds per turn; the SDK's `maxTurns` is this plus one (the answering round).
    pub max_tool_rounds: u32,
    pub max_question_chars: usize,
    pub max_turns_per_conversation: u32,
    pub max_tokens: u32,
    /// Turns replayed verbatim before the history rolls over into a summary.
    pub replay_turns: u32,
}

pub const CHAT_LIMITS: ChatLimits = ChatLimits {
    max_tool_rounds: 6,
    max_question_chars: 2000,
    max_turns_per_conversation: 40,
    max_tokens: 8000,
    replay_turns: 6,
};

/// The longest a tool call waits for an engine check in all (inline wait plus `wait`). It sits
/// under the tool endpoint's 50 s per-call timeout (ADR 0062 timeout ordering).
pub const ENGINE_WAIT_MAX: Duration = Duration::from_secs(45);

/// Why a turn is not started.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DenyReason {
    WeeklyCap,
    DailyCap,
    MonthlyCap,
    ConversationFull,
    TooLong,
}

/// Whether a new turn may start: the question length, the conversation size, then the plan's
/// chat quotas (`entitlements::chat_quotas`; `used(window)` is the count already used in that
/// window). A quota denial carries its reset time.
pub fn check_chat_allowance(
    plan: Plan,
    used: impl Fn(QuotaWindow) -> u32,
    turns_in_conversation: u32,
    question_chars: usize,
    now: OffsetDateTime,
) -> Result<(), (DenyReason, Option<OffsetDateTime>)> {
    check_chat_allowance_with(
        chat_quotas(plan),
        used,
        turns_in_conversation,
        question_chars,
        now,
    )
}

/// [`check_chat_allowance`] with the quotas given (the job passes
/// `Entitlements::chat_quotas`, so the beta limits come from the environment).
pub fn check_chat_allowance_with(
    quotas: &[Quota],
    used: impl Fn(QuotaWindow) -> u32,
    turns_in_conversation: u32,
    question_chars: usize,
    now: OffsetDateTime,
) -> Result<(), (DenyReason, Option<OffsetDateTime>)> {
    if question_chars > CHAT_LIMITS.max_question_chars {
        return Err((DenyReason::TooLong, None));
    }
    if turns_in_conversation >= CHAT_LIMITS.max_turns_per_conversation {
        return Err((DenyReason::ConversationFull, None));
    }
    match exhausted(quotas, used, now) {
        None => Ok(()),
        Some((q, resets_at)) => {
            let reason = match q.window {
                QuotaWindow::Day => DenyReason::DailyCap,
                QuotaWindow::Week => DenyReason::WeeklyCap,
                QuotaWindow::Month => DenyReason::MonthlyCap,
            };
            Err((reason, Some(resets_at)))
        }
    }
}

// --- Fixed copy ---

pub const CHAT_DROPPED_NOTE: &str =
    "I removed {n} statement(s) I couldn't check against your games.";
pub const CHAT_FALLBACK_TEXT: &str = "I couldn't check that answer against your games, so I'd rather not guess. Try asking about a specific game, move or position.";
pub const CHAT_REFUSAL_TEXT: &str =
    "I can't help with that one. Ask me about your games, openings or weaknesses.";
pub const CHAT_OFF_TOPIC_TEXT: &str = "I can only help with chess and your own games. Ask me about a game, a position, an opening or one of your weaknesses.";
pub const CHAT_NO_DATA_TEXT: &str =
    "Your games are still being analysed. Ask me again once your first report is ready.";
pub const CHAT_UNAVAILABLE_TEXT: &str =
    "Your coach is unavailable right now. Please try again later.";
pub const CHAT_ENGINE_PENDING_TEXT: &str = "Checking that position with the engine, one moment.";
pub const ENGINE_LIMIT_NOTE: &str =
    "You've used all your engine checks for this {period}. They reset {reset}.";

/// [`CHAT_DROPPED_NOTE`] for `n` dropped statements.
pub fn dropped_note(n: u32) -> String {
    CHAT_DROPPED_NOTE.replace("{n}", &n.to_string())
}

/// [`ENGINE_LIMIT_NOTE`] for the exhausted window. Engine quotas are daily or weekly; a monthly
/// window is rendered for completeness.
pub fn engine_limit_note(window: QuotaWindow) -> String {
    let (period, reset) = match window {
        QuotaWindow::Day => ("day", "tomorrow"),
        QuotaWindow::Week => ("week", "on Monday"),
        QuotaWindow::Month => ("month", "at the start of next month"),
    };
    ENGINE_LIMIT_NOTE
        .replace("{period}", period)
        .replace("{reset}", reset)
}

/// `CHAT_CAP_TEXT`: what the player sees when a turn is denied.
pub fn chat_cap_text(reason: DenyReason) -> &'static str {
    match reason {
        DenyReason::WeeklyCap => "You've used this week's coach messages. They reset on Monday.",
        DenyReason::DailyCap => "You've used today's coach messages. They reset tomorrow.",
        DenyReason::MonthlyCap => {
            "You've used this month's coach messages. They reset at the start of next month."
        }
        DenyReason::ConversationFull => "This conversation is full. Start a new one to keep going.",
        DenyReason::TooLong => "That question is too long. Please shorten it.",
    }
}
