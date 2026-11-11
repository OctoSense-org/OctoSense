//! A Mac's "quit unexpectedly" alert. An app that quits without being asked
//! to (its process died, its in-process module panicked) takes its window
//! with it, the menu bar passes to the app now in front, and one alert says
//! what happened, with Reopen, Report… and Ignore, as macOS does (ADR 0015).
//! Report… shows what the shell knows of why ([`Report`]): an in-process
//! app's error, or a process's exit and the last of what it wrote, or that
//! nothing was recorded. The other styles keep the closed window with its
//! Reopen (ADR 0004 §2).
//!
//! The shell queues an alert ([`push`]) and reopens what the person asked
//! for ([`take_reopen`]). The approvals overlay draws the alert in front and
//! takes the answer ([`answer`]; approvals/mac_view.rs). One alert at a
//! time, oldest first, one to an app.

use std::cell::RefCell;

use crate::desktop_layout::DesktopWindow;

/// One app that quit.
#[derive(Clone, Debug)]
pub struct Alert {
    /// Its id, as the shell launches it.
    pub app: String,
    /// Its name, as the menu bar showed it.
    pub label: String,
    /// The window it had: Reopen opens the new one there.
    pub window: Option<DesktopWindow>,
    /// What Report… shows.
    pub report: Report,
}

/// Why an app quit, as far as the shell knows: what happened in one
/// sentence, and the app's own words (its error, or the end of its output)
/// as it wrote them, a home folder shown as `~`. No lines: nothing was
/// recorded, and the report says so.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub summary: String,
    pub lines: Vec<String>,
}

/// The most of an app's own words a report shows: the last of them.
pub const REPORT_LINES: usize = 12;

/// The last `REPORT_LINES` lines of `text` that say something, each with its
/// trailing space trimmed and a home folder shown as `~`.
pub fn last_lines(text: &str) -> Vec<String> {
    let lines: Vec<String> = text.lines().map(|l| home_masked(l.trim_end())).filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(REPORT_LINES)..].to_vec()
}

/// `line` with each home folder in it (`/Users/<name>`, `/home/<name>`, where
/// a path starts) as `~`.
pub fn home_masked(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < line.len() {
        let rest = &line[i..];
        let starts = i == 0 || !matches!(line.as_bytes()[i - 1], b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' | b'.' | b'_' | b'-' | b'~');
        if let Some(home) = ["/Users/", "/home/"].iter().find(|home| starts && rest.starts_with(**home)) {
            out.push('~');
            let name = &rest[home.len()..];
            i += home.len() + name.find('/').unwrap_or(name.len());
            continue;
        }
        let ch = rest.chars().next().unwrap_or(' ');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// The end of a client's log: its last 16 KiB, as text.
pub fn read_tail(path: &std::path::Path) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(16 * 1024))).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

#[derive(Default)]
struct Queue {
    up: Vec<Alert>,
    reopen: Vec<Alert>,
    /// The alert in front shows its report.
    reporting: bool,
    generation: u64,
}

thread_local! {
    static QUEUE: RefCell<Queue> = RefCell::new(Queue::default());
}

fn with<R>(f: impl FnOnce(&mut Queue) -> R) -> R {
    QUEUE.with(|queue| f(&mut queue.borrow_mut()))
}

/// An app quit: its alert, behind any already up. Another one for the same
/// app takes the place of the first, which said the same.
pub fn push(alert: Alert) {
    with(|q| {
        if q.up.first().is_some_and(|a| a.app == alert.app) {
            q.reporting = false;
        }
        q.up.retain(|a| a.app != alert.app);
        q.up.push(alert);
        q.generation += 1;
    });
}

/// The alert in front.
pub fn front() -> Option<Alert> {
    with(|q| q.up.first().cloned())
}

/// Report… on the alert in front: it shows its report until it is answered.
pub fn show_report() {
    with(|q| {
        if !q.up.is_empty() && !q.reporting {
            q.reporting = true;
            q.generation += 1;
        }
    });
}

/// Whether the alert in front shows its report.
pub fn reporting() -> bool {
    with(|q| q.reporting && !q.up.is_empty())
}

/// The person's answer to the alert in front: Reopen, or Ignore.
pub fn answer(reopen: bool) {
    with(|q| {
        if q.up.is_empty() {
            return;
        }
        let alert = q.up.remove(0);
        q.reporting = false;
        if reopen {
            q.reopen.push(alert);
        }
        q.generation += 1;
    });
}

/// What the person asked to reopen since the shell last looked.
pub fn take_reopen() -> Vec<Alert> {
    with(|q| std::mem::take(&mut q.reopen))
}

/// Changes with every alert and every answer: the overlay redraws.
pub fn generation() -> u64 {
    with(|q| q.generation)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alert(app: &str) -> Alert {
        Alert { app: app.into(), label: app.to_uppercase(), window: None, report: Report::default() }
    }

    #[test]
    fn report_shows_the_last_lines_that_say_something_and_ends_with_the_answer() {
        let text = (1..=20).map(|i| format!("line {i}  ")).collect::<Vec<_>>().join("\n\n");
        let lines = last_lines(&text);
        assert_eq!(lines.len(), REPORT_LINES);
        assert_eq!(lines.first().map(String::as_str), Some("line 9"));
        assert_eq!(lines.last().map(String::as_str), Some("line 20"));
        assert!(last_lines("\n  \n").is_empty(), "nothing said: nothing to show");
        assert_eq!(home_masked("panicked at /Users/someone/.cargo/git/x.rs:3 and /home/u/y"), "panicked at ~/.cargo/git/x.rs:3 and ~/y");
        assert_eq!(home_masked("/Users/someone"), "~");
        assert_eq!(home_masked("/Users/someone/home/x and data/home/y"), "~/home/x and data/home/y", "only where a path starts");
        assert_eq!(home_masked("caf\u{e9} \u{2192} /home/u"), "caf\u{e9} \u{2192} ~");
        push(alert("maps"));
        assert!(!reporting());
        show_report();
        assert!(reporting());
        push(alert("maps"));
        assert!(!reporting(), "a new alert for the app in front starts without its report");
        show_report();
        answer(false);
        assert!(!reporting(), "the answer puts the report away with its alert");
        show_report();
        assert!(!reporting(), "no alert, no report");
    }

    #[test]
    fn one_alert_at_a_time_one_to_an_app_and_only_reopen_reopens() {
        assert!(front().is_none());
        let start = generation();
        push(alert("terminal"));
        push(alert("news"));
        push(alert("terminal"));
        assert_eq!(front().map(|a| a.app), Some("news".into()), "the second alert for Terminal took the first one's place, behind News");
        assert!(generation() > start);
        answer(false);
        assert!(take_reopen().is_empty(), "Ignore reopens nothing");
        assert_eq!(front().map(|a| a.app), Some("terminal".into()));
        answer(true);
        assert!(front().is_none());
        let reopen = take_reopen();
        assert_eq!(reopen.iter().map(|a| a.app.as_str()).collect::<Vec<_>>(), ["terminal"]);
        assert!(take_reopen().is_empty(), "taken once");
        answer(true);
        assert!(take_reopen().is_empty(), "an answer with no alert up does nothing");
    }
}
