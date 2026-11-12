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

/// The end of a client's log: its last 16 KiB, as text, from the first whole
/// line in them.
pub fn read_tail(path: &std::path::Path) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let start = len.saturating_sub(16 * 1024);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    Some(if start > 0 { text.split_once('\n').map(|(_, rest)| rest.to_string()).unwrap_or_default() } else { text })
}

/// A process's report, as a Mac's reads: the alert's own sentence, then under
/// Problem Details why the process ended (its signal or exit code, from `why`'s
/// first line) and the last of what it wrote that was more than information:
/// its warnings, errors and panics. A process that did not start says so.
pub fn process_report(label: &str, why: Option<&str>, output: &str) -> Report {
    let first = why.and_then(|why| why.lines().next()).unwrap_or_default();
    if first.contains(" did not start: ") {
        return Report { summary: home_masked(first), lines: last_lines(&more_than_information(output)) };
    }
    let mut lines: Vec<String> = termination_reason(first).into_iter().collect();
    let wrong = last_lines(&more_than_information(output));
    let room = REPORT_LINES - lines.len();
    lines.extend(wrong[wrong.len().saturating_sub(room)..].iter().cloned());
    Report { summary: format!("{label} quit unexpectedly."), lines }
}

/// How a report's Termination Reason line starts.
const REASON: &str = "Termination Reason: ";

/// A Mac report's Termination Reason, from an exit as Rust words it: `signal:
/// 9 (SIGKILL)` reads "SIGKILL (signal 9)", `exit status: 101` "exit code 101".
fn termination_reason(exit: &str) -> Option<String> {
    if let Some(rest) = exit.split("signal: ").nth(1) {
        let number: String = rest.chars().take_while(char::is_ascii_digit).collect();
        let name = rest.split('(').nth(1).and_then(|name| name.split(')').next()).filter(|name| name.starts_with("SIG"));
        return Some(match name {
            Some(name) => format!("{REASON}{name} (signal {number})"),
            None => format!("{REASON}signal {number}"),
        });
    }
    let code: String = exit.split("exit status: ").nth(1)?.chars().take_while(|c| c.is_ascii_digit() || *c == '-').collect();
    (!code.is_empty()).then(|| format!("{REASON}exit code {code}"))
}

/// A report's lines as its box shows them, each wrapped to the box already
/// (`wrapped[i]` is `lines[i]`'s rows): the Termination Reason on top, whole,
/// then the last of what went wrong that fits in [`REPORT_LINES`] rows, so a
/// long error never pushes the reason out.
pub fn shown_rows(lines: &[String], wrapped: Vec<Vec<String>>) -> Vec<String> {
    let mut wrapped = wrapped.into_iter();
    let mut rows = Vec::new();
    if lines.first().is_some_and(|line| line.starts_with(REASON)) {
        rows.extend(wrapped.next().unwrap_or_default());
    }
    let rest: Vec<String> = wrapped.flatten().collect();
    let room = REPORT_LINES.saturating_sub(rows.len());
    rows.extend(rest[rest.len().saturating_sub(room)..].iter().cloned());
    rows
}

/// A log's lines without its information and waiting lines (`[I]`, `[.]`):
/// what is left is what went wrong, and what the process wrote by itself.
fn more_than_information(output: &str) -> String {
    output.lines().filter(|line| !line.starts_with("[I]") && !line.starts_with("[.]")).collect::<Vec<_>>().join("\n")
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

    /// A process's report says what a Mac's does: the alert's sentence, the
    /// Termination Reason, then only what went wrong, never its information lines.
    #[test]
    fn a_process_report_gives_its_termination_reason_and_what_went_wrong() {
        let log = "[I] cx.rs:696:9 - memory budget: 8192 MiB\n[W] font.rs:12:3 - no face for U+1F600\n[.] waiting\n[E] pty.rs:40:1 - read failed\nthread 'main' panicked at /Users/someone/x.rs:3:1";
        let report = process_report("Terminal", Some("Terminal exited unexpectedly (signal: 9 (SIGKILL)).\nLog: /tmp/t.log"), log);
        assert_eq!(report.summary, "Terminal quit unexpectedly.");
        assert_eq!(report.lines, ["Termination Reason: SIGKILL (signal 9)", "[W] font.rs:12:3 - no face for U+1F600", "[E] pty.rs:40:1 - read failed", "thread 'main' panicked at ~/x.rs:3:1"]);
        let report = process_report("Reference App", Some("Reference App exited unexpectedly (exit status: 101)."), "[I] started");
        assert_eq!(report.lines, ["Termination Reason: exit code 101"], "only information: nothing more to show");
        assert_eq!(termination_reason("News exited unexpectedly (signal: 6 (SIGABRT) (core dumped))."), Some("Termination Reason: SIGABRT (signal 6)".to_string()));
        let report = process_report("Maps", Some("Maps did not start: no such file."), "");
        assert_eq!(report.summary, "Maps did not start: no such file.");
        assert!(report.lines.is_empty(), "nothing recorded: the report says so");
        assert!(process_report("Maps", None, "").lines.is_empty());
    }

    /// Wrapped long errors fill the box from its foot, but the Termination Reason
    /// keeps the top row.
    #[test]
    fn a_reports_long_errors_never_push_its_termination_reason_out() {
        let lines = vec!["Termination Reason: SIGKILL (signal 9)".to_string(), "[E] a long error".to_string()];
        let error: Vec<String> = (0..20).map(|i| format!("row {i}")).collect();
        let rows = shown_rows(&lines, vec![vec![lines[0].clone()], error]);
        assert_eq!(rows.len(), REPORT_LINES);
        assert_eq!(rows[0], "Termination Reason: SIGKILL (signal 9)");
        assert_eq!(rows[1], "row 9", "the last of the error after it");
        assert_eq!(rows[REPORT_LINES - 1], "row 19");
        let lines = vec!["[E] no reason".to_string()];
        let rows = shown_rows(&lines, vec![(0..15).map(|i| format!("row {i}")).collect()]);
        assert_eq!(rows.first().map(String::as_str), Some("row 3"), "without a reason the last rows fill the box");
        assert!(shown_rows(&[], Vec::new()).is_empty());
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
