//! JSON logs on stdout, one object per line, each with `"service":"worker"`.
//!
//! `LOG_LEVEL=silent` turns logging off. Otherwise `RUST_LOG` filters (default `info`).

use tracing::Subscriber;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::format::{Format, Json, JsonFields, Writer};
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::registry::LookupSpan;

pub const SERVICE: &str = "worker";

/// Puts `"service":"<name>"` first in every JSON event of the inner formatter.
pub struct WithService<F> {
    service: &'static str,
    inner: F,
}

impl<S, N, F> FormatEvent<S, N> for WithService<F>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
    F: FormatEvent<S, N>,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &tracing::Event<'_>,
    ) -> std::fmt::Result {
        let mut line = String::new();
        self.inner
            .format_event(ctx, Writer::new(&mut line), event)?;
        match line.strip_prefix('{') {
            Some(rest) => write!(writer, "{{\"service\":\"{}\",{rest}", self.service),
            None => writer.write_str(&line),
        }
    }
}

/// The worker's event format: flattened JSON with the service name.
pub fn format() -> WithService<Format<Json>> {
    WithService {
        service: SERVICE,
        inner: tracing_subscriber::fmt::format()
            .json()
            .flatten_event(true)
            .with_current_span(false)
            .with_span_list(false),
    }
}

/// `false` when `LOG_LEVEL` is `silent`.
pub fn enabled(log_level: Option<&str>) -> bool {
    !log_level.is_some_and(|l| l.trim().eq_ignore_ascii_case("silent"))
}

/// Installs the global subscriber (see the module docs). Call once, from `main`.
pub fn init() {
    if !enabled(std::env::var("LOG_LEVEL").ok().as_deref()) {
        return;
    }
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .fmt_fields(JsonFields::new())
        .event_format(format())
        .with_writer(std::io::stdout)
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct Buf(Arc<Mutex<Vec<u8>>>);

    impl io::Write for Buf {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn every_line_is_json_with_the_service() {
        let buf = Buf::default();
        let writer = buf.clone();
        let subscriber = tracing_subscriber::fmt()
            .fmt_fields(JsonFields::new())
            .event_format(format())
            .with_writer(move || writer.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(stockfish_version = "Stockfish 19", "worker ready");
            tracing::info!(ok = true, ms = 12, "shutdown complete");
        });
        let out = String::from_utf8(buf.0.lock().unwrap().clone()).unwrap();
        let lines: Vec<serde_json::Value> = out
            .lines()
            .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("{e}: {l}")))
            .collect();
        assert_eq!(lines.len(), 2, "{out}");
        assert_eq!(lines[0]["service"], "worker");
        assert_eq!(lines[0]["message"], "worker ready");
        assert_eq!(lines[0]["stockfish_version"], "Stockfish 19");
        assert_eq!(lines[1]["ok"], true);
        assert!(out.contains("\"worker ready\""));
    }

    #[test]
    fn silent_turns_logging_off() {
        assert!(!enabled(Some("silent")));
        assert!(!enabled(Some("SILENT")));
        assert!(enabled(Some("info")));
        assert!(enabled(None));
    }
}
