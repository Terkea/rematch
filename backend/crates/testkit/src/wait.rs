use std::future::Future;
use std::time::Duration;

/// Polls `f` every `interval` until it returns `Some`, and returns that value.
/// Panics with "timed out waiting for {what}" after `timeout`.
pub async fn wait_for<T, F, Fut>(what: &str, timeout: Duration, interval: Duration, mut f: F) -> T
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Option<T>>,
{
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Some(value) = f().await {
            return value;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!("timed out waiting for {what}");
        }
        tokio::time::sleep(interval).await;
    }
}
