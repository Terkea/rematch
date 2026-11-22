//! Where the generate-report job gets its coaching input (Task 18). R2's profile engine serves
//! the real one; M7 wires it. The job validates whatever it gets (`parse_coaching_input`), so a
//! provider never has to be trusted.

use futures::future::BoxFuture;
use serde_json::Value;
use uuid::Uuid;

use crate::input::types::TimeClass;

/// The current `CoachingInput` JSON of (account, time class). `Ok(None)` means there is none
/// yet (too few analysed games); an `Err` is transient (the job retries).
pub trait CoachingInputProvider: Send + Sync {
    fn coaching_input<'a>(
        &'a self,
        account_id: Uuid,
        tc: TimeClass,
    ) -> BoxFuture<'a, anyhow::Result<Option<Value>>>;
}

/// Never has an input: a report request fails as `no_input` until a real provider is wired.
#[derive(Clone, Copy, Debug, Default)]
pub struct NullInputProvider;

impl CoachingInputProvider for NullInputProvider {
    fn coaching_input<'a>(
        &'a self,
        _account_id: Uuid,
        _tc: TimeClass,
    ) -> BoxFuture<'a, anyhow::Result<Option<Value>>> {
        Box::pin(async { Ok(None) })
    }
}
