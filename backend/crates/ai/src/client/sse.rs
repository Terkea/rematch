//! A server-sent-events decoder (the WHATWG framing rules), fed raw body chunks.
//!
//! Lines end with LF or CRLF; a chunk may split a line, a CRLF pair or a UTF-8 sequence anywhere,
//! because bytes are buffered until a whole line arrives. `:` lines are comments; `id` and
//! `retry` are ignored. A blank line dispatches the pending event when at least one `data` field
//! was seen (an empty `data:` still counts). Multi-line data is joined with `\n`.

/// One dispatched event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SseEvent {
    /// The `event` field, if any.
    pub event: Option<String>,
    pub data: String,
}

#[derive(thiserror::Error, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SseError {
    /// The body ended inside an event (no closing blank line) or inside a line.
    #[error("the event stream ended inside an event")]
    TrailingPartialEvent,
}

/// The longest line, and the most data in one event, the decoder buffers (4 MiB). Beyond it the
/// decoder stops and [`SseDecoder::overflowed`] is true.
pub const MAX_LINE: usize = 4 * 1024 * 1024;

#[derive(Debug)]
pub struct SseDecoder {
    buf: Vec<u8>,
    /// Bytes of `buf` already searched for a newline (the scan is incremental).
    scanned: usize,
    event: Option<String>,
    data: String,
    has_data: bool,
    max: usize,
    overflow: bool,
}

impl Default for SseDecoder {
    fn default() -> Self {
        SseDecoder::with_max_line(MAX_LINE)
    }
}

impl SseDecoder {
    /// A decoder whose line and event-data limit is `max` bytes.
    pub fn with_max_line(max: usize) -> Self {
        SseDecoder {
            buf: Vec::new(),
            scanned: 0,
            event: None,
            data: String::new(),
            has_data: false,
            max,
            overflow: false,
        }
    }

    /// True once a line or an event's data exceeded the limit; nothing more is decoded.
    pub fn overflowed(&self) -> bool {
        self.overflow
    }

    /// Feeds one chunk and returns every event it completes.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<SseEvent> {
        let mut out = Vec::new();
        if self.overflow {
            return out;
        }
        self.buf.extend_from_slice(chunk);
        let mut start = 0;
        while let Some(off) = self.buf[self.scanned..].iter().position(|&b| b == b'\n') {
            let end = self.scanned + off;
            let mut line = &self.buf[start..end];
            if let [rest @ .., b'\r'] = line {
                line = rest;
            }
            let line = String::from_utf8_lossy(line).into_owned();
            start = end + 1;
            self.scanned = start;
            if line.len() > self.max {
                self.overflow = true;
            } else if let Some(ev) = self.line(&line) {
                out.push(ev);
            }
            if self.data.len() > self.max {
                self.overflow = true;
            }
            if self.overflow {
                self.buf.clear();
                self.scanned = 0;
                self.reset();
                return out;
            }
        }
        self.buf.drain(..start);
        self.scanned = self.buf.len();
        if self.buf.len() > self.max {
            self.overflow = true;
            self.buf.clear();
            self.scanned = 0;
            self.reset();
        }
        out
    }

    /// Ends the stream: an unterminated line or event is an error.
    pub fn finish(&mut self) -> Result<(), SseError> {
        let partial = !self.buf.is_empty() || self.has_data || self.event.is_some();
        self.buf.clear();
        self.scanned = 0;
        self.reset();
        if partial {
            Err(SseError::TrailingPartialEvent)
        } else {
            Ok(())
        }
    }

    fn reset(&mut self) {
        self.event = None;
        self.data.clear();
        self.has_data = false;
    }

    fn line(&mut self, line: &str) -> Option<SseEvent> {
        if line.is_empty() {
            let ev = self.has_data.then(|| {
                let mut data = std::mem::take(&mut self.data);
                if data.ends_with('\n') {
                    data.pop();
                }
                SseEvent {
                    event: self.event.take(),
                    data,
                }
            });
            self.reset();
            return ev;
        }
        if line.starts_with(':') {
            return None;
        }
        let (field, value) = match line.split_once(':') {
            Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
            None => (line, ""),
        };
        match field {
            "event" => self.event = Some(value.to_owned()),
            "data" => {
                self.data.push_str(value);
                self.data.push('\n');
                self.has_data = true;
            }
            _ => {}
        }
        None
    }
}
