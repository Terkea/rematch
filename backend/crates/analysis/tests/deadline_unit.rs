use std::time::Duration;

use analysis::deadline::{ANALYSE_DEADLINE, Deadline, IMPORT_IDLE_DEADLINE, JobDeadlineError};

#[test]
fn deadline_constants_and_message() {
    assert_eq!(ANALYSE_DEADLINE, Duration::from_secs(600));
    assert_eq!(IMPORT_IDLE_DEADLINE, Duration::from_secs(1800));
    let e = JobDeadlineError {
        kind: "analyse-game",
        ms: 50,
    };
    assert_eq!(e.to_string(), "analyse-game exceeded its 50 ms deadline");
}

#[tokio::test]
async fn deadline_cancels_its_token() {
    let d = Deadline::new("analyse-game", Duration::from_millis(50));
    assert!(d.expired().is_none());
    tokio::time::timeout(Duration::from_secs(2), d.token().cancelled())
        .await
        .expect("token cancelled");
    assert_eq!(
        d.expired(),
        Some(JobDeadlineError {
            kind: "analyse-game",
            ms: 50
        })
    );
}

#[tokio::test]
async fn reset_postpones_the_deadline() {
    let d = Deadline::new("import-account", Duration::from_millis(1000));
    let token = d.token();
    for _ in 0..4 {
        tokio::time::sleep(Duration::from_millis(300)).await;
        d.reset();
        assert!(!token.is_cancelled(), "reset must keep postponing");
    }
    tokio::time::timeout(Duration::from_secs(3), token.cancelled())
        .await
        .expect("fires after resets stop");
    assert!(d.expired().is_some());
}
