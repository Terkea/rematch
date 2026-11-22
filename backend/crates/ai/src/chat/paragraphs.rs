//! Splits streamed chat text into paragraphs. A paragraph ends at a blank line (a line that is
//! empty or holds only spaces, tabs or `\r`); the text after the last blank line is kept as the
//! rest, since more deltas may extend it.

/// The finished paragraphs of `buffer` (trimmed, never empty) and the unfinished rest.
pub fn split_paragraphs(buffer: &str) -> (Vec<String>, String) {
    let mut out = Vec::new();
    let mut start = 0;
    let bytes = buffer.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            // Look for a second newline with only blanks in between.
            let mut j = i + 1;
            while j < bytes.len() && matches!(bytes[j], b' ' | b'\t' | b'\r') {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'\n' {
                push(&mut out, &buffer[start..i]);
                start = j + 1;
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    let rest = &buffer[start..];
    // A rest that is only whitespace after a break carries nothing yet.
    let rest = if start > 0 && rest.trim().is_empty() {
        ""
    } else {
        rest
    };
    (out, rest.to_owned())
}

fn push(out: &mut Vec<String>, p: &str) {
    let p = p.trim();
    if !p.is_empty() {
        out.push(p.to_owned());
    }
}
