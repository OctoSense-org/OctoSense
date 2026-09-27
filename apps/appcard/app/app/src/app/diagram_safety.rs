//! Diagram-fence safety scanner.
//!
//! Lifted verbatim from `aichat:1238–1335`. This is the scan that prevents
//! a half-streamed `diagram` fenced block from being persisted as a final
//! message — the bytes haven't been parsed yet, so we'd commit malformed
//! JSON. Octos may emit different fenced types but the safety rules are
//! universal (see `05-AICHAT-REUSE-MAP.md`).
//!
//! The streaming pipeline calls `assistant_message_is_safe_to_store` from
//! `main.rs::handle_event` on `AgentEvent::TurnComplete`. The matching
//! `assistant_message_is_safe_for_history` is exercised only by the
//! regression tests in `main.rs::tests` — production no longer replays
//! history client-side.

/// Some LLMs, when asked "show me a markdown file demo with ... inside",
/// wrap their ENTIRE reply in a single ```markdown ... ``` fence. Because
/// CommonMark does not support fence nesting, pulldown-cmark then treats
/// the whole reply as ONE code block — collapsing markdown structure
/// (headings, lists, inner fences, math, …) into monospace text and killing
/// the streaming fade animation.
///
/// Strategy is aggressive: as soon as the text starts with ```markdown\n (or
/// ```md\n), strip that opener even if the outer fence hasn't closed yet.
/// Otherwise we'd keep the whole streaming reply stuck in code-block mode
/// until the final token arrives. The trailing outer fence is stripped too
/// when present.
pub fn unwrap_outer_markdown_fence(text: &str) -> &str {
    let trimmed_text = text.trim_start();
    // CommonMark allows fences of any length ≥ 3 — 3 backticks for plain
    // code, 4+ for wrappers that want to contain inner 3-backtick blocks.
    // LLMs use both; handle any length.
    let bt_count = trimmed_text.bytes().take_while(|b| *b == b'`').count();
    if bt_count < 3 {
        return text;
    }
    let after_ticks = &trimmed_text[bt_count..];
    let body_start = after_ticks
        .strip_prefix("markdown\n")
        .or_else(|| after_ticks.strip_prefix("md\n"));
    let Some(body) = body_start else {
        return text;
    };
    // Try to strip a matching closing fence at the end: same length or
    // longer, optionally followed by trailing whitespace. If streaming is
    // mid-way and there's no close yet, return the opener-stripped body.
    let close_pat = "`".repeat(bt_count);
    let end_trimmed = body.trim_end();
    if let Some(without_close) = end_trimmed.strip_suffix(&close_pat) {
        return without_close.trim_end_matches('\n').trim_end();
    }
    body
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiagramFenceStatus {
    None,
    Valid,
    UnclosedNonDiagram,
    Invalid,
}

struct OpenReplyFence {
    count: usize,
    fence_char: char,
    info: String,
    body_start: usize,
}

pub(crate) fn scan_diagram_fence_status(text: &str) -> DiagramFenceStatus {
    let text = unwrap_outer_markdown_fence(text);
    let mut status = DiagramFenceStatus::None;
    let mut open: Option<OpenReplyFence> = None;
    let mut line_start = 0;
    let bytes = text.as_bytes();
    let mut i = 0;

    while i <= bytes.len() {
        let at_end = i == bytes.len();
        let is_newline = !at_end && bytes[i] == b'\n';
        if at_end || is_newline {
            let line = text.get(line_start..i).unwrap_or("");
            let next_line_start = if is_newline { i + 1 } else { i };

            match &open {
                Some(fence) => {
                    if reply_fence_closes(line, fence) {
                        if fence.info.eq_ignore_ascii_case("diagram") {
                            let body = text.get(fence.body_start..line_start).unwrap_or("");
                            if crate::makepad_diagram_kit::parse(body.trim()).is_err() {
                                return DiagramFenceStatus::Invalid;
                            }
                            status = DiagramFenceStatus::Valid;
                        }
                        open = None;
                    }
                }
                None => {
                    if let Some((count, fence_char, info)) = reply_fence_opens(line) {
                        open = Some(OpenReplyFence {
                            count,
                            fence_char,
                            info,
                            body_start: next_line_start,
                        });
                    }
                }
            }

            line_start = next_line_start;
            i += 1;
        } else {
            i += 1;
        }
    }

    match open {
        Some(fence) if fence.info.eq_ignore_ascii_case("diagram") => DiagramFenceStatus::Invalid,
        Some(_) => DiagramFenceStatus::UnclosedNonDiagram,
        None => status,
    }
}

fn reply_fence_opens(line: &str) -> Option<(usize, char, String)> {
    let trimmed = line.trim_start().trim_end_matches('\r');
    let first = trimmed.chars().next()?;
    if first != '`' && first != '~' {
        return None;
    }

    let count = trimmed.chars().take_while(|ch| *ch == first).count();
    if count < 3 {
        return None;
    }

    let info = trimmed[count..]
        .trim()
        .split_ascii_whitespace()
        .next()
        .unwrap_or("")
        .to_string();
    Some((count, first, info))
}

fn reply_fence_closes(line: &str, fence: &OpenReplyFence) -> bool {
    let trimmed = line.trim_start().trim_end_matches('\r');
    let count = trimmed
        .chars()
        .take_while(|ch| *ch == fence.fence_char)
        .count();
    count >= fence.count && trimmed[count..].trim().is_empty()
}

pub(crate) fn assistant_message_is_safe_to_store(text: &str) -> bool {
    scan_diagram_fence_status(text) != DiagramFenceStatus::Invalid
}

// Retained as a regression-test target for the diagram-safety scanner —
// production code in this crate no longer replays history client-side
// (Octos sessions are stateful server-side; see `App::clear_chat` in main.rs).
#[allow(dead_code)]
pub(crate) fn assistant_message_is_safe_for_history(text: &str) -> bool {
    scan_diagram_fence_status(text) == DiagramFenceStatus::None
}
