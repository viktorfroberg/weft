//! Scrollback recorder. Sits alongside the reader thread's flush buffer and
//! accumulates a sanitized transcript of PTY output that we can replay back
//! into xterm.js when the user re-opens a dormant tab.
//!
//! "Sanitized" means:
//! - **Alt-screen segments are dropped.** Bytes emitted while the program is
//!   in alt-screen (vim, less, htop, Claude's TUI) are ephemeral by design —
//!   they don't reflect any persistent state. Replaying them into a fresh
//!   xterm positions the cursor at absolute coords that no longer match.
//! - **Cursor-position and screen-clear CSIs are dropped** (`CUP`, `HVP`,
//!   `ED`, `EL`, `CHA`, `CUU/D/F/B` etc). These rearrange a live screen and
//!   would either clobber the separator we print before replay, or race the
//!   fresh prompt we write after.
//! - **SGR (colors, bold, underline), newlines, tabs, printable bytes pass
//!   through** — the transcript keeps its styling.
//!
//! The parser is `vte` (a vetted VT500 state machine) so an escape sequence
//! split across two `read()` chunks still classifies correctly — a naive
//! per-chunk scan would miss `\x1b[?1049h` straddling a boundary.
//!
//! The recording is a bounded VecDeque<u8>; overflow drops from the front.
//! 512 KiB caps per-tab disk usage while still preserving a few thousand
//! lines of output at typical terminal widths.

use std::collections::VecDeque;
use vte::{Params, Parser, Perform};

/// Max bytes retained per session. Reader output arrives in ≤8 KiB chunks;
/// a single long agent run can blow past this, so overflow drops from the
/// front. Disk persistence uses the same cap by truncating to the ring.
pub const RING_CAP_BYTES: usize = 512 * 1024;

/// Recorder state kept alongside the reader's flush buffer. Feed bytes via
/// `feed(chunk)`; read the final transcript via `finalize()` on child exit.
///
/// Thread-safety: NOT internally synchronized. Callers must hold whatever
/// mutex protects the surrounding reader buffer. We extend the existing
/// reader critical section in `session.rs` rather than add a second lock.
pub struct ScrollbackRecorder {
    parser: Parser,
    sink: RingSink,
}

impl ScrollbackRecorder {
    pub fn new() -> Self {
        Self {
            parser: Parser::new(),
            sink: RingSink::new(RING_CAP_BYTES),
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.parser.advance(&mut self.sink, *b);
        }
    }

    /// Drain the recording into a Vec. If the process died mid-alt-screen,
    /// any in-flight alt-buffered bytes are dropped — we never emit a
    /// partial alt-screen view.
    pub fn finalize(mut self) -> Vec<u8> {
        if self.sink.alt_screen {
            // Process exited while in alt-screen; the alt-buffer content
            // we suppressed isn't coming back, nothing more to do.
            self.sink.alt_screen = false;
        }
        self.sink.buf.drain(..).collect()
    }

    /// Peek the current buffer size (testing + telemetry).
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.sink.buf.len()
    }
}

struct RingSink {
    buf: VecDeque<u8>,
    cap: usize,
    alt_screen: bool,
}

impl RingSink {
    fn new(cap: usize) -> Self {
        Self {
            buf: VecDeque::with_capacity(cap.min(8192)),
            cap,
            alt_screen: false,
        }
    }

    fn push(&mut self, b: u8) {
        if self.alt_screen {
            return;
        }
        if self.buf.len() >= self.cap {
            self.buf.pop_front();
        }
        self.buf.push_back(b);
    }

    fn push_slice(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.push(*b);
        }
    }
}

impl Perform for RingSink {
    fn print(&mut self, c: char) {
        // `char` can encode a 4-byte UTF-8 codepoint. Encode and push.
        let mut tmp = [0u8; 4];
        let s = c.encode_utf8(&mut tmp);
        self.push_slice(s.as_bytes());
    }

    fn execute(&mut self, byte: u8) {
        // C0 controls: keep CR, LF, HT, BS. Drop the rest (BEL, SI, SO,
        // form feed etc. clutter transcript replay).
        match byte {
            b'\r' | b'\n' | b'\t' | 0x08 => self.push(byte),
            _ => {}
        }
    }

    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _c: char) {
        // DCS passthrough start — drop. We never replay DCS sequences.
    }

    fn put(&mut self, _byte: u8) {
        // DCS body — drop.
    }

    fn unhook(&mut self) {
        // DCS end — drop.
    }

    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {
        // OSC (e.g. set window title) — drop. Replaying title changes from
        // a dead process is wrong.
    }

    fn csi_dispatch(
        &mut self,
        params: &Params,
        intermediates: &[u8],
        _ignore: bool,
        c: char,
    ) {
        // Private mode set/reset: `\x1b[?NNNh` / `\x1b[?NNNl`. Alt-screen
        // is 1049 (xterm's preferred, saves cursor too), 1047, or 47.
        if matches!(c, 'h' | 'l') && intermediates == [b'?'] {
            let mut hit_alt = false;
            for p in params.iter() {
                for v in p {
                    if matches!(*v, 47 | 1047 | 1049) {
                        hit_alt = true;
                    }
                }
            }
            if hit_alt {
                self.alt_screen = c == 'h';
                return;
            }
            // Other DEC private modes — drop silently.
            return;
        }

        match c {
            // SGR (colors, bold, underline, reset) — preserve. This is the
            // entire reason the transcript has any visual fidelity.
            'm' => {
                self.push(0x1b);
                self.push(b'[');
                let mut first = true;
                for p in params.iter() {
                    for v in p {
                        if !first {
                            self.push(b';');
                        }
                        first = false;
                        let s = v.to_string();
                        self.push_slice(s.as_bytes());
                    }
                }
                self.push(b'm');
            }
            // Cursor-position + screen-clear family — drop. These clobber
            // replay into a fresh xterm. CUP(H), HVP(f), ED(J), EL(K),
            // CHA(G), CNL(E), CPL(F), VPA(d), CUU(A), CUD(B), CUF(C),
            // CUB(D), DECSTBM(r), RI/DECSC etc.
            'H' | 'f' | 'J' | 'K' | 'G' | 'E' | 'F' | 'd' | 'A' | 'B' | 'C' | 'D' | 'r'
            | 's' | 'u' => {}
            // Everything else (hyperlink setups, scrollback etc.) — drop.
            // Being conservative keeps replay predictable; we can widen
            // the allowlist if users complain about missing features.
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {
        // ESC-only sequences (no CSI): shift in/out, index, reset etc. —
        // all cursor-disturbing, drop.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_printable_and_colors() {
        let mut r = ScrollbackRecorder::new();
        r.feed(b"\x1b[31mhello\x1b[0m\n");
        let out = r.finalize();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("hello"));
        assert!(s.contains("\x1b[31m"));
        assert!(s.contains("\x1b[0m"));
    }

    #[test]
    fn strips_cursor_csi() {
        let mut r = ScrollbackRecorder::new();
        r.feed(b"\x1b[2J\x1b[Hhello\n");
        let out = r.finalize();
        let s = String::from_utf8(out).unwrap();
        assert_eq!(s, "hello\n");
    }

    #[test]
    fn alt_screen_segment_dropped() {
        let mut r = ScrollbackRecorder::new();
        r.feed(b"before\n\x1b[?1049hHIDDEN\x1b[?1049lafter\n");
        let out = r.finalize();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("before"));
        assert!(!s.contains("HIDDEN"));
        assert!(s.contains("after"));
    }

    #[test]
    fn alt_screen_spanning_read_boundary() {
        // Verifies the real motivation for using vte: an escape split
        // across two feed() calls must still be detected.
        let mut r = ScrollbackRecorder::new();
        r.feed(b"seen\n\x1b[?104");
        r.feed(b"9hHIDDEN\x1b[?1049lback\n");
        let out = r.finalize();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("seen"));
        assert!(!s.contains("HIDDEN"));
        assert!(s.contains("back"));
    }

    #[test]
    fn unclosed_alt_screen_on_finalize() {
        // Process killed mid-vim: entered alt-screen but never exited.
        // Finalize should produce only the pre-alt text.
        let mut r = ScrollbackRecorder::new();
        r.feed(b"pre\n\x1b[?1049hTUI content");
        let out = r.finalize();
        let s = String::from_utf8(out).unwrap();
        assert_eq!(s, "pre\n");
    }

    #[test]
    fn ring_drops_oldest() {
        let mut r = ScrollbackRecorder::new();
        // Push more than RING_CAP_BYTES of printable ASCII. Tail should
        // always be the last RING_CAP_BYTES bytes.
        for _ in 0..2 {
            r.feed(&vec![b'x'; RING_CAP_BYTES]);
        }
        assert_eq!(r.len(), RING_CAP_BYTES);
    }
}
