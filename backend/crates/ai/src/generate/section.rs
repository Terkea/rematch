//! Section generation (Task 14). Until Task 14 lands, this file holds only the two shared types
//! the chat (Task 23) already needs, in the place Task 14 owns them; Task 14 keeps them and adds
//! the rest of the module.

use futures::future::BoxFuture;

use crate::client::port::{AiError, AiUsage};

/// One metered model entry of one call (a call that touched two models gives two records).
#[derive(Clone, Debug, PartialEq)]
pub struct CallRecord {
    pub model: String,
    pub usage: AiUsage,
    pub batch: bool,
    /// The SDK's client-side estimate, for comparison only, never billed (T18 controller note);
    /// on the call's first record only.
    pub sdk_cost_usd: Option<f64>,
}

/// The budget hook, run before every model call.
pub type BeforeCall<'a> = &'a (dyn Fn() -> BoxFuture<'static, Result<(), AiError>> + Send + Sync);
