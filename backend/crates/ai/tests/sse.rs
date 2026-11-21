//! The SSE decoder (reused from the paused Task 10 WIP, b36215f). The sidecar's SSE events are
//! tested in `sidecar_client.rs`; these cover framing only. Every input is a hand-written fixture
//! in `backend/fixtures/ai/api/` or an inline string; nothing touches the network.

use ai::client::sse::{SseDecoder, SseError, SseEvent};
use ai::testing::fixtures_dir;
use serde_json::Value;

fn fixture(name: &str) -> Vec<u8> {
    let path = fixtures_dir().join("api").join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn decode_all(bytes: &[u8]) -> Vec<SseEvent> {
    let mut d = SseDecoder::default();
    let events = d.push(bytes);
    d.finish().expect("no trailing partial event");
    events
}

fn ev(event: Option<&str>, data: &str) -> SseEvent {
    SseEvent {
        event: event.map(str::to_owned),
        data: data.to_owned(),
    }
}

// ---- decoder ----

#[test]
fn decodes_events_across_arbitrary_chunk_boundaries() {
    let bytes = fixture("sse-tool-use.sse");
    let one_shot = decode_all(&bytes);
    assert_eq!(
        one_shot.len(),
        13,
        "every event of the fixture, the comment skipped"
    );
    for size in 1..=64 {
        let mut d = SseDecoder::default();
        let mut events = Vec::new();
        for chunk in bytes.chunks(size) {
            events.extend(d.push(chunk));
        }
        d.finish().expect("clean end");
        assert_eq!(events, one_shot, "split size {size}");
    }
}

#[test]
fn crlf_lines_decode_like_lf() {
    let lf = fixture("sse-tool-use.sse");
    let crlf = String::from_utf8(lf.clone()).unwrap().replace('\n', "\r\n");
    let want = decode_all(&lf);
    for size in [1, 2, 3, 7, 64] {
        let mut d = SseDecoder::default();
        let mut events = Vec::new();
        for chunk in crlf.as_bytes().chunks(size) {
            events.extend(d.push(chunk));
        }
        d.finish().expect("clean end");
        assert_eq!(events, want, "split size {size}");
    }
}

#[test]
fn split_utf8_is_reassembled() {
    let bytes = fixture("sse-split-utf8.bin");
    let at = bytes
        .iter()
        .position(|&b| b == 0xC3)
        .expect("an é in the fixture")
        + 1;
    let mut d = SseDecoder::default();
    assert!(d.push(&bytes[..at]).is_empty());
    let events = d.push(&bytes[at..]);
    d.finish().expect("clean end");
    assert_eq!(events.len(), 1);
    let data: Value = serde_json::from_str(&events[0].data).unwrap();
    assert_eq!(data["delta"]["text"], "café");
    assert_eq!(events[0].event.as_deref(), Some("content_block_delta"));
}

#[test]
fn multi_line_data_comments_and_valueless_fields() {
    let input = ": hello\nevent: x\ndata: a\ndata:b\ndata\nid: 7\nretry: 10\n\ndata: only\n\n";
    assert_eq!(
        decode_all(input.as_bytes()),
        vec![ev(Some("x"), "a\nb\n"), ev(None, "only")]
    );
}

#[test]
fn a_blank_line_without_data_dispatches_nothing() {
    assert!(decode_all(b"\n\nevent: lonely\n\n: c\n\n").is_empty());
    // An empty `data:` is still an event, with empty data.
    assert_eq!(decode_all(b"data:\n\n"), vec![ev(None, "")]);
}

#[test]
fn trailing_partial_event_is_an_error() {
    let mut d = SseDecoder::default();
    assert!(d.push(b"data: {\"type\":\"ping\"}\n").is_empty());
    assert_eq!(d.finish(), Err(SseError::TrailingPartialEvent));

    let mut d = SseDecoder::default();
    assert!(d.push(b"data: {\"type\":\"pi").is_empty());
    assert_eq!(d.finish(), Err(SseError::TrailingPartialEvent));
}
