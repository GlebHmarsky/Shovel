//! A pipe worker publishes complete, redacted lines and wakes the UI once per unread batch.
use shovel_core::process::OutputCallback;
use std::sync::{Arc, Mutex};

const DISPLAY_LIMIT: usize = 32768;

#[derive(Default)]
pub(crate) struct OutputStream {
    capture: Mutex<Capture>,
    pending: Mutex<String>,
}
impl OutputStream {
    pub(crate) fn callback(
        self: &Arc<Self>,
        notify: impl Fn(Arc<Self>) + Send + Sync + 'static,
    ) -> OutputCallback {
        let stream = self.clone();
        Arc::new(move |stderr, bytes| {
            let notify_needed = {
                // Only pipe workers take this lock. The UI takes pending deltas
                // without waiting for decoding or redaction of a long line.
                let mut capture = stream.capture.lock().unwrap();
                let previous = capture.text.len();
                capture.append(stderr, bytes);
                let added = &capture.text[previous..];
                if added.is_empty() {
                    return;
                }
                let mut pending = stream.pending.lock().unwrap();
                let was_empty = pending.is_empty();
                pending.push_str(added);
                was_empty
            };
            if notify_needed {
                notify(stream.clone());
            }
        })
    }
    pub(crate) fn take_pending(&self) -> String {
        std::mem::take(&mut *self.pending.lock().unwrap())
    }
    // The final event is authoritative even when the view was closed or had
    // not polled while the command ran. Native process completion joins both pipes.
    pub(crate) fn finish(&self, complete: bool, detail: &str) -> String {
        let mut capture = self.capture.lock().unwrap();
        capture.finish(complete);
        if !detail.is_empty() {
            if !capture.text.is_empty() && !capture.text.ends_with(['\r', '\n']) {
                capture.publish(b"\n");
            }
            capture.publish(detail.as_bytes());
        }
        std::mem::take(&mut capture.text)
    }
}

/// Redact complete lines, preserving independent stdout/stderr chunk boundaries.
/// Never expose an incomplete bounded line: a URL credential may span chunks.
#[derive(Default)]
struct Capture {
    lines: [Vec<u8>; 2],
    captured: usize,
    overflow: bool,
    text: String,
}
impl Capture {
    pub(crate) fn append(&mut self, stderr: bool, bytes: &[u8]) {
        if self.text.len() == DISPLAY_LIMIT {
            return;
        }
        let room = (1024 * 1024usize).saturating_sub(self.captured);
        self.overflow |= bytes.len() > room;
        let bytes = &bytes[..bytes.len().min(room)];
        self.captured += bytes.len();
        let pending = &mut self.lines[usize::from(stderr)];
        let previous = pending.len();
        pending.extend_from_slice(bytes);
        if let Some(end) = bytes.iter().rposition(|b| matches!(b, b'\r' | b'\n')) {
            let complete: Vec<_> = pending.drain(..=previous + end).collect();
            self.publish(&complete);
        }
    }
    fn publish(&mut self, bytes: &[u8]) {
        if bytes.is_empty() || self.text.len() == DISPLAY_LIMIT {
            return;
        }
        let redacted = shovel_core::network::redact(&String::from_utf8_lossy(bytes));
        let room = DISPLAY_LIMIT.saturating_sub(self.text.len());
        let mut end = redacted.len().min(room);
        while !redacted.is_char_boundary(end) {
            end -= 1;
        }
        self.text.push_str(&redacted[..end]);
    }
    pub(crate) fn finish(&mut self, complete: bool) {
        if complete && !self.overflow {
            for index in 0..2 {
                let bytes = std::mem::take(&mut self.lines[index]);
                self.publish(&bytes);
            }
        }
        self.lines = Default::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unread_pipe_output_coalesces_and_the_ui_only_receives_new_redacted_text() {
        let stream = Arc::new(OutputStream::default());
        let (tx, rx) = std::sync::mpsc::channel();
        let output = stream.callback(move |stream| tx.send(stream).unwrap());
        for byte in b"https://alice:private@hostname/path?token=secret" {
            output(true, &[*byte]);
        }
        assert!(rx.try_recv().is_err());
        output(false, b"working\r");
        output(true, b"\n");
        for _ in 0..3000 {
            output(false, b"line\n");
        }
        let first = rx.try_recv().unwrap().take_pending();
        assert!(rx.try_recv().is_err());
        assert!(first.starts_with("working\rhttps://[redacted]@hostname/path?[redacted]\n"));
        assert_eq!(first.matches("line\n").count(), 3000);
        output(true, b"second\n");
        assert_eq!(rx.try_recv().unwrap().take_pending(), "second\n");
        output(false, b"final line");
        assert!(rx.try_recv().is_err());
        let complete = stream.finish(true, "https://bob:other@host/log?q=secret");
        assert!(
            complete.starts_with(&first)
                && complete.ends_with("second\nfinal line\nhttps://[redacted]@host/log?[redacted]")
        );
        assert!(!complete.contains("secret") && !complete.contains("private"));
    }

    #[test]
    fn reaching_the_display_limit_stops_progress_notifications_but_preserves_utf8() {
        let stream = Arc::new(OutputStream::default());
        let (tx, rx) = std::sync::mpsc::channel();
        let output = stream.callback(move |stream| tx.send(stream).unwrap());
        let line = "é".repeat(DISPLAY_LIMIT / 2 + 7) + "\n";
        output(false, line.as_bytes());
        let prefix = rx.try_recv().unwrap().take_pending();
        assert_eq!(prefix.len(), DISPLAY_LIMIT);
        for _ in 0..3000 {
            output(true, b"more\n");
        }
        assert!(rx.try_recv().is_err());
        assert_eq!(stream.finish(false, ""), prefix);
    }

    #[test]
    fn streamed_activity_redacts_chunked_urls_and_never_publishes_truncated_lines() {
        let mut capture = Capture::default();
        capture.append(true, b"failure https://user:sec");
        assert!(capture.text.is_empty());
        capture.append(true, b"ret@host/path?token=hidden\n");
        assert_eq!(
            capture.text,
            "failure https://[redacted]@host/path?[redacted]\n"
        );
        capture.append(false, "completed \u{2713}".as_bytes());
        capture.finish(true);
        assert!(capture.text.ends_with("completed \u{2713}"));
        assert!(!capture.text.contains("secret"));
        let mut overflow = Capture::default();
        overflow.append(true, b"https://user:");
        overflow.append(true, &vec![b'x'; 1024 * 1024]);
        overflow.finish(true);
        assert!(overflow.text.is_empty());
        assert!(overflow.lines.iter().all(Vec::is_empty));
        let mut interrupted = Capture::default();
        interrupted.append(false, b"progress\nhttps://fixture:unfinished");
        interrupted.finish(false);
        assert_eq!(interrupted.text, "progress\n");
    }
}
