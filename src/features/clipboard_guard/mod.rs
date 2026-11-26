// Clipboard Guard module
//
// Automatically clears the clipboard after a configurable timeout.
// Only clears if the clipboard still contains the copied secret
// (avoids clearing unrelated data copied by other apps).

use std::time::{Duration, Instant};
use zeroize::Zeroizing;

/// Clipboard guard state.
pub struct ClipboardGuard {
    /// Timeout in seconds (0 = disabled).
    timeout_seconds: u32,
    /// Snapshot of the last copied text. Wrapped in `Zeroizing` so the buffer
    /// is wiped on every replacement (not just on struct drop) — important
    /// because `on_copy` may be called many times per session.
    snapshot: Option<Zeroizing<String>>,
    /// When the copy happened.
    copy_time: Option<Instant>,
}

/// Default: 30 seconds.
impl Default for ClipboardGuard {
    fn default() -> Self {
        Self::new(30)
    }
}

impl ClipboardGuard {
    /// Creates a new clipboard guard with the given timeout in seconds.
    pub fn new(timeout_seconds: u32) -> Self {
        Self {
            timeout_seconds,
            snapshot: None,
            copy_time: None,
        }
    }

    /// Disabled clipboard guard.
    pub fn disabled() -> Self {
        Self::new(0)
    }

    /// Record that text was copied to clipboard. Starts the clear timer.
    pub fn on_copy(&mut self, text: &str) {
        if self.timeout_seconds == 0 {
            return;
        }
        self.snapshot = Some(Zeroizing::new(text.to_string()));
        self.copy_time = Some(Instant::now());
    }

    /// Check if it's time to clear the clipboard.
    /// Returns the snapshot text if the clipboard should be cleared.
    pub fn should_clear(&self) -> Option<&str> {
        if self.timeout_seconds == 0 {
            return None;
        }
        if let (Some(snapshot), Some(copy_time)) = (&self.snapshot, &self.copy_time) {
            let timeout = Duration::from_secs(self.timeout_seconds as u64);
            if copy_time.elapsed() >= timeout {
                return Some(snapshot.as_str());
            }
        }
        None
    }

    /// Mark the clipboard as cleared. Drops the `Zeroizing<String>`, which
    /// wipes the heap buffer before releasing it. Don't just set to `None`
    /// without the wrapper or the underlying bytes linger until the
    /// allocator reuses them.
    pub fn mark_cleared(&mut self) {
        self.snapshot = None; // drops the Zeroizing wrapper → buffer wiped
        self.copy_time = None;
    }

    /// Get the timeout in seconds.
    pub fn timeout_seconds(&self) -> u32 {
        self.timeout_seconds
    }

    /// Set a new timeout (0 = disabled).
    pub fn set_timeout(&mut self, seconds: u32) {
        self.timeout_seconds = seconds;
        if seconds == 0 {
            self.mark_cleared();
        }
    }

    /// Timer interval in milliseconds for the UI timer, or None if disabled.
    pub fn timer_interval_ms(&self) -> Option<u32> {
        if self.timeout_seconds == 0 {
            None
        } else {
            Some(self.timeout_seconds * 1000)
        }
    }
}

// No manual Drop needed — Zeroizing<String> handles the wipe. Leaving
// a hand-rolled Drop here would be redundant and (worse) easy to get
// wrong after a future refactor reshuffles the field order.

/// Tell Windows to keep this clipboard entry out of the Clipboard History
/// (Win+V) and Cloud Clipboard sync. Implemented via the well-known
/// undocumented format `ExcludeClipboardContentFromMonitorProcessing`:
/// registering it and writing a single zero byte under that format tells
/// the cbdhsvc_* service to skip the current entry.
///
/// Call immediately after `set_clipboard(formats::Unicode, secret)` — must
/// be done in a fresh OpenClipboard session since clipboard-win closes the
/// session when its helper returns.
#[cfg(windows)]
pub fn exclude_current_clipboard_entry_from_history() {
    use std::ptr;
    unsafe {
        let fmt_name: Vec<u16> = "ExcludeClipboardContentFromMonitorProcessing"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let fmt = windows::Win32::System::DataExchange::RegisterClipboardFormatW(
            windows::core::PCWSTR(fmt_name.as_ptr()),
        );
        // Each failure is logged: the copy itself succeeded, but the secret
        // may then reach the clipboard history (issue #31: no silent result).
        if fmt == 0 {
            eprintln!("WARN clipboard: history exclusion format not registered");
            return;
        }
        if let Err(e) = windows::Win32::System::DataExchange::OpenClipboard(None) {
            eprintln!("WARN clipboard: history exclusion, OpenClipboard failed err={:?}", e);
            return;
        }
        // Writing a NULL handle under this format is the documented pattern;
        // the call then returns NULL, which is only a failure with an error
        // code set.
        if let Err(e) = windows::Win32::System::DataExchange::SetClipboardData(
            fmt,
            Some(windows::Win32::Foundation::HANDLE(ptr::null_mut())),
        ) {
            if e.code().is_err() {
                eprintln!("WARN clipboard: history exclusion, SetClipboardData failed err={:?}", e);
            }
        }
        if let Err(e) = windows::Win32::System::DataExchange::CloseClipboard() {
            eprintln!("WARN clipboard: history exclusion, CloseClipboard failed err={:?}", e);
        }
    }
}

#[cfg(not(windows))]
pub fn exclude_current_clipboard_entry_from_history() {}

/// What the auto-clear does with what it read from the clipboard.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClearStep {
    /// Still our copy: empty the clipboard.
    Clear,
    /// The clipboard could not be read (another application holds it): keep
    /// the guard armed and try again at the next tick.
    Retry,
    /// The user copied something else: nothing to clear.
    Done,
}

/// Why the clipboard text could not be read.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReadFailure {
    /// No text on the clipboard (an image, files, nothing): someone else's
    /// content, our secret is gone.
    NotText,
    /// Text is there but the clipboard could not be opened (another
    /// application holds it).
    Busy,
}

/// The auto-clear decision (pass 3: a failed read disarmed the guard and
/// left the secret on the clipboard; pass 4: "no text" is not "busy").
pub fn clear_step(read: Result<&str, ReadFailure>, snapshot: &str) -> ClearStep {
    match read {
        Ok(current) if current == snapshot => ClearStep::Clear,
        Ok(_) | Err(ReadFailure::NotText) => ClearStep::Done,
        Err(ReadFailure::Busy) => ClearStep::Retry,
    }
}

/// Whether the `failures`-th failed read in a row is logged: the first, then
/// one in 60 (the timer ticks every 5 s), so a held clipboard does not fill
/// the log.
pub fn warn_read_failure(failures: u32) -> bool {
    failures == 1 || (failures > 0 && failures % 60 == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_read_keeps_the_guard_armed() {
        assert_eq!(clear_step(Ok("secret"), "secret"), ClearStep::Clear);
        assert_eq!(clear_step(Ok("something else"), "secret"), ClearStep::Done);
        assert_eq!(clear_step(Err(ReadFailure::Busy), "secret"), ClearStep::Retry);
    }

    /// Pass 4: an image or files on the clipboard is someone else's
    /// content: done, not retried forever.
    #[test]
    fn a_clipboard_without_text_is_done() {
        assert_eq!(clear_step(Err(ReadFailure::NotText), "secret"), ClearStep::Done);
    }

    #[test]
    fn read_failures_are_logged_without_spam() {
        assert!(warn_read_failure(1));
        assert!(!warn_read_failure(2));
        assert!(!warn_read_failure(59));
        assert!(warn_read_failure(60));
        assert!(!warn_read_failure(61));
    }

    #[test]
    fn default_is_30_seconds() {
        let cg = ClipboardGuard::default();
        assert_eq!(cg.timeout_seconds(), 30);
    }

    #[test]
    fn disabled_ignores_copy() {
        let mut cg = ClipboardGuard::disabled();
        cg.on_copy("secret");
        assert!(cg.should_clear().is_none());
    }

    #[test]
    fn fresh_copy_not_ready_to_clear() {
        let mut cg = ClipboardGuard::new(60);
        cg.on_copy("my-password");
        // Just copied, shouldn't clear yet
        assert!(cg.should_clear().is_none());
    }

    #[test]
    fn mark_cleared_resets() {
        let mut cg = ClipboardGuard::new(1);
        cg.on_copy("secret");
        cg.mark_cleared();
        assert!(cg.should_clear().is_none());
    }

    #[test]
    fn set_timeout_to_zero_clears() {
        let mut cg = ClipboardGuard::new(30);
        cg.on_copy("data");
        cg.set_timeout(0);
        assert!(cg.should_clear().is_none());
        assert_eq!(cg.timeout_seconds(), 0);
    }

    #[test]
    fn timer_interval() {
        let cg = ClipboardGuard::new(30);
        assert_eq!(cg.timer_interval_ms(), Some(30_000));
        let cg = ClipboardGuard::disabled();
        assert_eq!(cg.timer_interval_ms(), None);
    }
}
