use crate::TimeClass;

/// Classify by estimated duration: base + 40 * increment seconds. Never `Daily`.
pub fn classify_time_control(base_seconds: u32, increment_seconds: u32) -> TimeClass {
    let estimated = u64::from(base_seconds) + 40 * u64::from(increment_seconds);
    if estimated < 180 {
        TimeClass::Bullet
    } else if estimated < 480 {
        TimeClass::Blitz
    } else if estimated < 1500 {
        TimeClass::Rapid
    } else {
        TimeClass::Classical
    }
}
