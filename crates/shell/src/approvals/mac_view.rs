//! A Mac's approval surfaces (ADR 0004 §8, §4): the same requests, the same
//! answers and the same order as `view.rs` draws them on every other style, in
//! a Mac's forms.
//!
//! - **An approval sheet** says one sentence about one actor, then every value
//!   of the call a person can read in labelled rows. It attaches under the top edge
//!   of the window it belongs to, with a light dim; with no such window in front
//!   it stands alone. Under the rows, with its label, the choice of how long: the
//!   two a phone's sheet offers, and a note says in full what a chosen rule does.
//!   Bottom right: Don't Allow, and a default that says what it will do.
//! - **The default is not the dangerous answer**: on a command, on what no rule
//!   may answer, on what undoes something and on a retry of what may have happened,
//!   Don't Allow is blue and rightmost, and is what Return presses. Elsewhere Return
//!   presses nothing: these sheets come up on their own, while someone may be typing.
//! - **A batch** is one list with one default, each item with its own choice.
//!   Each line is still answered on its own (ADR 0004 §8).
//! - **Every row has been on screen before Allow works**, as on a phone: the
//!   rows scroll in one area, whose end has to have been shown. Don't Allow
//!   always works.
//! - **The quit alert** (quit_alert.rs): an app that quit unexpectedly, with its
//!   icon, and Reopen, Report… and Ignore stacked. Report… shows what the shell
//!   recorded of why, or says nothing was. Its window went with it, so it stands
//!   alone.
//! - **The first-use alert** says what the agent may read and use and where its
//!   model runs, all of it, with the app's icon.
//!
//! A press decides nothing here but what `view.rs`'s `act` decides. Choosing in
//! a pop-up only changes what the default will do, and says so.

use super::super::mac_kit::*;
use super::super::words;
use super::*;

const SHEET_W: f64 = 560.0;
const SHEET_PAD: f64 = 20.0;
const SHEET_ICON: f64 = 48.0;
const QUESTION_W: f64 = 440.0;
const ALERT_W: f64 = 300.0;
const ALERT_PAD: f64 = 16.0;
const ALERT_ICON: f64 = 64.0;
/// A form row's label column, right-aligned: as wide as the sheet's widest label
/// within these. A longer label wraps.
const LABEL_MIN: f64 = 44.0;
const LABEL_MAX: f64 = 170.0;
const ROW_INSET: f64 = 12.0;
const ROW_PAD: f64 = 5.0;
/// Room the overlay thumb may cover.
const THUMB_ROOM: f64 = 14.0;
const CORNER: f64 = 12.0;
/// What the quit alert says under its title.
const QUIT_MESSAGE: &str = "Click Reopen to open it again. Click Report to see more detailed information.";
/// The quit alert's stacked buttons: the space between them.
const STACK_GAP: f64 = 8.0;
/// The problem report Report… shows: its width, the heading of its details,
/// and what it says when nothing was recorded.
const REPORT_W: f64 = 440.0;
const REPORT_DETAILS: &str = "Problem Details";
const REPORT_NOTHING: &str = "Nothing was recorded before it quit.";
const GATE_NOTE: &str = "Scroll to the end to read everything before you allow this.";
const GATE_DONE: &str = "You have seen everything this asks.";
const HIDDEN_NOTE: &str = "Passwords and keys are hidden here.";
const ONLY_REFERENCES: &str = "The request names this only by an internal reference.";
/// The answer to a permission, wherever a sheet asks for one.
const REFUSE: &str = "Don't Allow";
/// The label of a sheet's choice of how long.
const CHOICE_LABEL: &str = "Allow:";
/// How far a press has to move before its release counts as the end of a drag.
const DRAG_SLOP: f64 = 4.0;

/// What the rows of a sheet are laid out as.
#[derive(Clone, Debug)]
pub(super) enum Block {
    /// A batch's item: what it does and its own choice.
    Head { line: usize, text: Vec<String>, note: Vec<String>, h: f64 },
    /// `top` is the space above its first line: a continuation has none.
    Row { label: Vec<String>, value: Vec<String>, code: bool, top: f64, h: f64 },
    Line { inset: f64, h: f64 },
}

/// One line's rows, laid out for a width and a label column.
#[derive(Clone, Debug)]
pub(super) struct LaidOut {
    /// Its widest label, however the rows are laid out.
    widest: f64,
    group_w: f64,
    label_w: f64,
    blocks: Vec<Block>,
}

impl Block {
    fn h(&self) -> f64 {
        match self {
            Block::Head { h, .. } | Block::Row { h, .. } | Block::Line { h, .. } => *h,
        }
    }
}

/// A pop-up's menu, while it is open.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct OpenPopup {
    pub request: RequestId,
    pub button: Rect,
}

/// The choices of one line: once, or its rules, which are the same two a phone's
/// sheet offers. On a sheet of one they say how far, under their label. In a
/// batch each line's pop-up is its whole answer, and Don't Allow is one of them.
fn line_choices(line: &Line, batch: bool) -> Vec<(String, Answer)> {
    let mut out = vec![(if batch { "Allow Once" } else { "Once" }.to_string(), Answer::Once)];
    for (k, choice) in line.always.iter().enumerate().take(2) {
        out.push((if batch { words::choice_title(&choice.draft) } else { words::choice_scope(&choice.draft) }, Answer::Always(k)));
    }
    if batch {
        out.push((REFUSE.to_string(), Answer::Deny));
    }
    out
}

/// What a sheet's allowing button says for the choice made: what it will do.
fn allow_title(line: &Line, answer: Answer) -> String {
    match answer {
        Answer::Always(k) => line.always.get(k).map_or_else(|| "Allow Once".to_string(), |choice| words::choice_title(&choice.draft)),
        _ => "Allow Once".to_string(),
    }
}

/// The icon of who a line's sentence is about: the agent's app, the assistant,
/// or for an outside client the app it calls.
fn actor_icon(line: &Line) -> String {
    match &line.agent {
        Some(agent) => icon_name(agent),
        None if line.said.actor == words::SYSTEM_ACTOR => crate::desktop::ASSISTANT_ICON.to_string(),
        None => icon_name(&line.app),
    }
}

/// What the batch's title says: one actor when there is one.
fn batch_title(lines: &[&Line]) -> String {
    let n = lines.len();
    let actor = &lines[0].said.actor;
    if lines.iter().all(|l| l.said.actor == *actor) {
        format!("{actor} wants to do {n} things.")
    } else {
        format!("{n} requests need your answer.")
    }
}

/// The button that answers a batch as its lines are set. Like any Mac button it
/// names one action; the lines set to Don't Allow say what happens to the rest.
fn batch_default(allow: usize, deny: usize) -> String {
    match (allow, deny) {
        (_, 0) => "Allow All".to_string(),
        (0, n) => format!("{REFUSE} {n}"),
        (a, _) => format!("Allow {a}"),
    }
}

impl ShellApprovals {
    fn control_state(&self, r: Rect) -> (bool, bool) {
        let down = self.down_rect == Some(r) && self.hover == Some(r);
        (self.hover == Some(r), down)
    }

    fn note_control(&mut self, label: &str, r: Rect, on: bool) {
        let middle = dvec2(r.pos.x + r.size.x * 0.5, r.pos.y + r.size.y * 0.5);
        if self.note_clip.is_some_and(|area| !contains(area, middle)) {
            return;
        }
        self.controls.push(format!("\u{27e6}{label}|{}|{}|{}|{}|{}\u{27e7}", r.pos.x, r.pos.y, r.size.x, r.size.y, if on { "on" } else { "off" }));
    }

    fn mac_button(&mut self, cx: &mut Cx2d, r: Rect, k: &MacInk, face: Face, label: &str, hit: Option<Hit>) {
        let (hover, down) = self.control_state(r);
        push_button(&mut self.d, cx, r, k, face, hover && face != Face::Disabled, down && face != Face::Disabled, label);
        self.note_control(label, r, face != Face::Disabled && hit.is_some());
        if face == Face::Default {
            // For the control surface: which of them is blue.
            self.controls.push(format!("\u{27e6}default|{label}\u{27e7}"));
        }
        if face != Face::Disabled {
            if let Some(hit) = hit {
                self.hits.push((r, hit));
            }
        }
    }

    /// Everything on a Mac, in `draw_all`'s order.
    pub(super) fn mac_draw_all(&mut self, cx: &mut Cx2d, screen: Rect, f: &Frame, place: &MacPlace) {
        let k = mac_ink(place.dark);
        self.controls.clear();
        self.escape = None;
        self.return_key = None;
        self.mac_banners(cx, screen, f, place);
        if let Some(summary) = &f.consent {
            self.modal = true;
            self.mac_consent(cx, screen, summary, place, &k);
        } else if let Some(sheet) = &f.sheet {
            self.modal = true;
            self.mac_sheet(cx, screen, sheet, f.more_sheets, place, &k);
        } else if let Some(question) = &f.question {
            self.modal = true;
            self.mac_question(cx, screen, question, place, &k);
        } else if let Some(alert) = &f.quit {
            self.modal = true;
            self.mac_quit_alert(cx, screen, alert, place, &k);
        } else {
            self.scroller.clear();
            self.popup = None;
            self.popup_cursor = None;
            self.popup_press = None;
            self.choices.clear();
            self.mac_blocks.clear();
            self.question_pick = None;
        }
    }

    /// Where a card goes: under the window it belongs to, or standing alone in
    /// the upper part of the screen. Returns the card's rect and that window.
    fn mac_card_rect(screen: Rect, place: &MacPlace, window: Option<&FrontWindow>, w: f64, h: f64) -> Rect {
        let floor = screen.pos.y + screen.size.y - place.dock - 8.0;
        let top = screen.pos.y + 8.0;
        let room = (floor - top).max(120.0);
        let h = h.min(room);
        match window {
            Some(win) => {
                let over = crate::system_chat::view::mac::sheet_room(win.rect, w);
                let x = over.pos.x + (over.size.x - w) * 0.5;
                let x = x.clamp(screen.pos.x + 8.0, (screen.pos.x + screen.size.x - w - 8.0).max(screen.pos.x + 8.0));
                let y = (win.rect.pos.y + win.top).min(floor - h).max(top);
                rect(x.floor(), y.floor(), w, h)
            }
            None => {
                let x = screen.pos.x + (screen.size.x - w) * 0.5;
                let y = (top + (room - h) * 0.3).max(top);
                rect(x.floor(), y.floor(), w, h)
            }
        }
    }

    /// The most a card under `window` may be tall.
    fn mac_card_room(screen: Rect, place: &MacPlace, window: Option<&FrontWindow>) -> f64 {
        let floor = screen.pos.y + screen.size.y - place.dock - 8.0;
        let top = match window {
            Some(win) => (win.rect.pos.y + win.top).max(screen.pos.y + 8.0),
            None => screen.pos.y + 8.0,
        };
        (floor - top).max(120.0)
    }

    /// The light dim over the window a sheet belongs to.
    fn mac_dim(&mut self, cx: &mut Cx2d, window: Option<&FrontWindow>, k: &MacInk) {
        if let Some(win) = window {
            dim_window(&mut self.d, cx, win.rect, k);
        }
    }

    /// Rows and batch items, laid out for a value column.
    fn mac_layout_blocks(&mut self, cx: &mut Cx2d, lines: &[&Line], group_w: f64, batch: bool) -> Vec<Block> {
        // One column for the whole sheet, so that a batch's rows line up.
        let mut widest: f64 = 0.0;
        for line in lines {
            let own = match self.mac_blocks.get(&line.request) {
                Some(laid) => laid.widest,
                None => {
                    let mut own: f64 = 0.0;
                    for row in line.said.rows.iter().filter(|r| !r.label.is_empty()) {
                        own = own.max(measure(&mut self.d, cx, false, TEXT, &row.label));
                    }
                    self.mac_blocks.insert(line.request.clone(), LaidOut { widest: own, group_w: 0.0, label_w: 0.0, blocks: Vec::new() });
                    own
                }
            };
            widest = widest.max(own);
        }
        let label_w = (widest + 1.0).ceil().clamp(LABEL_MIN, LABEL_MAX.min(group_w * 0.36));
        self.mac_label_w = label_w;
        let value_w = (group_w - ROW_INSET - label_w - 10.0 - THUMB_ROOM).max(80.0);
        let mut blocks = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            let cached = self.mac_blocks.get(&line.request).filter(|l| l.group_w == group_w && l.label_w == label_w).map(|l| l.blocks.clone());
            let own = match cached {
                Some(own) => own,
                None => {
                    let mut own = Vec::new();
                    for (r, row) in line.said.rows.iter().enumerate() {
                        let label = if row.label.is_empty() { Vec::new() } else { wrapped(&mut self.d, cx, false, TEXT, &row.label, label_w) };
                        let value = wrapped_exact(&mut self.d, cx, row.code, if row.code { 12.0 } else { TEXT }, &row.value, value_w);
                        let n = label.len().max(value.len()).max(1) as f64;
                        if !row.label.is_empty() && r > 0 {
                            own.push(Block::Line { inset: ROW_INSET, h: 1.0 });
                        }
                        let top = if row.label.is_empty() { 0.0 } else { ROW_PAD };
                        let next_continues = line.said.rows.get(r + 1).is_some_and(|n| n.label.is_empty());
                        let bottom = if next_continues { 0.0 } else { ROW_PAD };
                        own.push(Block::Row { label, value, code: row.code, top, h: n * TEXT_LINE + top + bottom });
                    }
                    if let Some(laid) = self.mac_blocks.get_mut(&line.request) {
                        laid.group_w = group_w;
                        laid.label_w = label_w;
                        laid.blocks = own.clone();
                    }
                    own
                }
            };
            if batch {
                if i > 0 {
                    blocks.push(Block::Line { inset: 0.0, h: 1.0 });
                }
                // One column of pop-ups, one width.
                let titles: Vec<String> = lines.iter().flat_map(|l| line_choices(l, true)).map(|(t, _)| t).collect();
                let popup_w = popup_width(&mut self.d, cx, &titles, 12.0, 96.0);
                let text = wrapped(&mut self.d, cx, true, TEXT, &line.said.item, group_w - ROW_INSET * 2.0 - popup_w - THUMB_ROOM);
                let note_text = words::surfaced_said(&line.surfaced);
                let note = if note_text.is_empty() { Vec::new() } else { wrapped(&mut self.d, cx, false, NOTE, note_text, group_w - ROW_INSET * 2.0 - popup_w - THUMB_ROOM) };
                let h = (text.len() as f64 * TEXT_LINE + note.len() as f64 * NOTE_LINE).max(SMALL_H) + 12.0;
                blocks.push(Block::Head { line: i, text, note, h });
                blocks.push(Block::Line { inset: ROW_INSET, h: 1.0 });
            }
            if own.is_empty() {
                let said = if line.said.only_references { ONLY_REFERENCES } else { "Nothing else goes with this." };
                let value = wrapped(&mut self.d, cx, false, TEXT, said, group_w - ROW_INSET * 2.0 - THUMB_ROOM);
                let h = value.len() as f64 * TEXT_LINE + ROW_PAD * 2.0;
                blocks.push(Block::Row { label: Vec::new(), value, code: false, top: ROW_PAD, h });
            }
            blocks.extend(own);
        }
        blocks
    }

    /// The blocks in their scrolling area. `offset` is how far it has scrolled.
    #[allow(clippy::too_many_arguments)]
    fn mac_draw_blocks(&mut self, cx: &mut Cx2d, area: Rect, blocks: &[Block], lines: &[&Line], k: &MacInk) {
        let offset = self.scroller.offset;
        let label_w = self.mac_label_w;
        let mut y = area.pos.y - offset;
        self.note_clip = Some(area);
        cx.push_clip_rect(area);
        for block in blocks {
            let h = block.h();
            if y + h > area.pos.y && y < area.pos.y + area.size.y {
                match block {
                    Block::Line { inset, .. } => {
                        self.d.solid(cx, rect(area.pos.x + inset, y, area.size.x - inset, 1.0), k.line);
                    }
                    Block::Row { label, value, code, top, .. } => {
                        let top = y + top;
                        for (i, text) in label.iter().enumerate() {
                            self.d.label(cx, rect(area.pos.x + ROW_INSET, top + i as f64 * TEXT_LINE, label_w, TEXT_LINE), false, TEXT, k.muted, HAlign::Right, text);
                        }
                        let vx = area.pos.x + ROW_INSET + label_w + 10.0;
                        let vw = area.size.x - ROW_INSET - label_w - 10.0 - THUMB_ROOM;
                        for (i, text) in value.iter().enumerate() {
                            let line = rect(vx, top + i as f64 * TEXT_LINE, vw, TEXT_LINE);
                            if *code {
                                self.d.with_code(|d| d.label(cx, line, false, 12.0, k.ink, HAlign::Left, text));
                            } else {
                                self.d.label(cx, line, false, TEXT, k.ink, HAlign::Left, text);
                            }
                        }
                    }
                    Block::Head { line, text, note, h } => {
                        let l = lines[*line];
                        let titles = line_choices(l, true);
                        let names: Vec<String> = lines.iter().flat_map(|l| line_choices(l, true)).map(|(t, _)| t).collect();
                        let popup_w = popup_width(&mut self.d, cx, &names, 12.0, 96.0);
                        // The words centred on the pop-up beside them, like any row with a control.
                        let mut ty = y + (*h - text.len() as f64 * TEXT_LINE - note.len() as f64 * NOTE_LINE) * 0.5;
                        for t in text {
                            self.d.label(cx, rect(area.pos.x + ROW_INSET, ty, area.size.x - ROW_INSET * 2.0 - popup_w - THUMB_ROOM, TEXT_LINE), true, TEXT, k.ink, HAlign::Left, t);
                            ty += TEXT_LINE;
                        }
                        for t in note {
                            self.d.label(cx, rect(area.pos.x + ROW_INSET, ty, area.size.x - ROW_INSET * 2.0 - popup_w - THUMB_ROOM, NOTE_LINE), false, NOTE, k.warn, HAlign::Left, t);
                            ty += NOTE_LINE;
                        }
                        let chosen = self.choices.get(&l.request).copied().unwrap_or(Answer::Once);
                        let title = titles.iter().find(|(_, a)| *a == chosen).map_or_else(|| allow_title(l, chosen), |(t, _)| t.clone());
                        let button = rect(area.pos.x + area.size.x - THUMB_ROOM - popup_w, y + (*h - SMALL_H) * 0.5, popup_w, SMALL_H);
                        let (hover, down) = self.control_state(button);
                        popup_button(&mut self.d, cx, button, k, 12.0, hover, down, &title);
                        self.note_control(&title, button, true);
                        if let Some(r) = clipped(button, area) {
                            self.hits.push((r, Hit::Popup { request: l.request.clone(), button }));
                        }
                    }
                }
            }
            y += h;
        }
        cx.pop_clip_rect();
        self.note_clip = None;
        self.scroller.draw_thumb(&mut self.d, cx, k);
    }

    fn mac_sheet(&mut self, cx: &mut Cx2d, screen: Rect, sheet: &Sheet, more: usize, place: &MacPlace, k: &MacInk) {
        let lines: Vec<&Line> = sheet.open_lines().collect();
        let Some(first) = lines.first().copied() else { return };
        self.choices.retain(|id, _| lines.iter().any(|l| l.request == *id));
        self.mac_blocks.retain(|id, _| lines.iter().any(|l| l.request == *id));
        if self.popup.as_ref().is_some_and(|p| !lines.iter().any(|l| l.request == p.request)) {
            self.popup = None;
        }
        let batch = lines.len() > 1;
        // The window it belongs to: the asking app's, else the owning app's.
        let apps = [first.agent.as_deref().unwrap_or(&first.app), &first.app];
        let window = card_window(&apps).or_else(|| related_window(place, &apps).cloned()).filter(|w| w.rect.size.x >= 440.0);
        let w = SHEET_W.min(screen.size.x - 16.0).max(320.0);
        let inner = w - SHEET_PAD * 2.0;
        let room = Self::mac_card_room(screen, place, window.as_ref());

        // The words above the rows.
        let title = if batch { batch_title(&lines) } else { first.said.sentence.clone() };
        let plan = match &sheet.place {
            Place::SystemChat { plan, .. } if !plan.trim().is_empty() => Some(format!("Its plan: \u{201c}{}\u{201d}", visible(&super::super::facts::mask_text(plan.trim(), None).0))),
            _ => None,
        };
        let sub = plan.or_else(|| first.said.detail.clone().filter(|_| !batch));
        let text_w = inner - SHEET_ICON - 14.0;
        let title_lines = wrapped(&mut self.d, cx, true, TEXT, &title, text_w);
        let sub_lines = sub.as_deref().map_or(Vec::new(), |s| wrapped(&mut self.d, cx, false, NOTE, s, text_w));
        let head_h = (title_lines.len() as f64 * TEXT_LINE + if sub_lines.is_empty() { 0.0 } else { 2.0 + sub_lines.len() as f64 * NOTE_LINE }).max(SHEET_ICON);

        // The rows.
        let blocks = self.mac_layout_blocks(cx, &lines, inner, batch);
        let content: f64 = blocks.iter().map(Block::h).sum();

        // What each line is set to, and what the default button then does.
        let answers: Vec<(RequestId, Answer)> = lines.iter().map(|l| (l.request.clone(), self.choices.get(&l.request).copied().unwrap_or(Answer::Once))).collect();
        let allows = answers.iter().filter(|(_, a)| *a != Answer::Deny).count();
        let denies = answers.len() - allows;

        // The notes under the rows.
        let mut notes: Vec<(String, Vec4f)> = Vec::new();
        if !batch {
            let said = words::surfaced_said(&first.surfaced);
            if !said.is_empty() {
                notes.push((said.to_string(), k.warn));
            }
        }
        if lines.iter().any(|l| l.said.hidden > 0) {
            notes.push((HIDDEN_NOTE.to_string(), k.muted));
        }
        if batch {
            for (line, (_, answer)) in lines.iter().zip(&answers) {
                if let Answer::Always(i) = answer {
                    if let Some(choice) = line.always.get(*i) {
                        notes.push((words::choice_said(&choice.draft), k.muted));
                    }
                }
            }
        }
        if more > 0 {
            notes.push((if more == 1 { "1 more request waits after this one.".to_string() } else { format!("{more} more requests wait after this one.") }, k.muted));
        }

        // How long, in the content under its label, and in full what a rule does.
        let choices = line_choices(first, false);
        let names: Vec<String> = choices.iter().map(|(t, _)| t.clone()).collect();
        let form = !batch && names.len() > 1;
        let form_x = ROW_INSET + self.mac_label_w + 10.0;
        let popup_w = if form { popup_width(&mut self.d, cx, &names, TEXT, 120.0) } else { 0.0 };
        let rule_said = if batch {
            None
        } else if let Answer::Always(i) = answers[0].1 {
            first.always.get(i).map(|choice| words::choice_said(&choice.draft))
        } else {
            None
        };
        let rule_lines = rule_said.as_deref().map_or(Vec::new(), |said| wrapped(&mut self.d, cx, false, NOTE, said, inner - form_x));
        let form_h = if form { 10.0 + CONTROL_H } else { 0.0 } + if rule_lines.is_empty() { 0.0 } else { 4.0 + rule_lines.len() as f64 * NOTE_LINE };

        // The buttons at the bottom right; Stop at the far left when there is one.
        // On what should not be agreed to by habit, Don't Allow is the default.
        let stop = sheet.stop_target().map(str::to_string);
        let careful = lines.iter().any(|l| l.said.careful);
        let (deny_label, allow_label) = if batch {
            (format!("{REFUSE} Any"), batch_default(allows, denies))
        } else {
            (REFUSE.to_string(), allow_title(first, answers[0].1))
        };
        // Every line refused: one button, since a Mac alert never offers two with one effect.
        let single = batch && allows == 0;
        let deny_w = if single { 0.0 } else { button_width(&mut self.d, cx, &deny_label, 72.0) };
        let allow_w = button_width(&mut self.d, cx, &allow_label, 72.0);
        let cluster = if single { allow_w } else { deny_w + 8.0 + allow_w };
        // It names the assistant when there is room, and goes above the others
        // only when even the short words do not fit.
        let stop_title = stop.as_deref().map(|app| {
            let long = words::stop_title(app);
            if button_width(&mut self.d, cx, &long, 72.0) + 16.0 + cluster <= inner { long } else { words::STOP_SHORT.to_string() }
        });
        let stop_w = stop_title.as_deref().map_or(0.0, |t| button_width(&mut self.d, cx, t, 72.0));
        let stop_above = stop.is_some() && stop_w + 16.0 + cluster > inner;
        let foot_h = CONTROL_H + if stop_above { CONTROL_H + 8.0 } else { 0.0 };

        let wrap_notes = |this: &mut Self, cx: &mut Cx2d, notes: &[(String, Vec4f)]| -> Vec<(String, Vec4f)> {
            let mut out = Vec::new();
            for (text, color) in notes {
                for line in wrapped(&mut this.d, cx, false, NOTE, text, inner) {
                    out.push((line, *color));
                }
            }
            out
        };
        let mut note_lines = wrap_notes(self, cx, &notes);
        let fixed = |note_rows: usize| SHEET_PAD + head_h + 12.0 + if note_rows > 0 { 8.0 + note_rows as f64 * NOTE_LINE } else { 0.0 } + form_h + 14.0 + foot_h + SHEET_PAD;
        let scrolls = fixed(note_lines.len()) + content > room;
        let mut said_seen = true;
        if scrolls {
            // One row of the notes says what to do; it holds its place afterwards.
            let key_seen = self.scroller.seen_end && self.scroller_key_matches(sheet, &lines, inner, room);
            said_seen = key_seen;
            let (text, color) = if key_seen { (GATE_DONE, k.muted) } else { (GATE_NOTE, k.warn) };
            let mut lines_gate = wrap_notes(self, cx, &[(text.to_string(), color)]);
            lines_gate.append(&mut note_lines);
            note_lines = lines_gate;
        }
        let group_h = content.min((room - fixed(note_lines.len())).max(72.0));
        let total = fixed(note_lines.len()) + group_h;

        let card = Self::mac_card_rect(screen, place, window.as_ref(), w, total);
        self.mac_dim(cx, window.as_ref(), k);
        draw_card(&mut self.d, cx, card, CORNER, k);
        self.note_control("sheet card", card, false);
        self.hits.push((card, Hit::Card));

        let x = card.pos.x + SHEET_PAD;
        let mut y = card.pos.y + SHEET_PAD;
        // The asking app's icon: who the one sentence is about.
        self.app_icons.draw(cx, &actor_icon(first), place.style, rect(x, y, SHEET_ICON, SHEET_ICON), 1.0, k.ink);
        let tx = x + SHEET_ICON + 14.0;
        let mut ty = y;
        for line in &title_lines {
            self.d.label(cx, rect(tx, ty, text_w, TEXT_LINE), true, TEXT, k.ink, HAlign::Left, line);
            ty += TEXT_LINE;
        }
        ty += 2.0;
        for line in &sub_lines {
            self.d.label(cx, rect(tx, ty, text_w, NOTE_LINE), false, NOTE, k.muted, HAlign::Left, line);
            ty += NOTE_LINE;
        }
        y += head_h + 12.0;
        self.shown.push(title.clone());
        if let Some(sub) = &sub {
            self.shown.push(sub.clone());
        }

        // The rows, in one area that scrolls.
        let area = rect(x, y, inner, group_h);
        draw_group(&mut self.d, cx, area, k);
        let key = self.scroll_key(sheet, &lines, inner, room);
        self.scroller.layout(&key, area, content);
        let seen = self.scroller.seen_end;
        // The end came on screen as the rows were laid out: Allow is on now,
        // and the note says so on the next frame.
        if scrolls && seen && !said_seen {
            self.redraw(cx);
        }
        self.mac_draw_blocks(cx, area, &blocks, &lines, k);
        y += group_h;
        for line in &lines {
            if batch {
                self.shown.push(line.said.item.clone());
            }
            self.shown.extend(line.said.rows.iter().map(|r| if r.label.is_empty() { r.value.clone() } else { format!("{}: {}", r.label, r.value) }));
        }

        if !note_lines.is_empty() {
            y += 8.0;
            for (text, color) in &note_lines {
                self.d.label(cx, rect(x, y, inner, NOTE_LINE), false, NOTE, *color, HAlign::Left, text);
                self.shown.push(text.clone());
                y += NOTE_LINE;
            }
        }

        // The choice of how long, under its label, in the rows' columns.
        if form {
            y += 10.0;
            let answer = answers[0].1;
            let title = choices.iter().find(|(_, a)| *a == answer).map_or_else(|| names[0].clone(), |(t, _)| t.clone());
            self.d.label(cx, text_seat(rect(x + ROW_INSET, y, self.mac_label_w, CONTROL_H)), false, TEXT, k.muted, HAlign::Right, CHOICE_LABEL);
            let button = rect(x + form_x, y, popup_w, CONTROL_H);
            let (hover, down) = self.control_state(button);
            popup_button(&mut self.d, cx, button, k, TEXT, hover, down, &title);
            self.note_control(&format!("{CHOICE_LABEL} {title}"), button, true);
            self.hits.push((button, Hit::Popup { request: first.request.clone(), button }));
            y += CONTROL_H;
        }
        if !rule_lines.is_empty() {
            y += 4.0;
            for line in &rule_lines {
                self.d.label(cx, rect(x + form_x, y, inner - form_x, NOTE_LINE), false, NOTE, k.muted, HAlign::Left, line);
                y += NOTE_LINE;
            }
            self.shown.extend(rule_said.clone());
        }

        // The buttons: the default rightmost, and what it answers says which it is.
        let foot = card.pos.y + card.size.y - SHEET_PAD - CONTROL_H;
        let right = card.pos.x + card.size.x - SHEET_PAD;
        let (allow_x, deny_x) = if single {
            (right - allow_w, right)
        } else if careful {
            (right - deny_w - 8.0 - allow_w, right - deny_w)
        } else {
            (right - allow_w, right - allow_w - 8.0 - deny_w)
        };
        let allow_r = rect(allow_x, foot, allow_w, CONTROL_H);
        let deny_r = rect(deny_x, foot, deny_w, CONTROL_H);
        let (allow_hit, deny_hit, usable) = if batch {
            let all = answers.iter().map(|(id, _)| (id.clone(), Answer::Deny)).collect();
            (Hit::AnswerAll { sheet: sheet.id, answers: answers.clone() }, Hit::AnswerAll { sheet: sheet.id, answers: all }, seen || allows == 0)
        } else {
            let answer = |answer| Hit::Answer { sheet: sheet.id, request: first.request.clone(), answer };
            (answer(answers[0].1), answer(Answer::Deny), seen)
        };
        let allow_face = match (usable, careful) {
            (false, _) => Face::Disabled,
            (true, false) => Face::Default,
            (true, true) => Face::Plain,
        };
        if single {
            self.mac_button(cx, allow_r, k, Face::Default, &allow_label, Some(deny_hit.clone()));
        } else {
            self.mac_button(cx, allow_r, k, allow_face, &allow_label, usable.then_some(allow_hit));
            self.mac_button(cx, deny_r, k, if careful { Face::Default } else { Face::Plain }, &deny_label, Some(deny_hit.clone()));
        }
        self.escape = Some(deny_hit.clone());
        self.return_key = (careful || single).then_some(deny_hit);
        if let (Some(app), Some(label)) = (&stop, &stop_title) {
            let sy = if stop_above { foot - CONTROL_H - 8.0 } else { foot };
            self.mac_button(cx, rect(x, sy, stop_w, CONTROL_H), k, Face::Plain, label, Some(Hit::StopAgent { app: app.clone() }));
        }

        // A pop-up's menu, over everything but the default: inside the sheet, and
        // moved up if it would otherwise cover the answering buttons.
        if let Some(open) = self.popup.clone() {
            if let Some(line) = lines.iter().find(|l| l.request == open.request) {
                let titles = line_choices(line, batch);
                let names: Vec<String> = titles.iter().map(|(t, _)| t.clone()).collect();
                let chosen = self.choices.get(&open.request).copied().unwrap_or(Answer::Once);
                let chosen_i = titles.iter().position(|(_, a)| *a == chosen);
                let width = menu_width(&mut self.d, cx, &names, open.button.size.x);
                let inside = rect(card.pos.x + 4.0, card.pos.y + 4.0, card.size.x - 8.0, card.size.y - 8.0);
                let mut menu = menu_rect(open.button, width, names.len(), chosen_i, inside, screen);
                if overlap(menu, rect(allow_x.min(deny_x), foot, cluster, CONTROL_H)) {
                    let above = rect(inside.pos.x, inside.pos.y, inside.size.x, foot - 6.0 - inside.pos.y);
                    menu = menu_rect(open.button, width, names.len(), chosen_i, above, screen);
                }
                let rows = popup_menu(&mut self.d, cx, menu, k, &names, chosen_i.as_slice(), self.pointer_at, self.popup_cursor);
                self.hits.push((screen, Hit::PopupDismiss));
                self.hits.push((menu, Hit::Card));
                for (row, (title, answer)) in rows.iter().zip(titles.iter()) {
                    self.note_control(title, *row, true);
                    self.hits.push((*row, Hit::PopupChoose { request: open.request.clone(), answer: *answer }));
                }
            }
        }
    }

    /// What a sheet's scrolling area is about: new lines, another width or a
    /// window of another height start it over, unread.
    fn scroll_key(&self, sheet: &Sheet, lines: &[&Line], width: f64, room: f64) -> String {
        let ids: Vec<&str> = lines.iter().map(|l| l.request.0.as_str()).collect();
        format!("sheet {} {} {width} {}", sheet.id, ids.join(","), room.round())
    }
    fn scroller_key_matches(&self, sheet: &Sheet, lines: &[&Line], width: f64, room: f64) -> bool {
        self.scroller.key() == self.scroll_key(sheet, lines, width, room)
    }

    /// The first-use alert: the app's icon, a title, a short message, everything
    /// the agent may read and use and where its model runs, and the two answers.
    fn mac_consent(&mut self, cx: &mut Cx2d, screen: Rect, s: &AgentSummary, place: &MacPlace, k: &MacInk) {
        let window = card_window(&[&s.app]).or_else(|| related_window(place, &[&s.app]).cloned()).filter(|w| w.rect.size.x >= ALERT_W + 24.0);
        let w = ALERT_W.min(screen.size.x - 16.0);
        let inner = w - ALERT_PAD * 2.0;
        let title = words::consent_title(s);
        let title_lines = wrapped(&mut self.d, cx, true, TEXT, &title, inner);
        let message_lines = wrapped(&mut self.d, cx, false, NOTE, words::CONSENT_MESSAGE, inner);
        // The disclosures: all of them, in plain words.
        let label_w = 44.0;
        let value_w = inner - 16.0 - label_w - 8.0 - THUMB_ROOM * 0.5;
        let mut rows: Vec<(String, Vec<String>)> = Vec::new();
        let mut plain: Vec<String> = Vec::new();
        for (label, values) in [("Reads", &s.reads), ("Uses", &s.uses)] {
            for (i, value) in values.iter().enumerate() {
                let value = words::consent_line(s, value);
                let lines = wrapped(&mut self.d, cx, false, NOTE, &value, value_w);
                plain.push(value);
                rows.push((if i == 0 { label.to_string() } else { String::new() }, lines));
            }
        }
        let model = words::consent_line(s, &s.model);
        rows.push(("Model".to_string(), wrapped(&mut self.d, cx, false, NOTE, &model, value_w)));
        plain.push(model);
        let mut content = 12.0;
        for (i, (label, lines)) in rows.iter().enumerate() {
            if i > 0 && !label.is_empty() {
                content += 8.0;
            }
            content += lines.len().max(1) as f64 * NOTE_LINE;
        }
        let head = ALERT_PAD + ALERT_ICON + 10.0 + title_lines.len() as f64 * TEXT_LINE + 4.0 + message_lines.len() as f64 * NOTE_LINE + 10.0;
        let foot = 14.0 + CONTROL_H + ALERT_PAD;
        let room = Self::mac_card_room(screen, place, window.as_ref());
        let group_h = content.min((room - head - foot).max(60.0));
        let card = Self::mac_card_rect(screen, place, window.as_ref(), w, head + group_h + foot);
        self.mac_dim(cx, window.as_ref(), k);
        draw_card(&mut self.d, cx, card, CORNER, k);
        self.hits.push((card, Hit::Card));
        let x = card.pos.x + ALERT_PAD;
        let mut y = card.pos.y + ALERT_PAD;
        self.app_icons.draw(cx, &icon_name(&s.app), place.style, rect(card.pos.x + (w - ALERT_ICON) * 0.5, y, ALERT_ICON, ALERT_ICON), 1.0, k.ink);
        y += ALERT_ICON + 10.0;
        for line in &title_lines {
            self.d.label(cx, rect(x, y, inner, TEXT_LINE), true, TEXT, k.ink, HAlign::Center, line);
            y += TEXT_LINE;
        }
        y += 4.0;
        for line in &message_lines {
            self.d.label(cx, rect(x, y, inner, NOTE_LINE), false, NOTE, k.muted, HAlign::Center, line);
            y += NOTE_LINE;
        }
        y += 10.0;
        let area = rect(x, y, inner, group_h);
        draw_group(&mut self.d, cx, area, k);
        self.scroller.layout(&format!("consent {} {w}", s.app), area, content);
        cx.push_clip_rect(area);
        let mut ry = y + 6.0 - self.scroller.offset;
        for (i, (label, lines)) in rows.iter().enumerate() {
            if i > 0 && !label.is_empty() {
                ry += 8.0;
            }
            self.d.label(cx, rect(x + 8.0, ry, label_w, NOTE_LINE), false, NOTE, k.muted, HAlign::Right, label);
            for line in lines {
                self.d.label(cx, rect(x + 8.0 + label_w + 8.0, ry, value_w, NOTE_LINE), false, NOTE, k.ink, HAlign::Left, line);
                ry += NOTE_LINE;
            }
        }
        cx.pop_clip_rect();
        self.scroller.draw_thumb(&mut self.d, cx, k);
        let fy = card.pos.y + card.size.y - ALERT_PAD - CONTROL_H;
        let half = (inner - 8.0) * 0.5;
        let deny = Hit::Consent { app: s.app.clone(), allow: false };
        self.mac_button(cx, rect(x, fy, half, CONTROL_H), k, Face::Plain, "Don't Allow", Some(deny.clone()));
        self.mac_button(cx, rect(x + half + 8.0, fy, half, CONTROL_H), k, Face::Default, "Allow", Some(Hit::Consent { app: s.app.clone(), allow: true }));
        self.escape = Some(deny);
        self.shown.push(title);
        self.shown.push(words::CONSENT_MESSAGE.to_string());
        self.shown.extend(plain);
    }

    /// An app that quit unexpectedly, as a Mac says it: its icon, what happened,
    /// and Reopen (the default), Report… and Ignore stacked. Report… turns the
    /// alert into its problem report ([`Self::mac_quit_report`]). Its window went
    /// with it, so the alert stands alone. Escape ignores it; Return presses
    /// nothing, as it came up on its own.
    fn mac_quit_alert(&mut self, cx: &mut Cx2d, screen: Rect, alert: &crate::quit_alert::Alert, place: &MacPlace, k: &MacInk) {
        if crate::quit_alert::reporting() {
            return self.mac_quit_report(cx, screen, alert, place, k);
        }
        let w = ALERT_W.min(screen.size.x - 16.0);
        let inner = w - ALERT_PAD * 2.0;
        let title = format!("{} quit unexpectedly.", alert.label);
        let title_lines = balanced(&mut self.d, cx, true, TEXT, &title, inner);
        let message_lines = wrapped(&mut self.d, cx, false, NOTE, QUIT_MESSAGE, inner);
        let buttons = [
            ("Reopen", Face::Default, Hit::Quit { reopen: true }),
            ("Report\u{2026}", Face::Plain, Hit::QuitReport),
            ("Ignore", Face::Plain, Hit::Quit { reopen: false }),
        ];
        let stack = buttons.len() as f64 * CONTROL_H + (buttons.len() - 1) as f64 * STACK_GAP;
        let h = ALERT_PAD + ALERT_ICON + 10.0 + title_lines.len() as f64 * TEXT_LINE + 4.0 + message_lines.len() as f64 * NOTE_LINE + 14.0 + stack + ALERT_PAD;
        let card = Self::mac_card_rect(screen, place, None, w, h);
        draw_card(&mut self.d, cx, card, CORNER, k);
        self.hits.push((card, Hit::Card));
        let x = card.pos.x + ALERT_PAD;
        let mut y = card.pos.y + ALERT_PAD;
        self.app_icons.draw(cx, &icon_name(&alert.app), place.style, rect(card.pos.x + (w - ALERT_ICON) * 0.5, y, ALERT_ICON, ALERT_ICON), 1.0, k.ink);
        y += ALERT_ICON + 10.0;
        for line in &title_lines {
            self.d.label(cx, rect(x, y, inner, TEXT_LINE), true, TEXT, k.ink, HAlign::Center, line);
            y += TEXT_LINE;
        }
        y += 4.0;
        for line in &message_lines {
            self.d.label(cx, rect(x, y, inner, NOTE_LINE), false, NOTE, k.muted, HAlign::Center, line);
            y += NOTE_LINE;
        }
        let mut by = card.pos.y + card.size.y - ALERT_PAD - stack;
        for (label, face, hit) in buttons {
            self.mac_button(cx, rect(x, by, inner, CONTROL_H), k, face, label, Some(hit));
            by += CONTROL_H + STACK_GAP;
        }
        self.escape = Some(Hit::Quit { reopen: false });
        self.shown.push(title);
        self.shown.push(QUIT_MESSAGE.to_string());
    }

    /// The problem report Report… shows in the alert's place: the app's icon,
    /// what happened, and under Problem Details its own last words as the shell
    /// recorded them (its error, or the end of what it wrote), wrapped and never
    /// cut: its Termination Reason, then the last of the rest, in
    /// [`crate::quit_alert::REPORT_LINES`] lines; or that nothing was recorded. OK puts it away; Reopen opens the app again. Escape is OK.
    fn mac_quit_report(&mut self, cx: &mut Cx2d, screen: Rect, alert: &crate::quit_alert::Alert, place: &MacPlace, k: &MacInk) {
        let w = REPORT_W.min(screen.size.x - 16.0);
        let inner = w - SHEET_PAD * 2.0;
        let text_w = inner - SHEET_ICON - 14.0;
        let box_w = inner - 16.0;
        let title = format!("Problem Report for {}", alert.label);
        let summary = if alert.report.summary.trim().is_empty() { format!("{} quit unexpectedly.", alert.label) } else { alert.report.summary.clone() };
        let title_lines = wrapped(&mut self.d, cx, true, TEXT, &title, text_w);
        let summary_lines = wrapped(&mut self.d, cx, false, NOTE, &summary, text_w);
        let head_h = (title_lines.len() as f64 * TEXT_LINE + 2.0 + summary_lines.len() as f64 * NOTE_LINE).max(SHEET_ICON);
        let wrapped = alert.report.lines.iter().map(|line| wrapped_exact(&mut self.d, cx, true, NOTE, line, box_w)).collect();
        let lines = crate::quit_alert::shown_rows(&alert.report.lines, wrapped);
        let group_h = if lines.is_empty() { NOTE_LINE * 3.0 } else { lines.len() as f64 * NOTE_LINE + 12.0 };
        let h = SHEET_PAD + head_h + 14.0 + NOTE_LINE + 4.0 + group_h + 16.0 + CONTROL_H + SHEET_PAD;
        let card = Self::mac_card_rect(screen, place, None, w, h);
        draw_card(&mut self.d, cx, card, CORNER, k);
        self.hits.push((card, Hit::Card));
        let x = card.pos.x + SHEET_PAD;
        let mut y = card.pos.y + SHEET_PAD;
        self.app_icons.draw(cx, &icon_name(&alert.app), place.style, rect(x, y, SHEET_ICON, SHEET_ICON), 1.0, k.ink);
        let tx = x + SHEET_ICON + 14.0;
        let mut ty = y;
        for line in &title_lines {
            self.d.label(cx, rect(tx, ty, text_w, TEXT_LINE), true, TEXT, k.ink, HAlign::Left, line);
            ty += TEXT_LINE;
        }
        ty += 2.0;
        for line in &summary_lines {
            self.d.label(cx, rect(tx, ty, text_w, NOTE_LINE), false, NOTE, k.muted, HAlign::Left, line);
            ty += NOTE_LINE;
        }
        y += head_h + 14.0;
        self.d.label(cx, rect(x, y, inner, NOTE_LINE), true, NOTE, k.ink, HAlign::Left, REPORT_DETAILS);
        y += NOTE_LINE + 4.0;
        let area = rect(x, y, inner, group_h);
        draw_group(&mut self.d, cx, area, k);
        if lines.is_empty() {
            self.d.label(cx, area, false, NOTE, k.muted, HAlign::Center, REPORT_NOTHING);
        }
        let mut ly = y + 6.0;
        for line in &lines {
            self.d.with_code(|d| d.label(cx, rect(x + 8.0, ly, box_w, NOTE_LINE), false, NOTE, k.ink, HAlign::Left, line));
            ly += NOTE_LINE;
        }
        let fy = card.pos.y + card.size.y - SHEET_PAD - CONTROL_H;
        let reopen_w = button_width(&mut self.d, cx, "Reopen", 80.0);
        let ok_w = button_width(&mut self.d, cx, "OK", 80.0);
        let rx = card.pos.x + w - SHEET_PAD - reopen_w;
        self.mac_button(cx, rect(rx - 12.0 - ok_w, fy, ok_w, CONTROL_H), k, Face::Plain, "OK", Some(Hit::Quit { reopen: false }));
        self.mac_button(cx, rect(rx, fy, reopen_w, CONTROL_H), k, Face::Default, "Reopen", Some(Hit::Quit { reopen: true }));
        self.escape = Some(Hit::Quit { reopen: false });
        self.shown.push(title);
        self.shown.push(summary);
        self.shown.push(REPORT_DETAILS.to_string());
        if lines.is_empty() {
            self.shown.push(REPORT_NOTHING.to_string());
        }
        self.shown.extend(lines);
    }

    /// An app agent's question: who asks, what it asks, its options as one
    /// choice and at most three buttons.
    fn mac_question(&mut self, cx: &mut Cx2d, screen: Rect, q: &crate::questions::Request, place: &MacPlace, k: &MacInk) {
        let window = card_window(&[&q.app]).or_else(|| related_window(place, &[&q.app]).cloned()).filter(|w| w.rect.size.x >= 440.0);
        let w = QUESTION_W.min(screen.size.x - 16.0);
        let inner = w - SHEET_PAD * 2.0;
        let text_w = inner - SHEET_ICON - 14.0;
        let heading = format!("{} has a question.", capital(&words::agent_named(&q.app)));
        let who = words::question_said(q);
        let title_lines = wrapped(&mut self.d, cx, true, TEXT, &heading, text_w);
        let who_lines = who.as_deref().map_or(Vec::new(), |t| wrapped(&mut self.d, cx, false, NOTE, t, text_w));
        let head_h = (title_lines.len() as f64 * TEXT_LINE + if who_lines.is_empty() { 0.0 } else { 2.0 + who_lines.len() as f64 * NOTE_LINE }).max(SHEET_ICON);
        let question = {
            let mut lines = Vec::new();
            if !q.title.trim().is_empty() && q.title != q.text() {
                lines.push(visible(&q.title));
            }
            lines.push(visible(q.text()));
            lines
        };
        let mut question_lines = Vec::new();
        for text in &question {
            question_lines.extend(wrapped_exact(&mut self.d, cx, false, TEXT, text, inner));
        }
        let options = q.options();
        if self.question_pick.is_some_and(|(id, i)| id != q.id || i >= options.len()) {
            self.question_pick = None;
        }
        let option_w = inner - ROW_INSET - 26.0 - THUMB_ROOM;
        let wrapped_options: Vec<Vec<String>> = options.iter().map(|o| wrapped_exact(&mut self.d, cx, false, TEXT, &visible(o), option_w)).collect();
        let content: f64 = wrapped_options.iter().map(|l| l.len().max(1) as f64 * TEXT_LINE + 10.0).sum();
        let mut stop = words::stop_title(&q.app);
        let dont = "Don't Answer";
        let question_h = question_lines.len() as f64 * TEXT_LINE;
        let fixed = SHEET_PAD + head_h + 12.0 + question_h + if options.is_empty() { 0.0 } else { 10.0 } + 14.0 + CONTROL_H + SHEET_PAD;
        let room = Self::mac_card_room(screen, place, window.as_ref());
        let group_h = if options.is_empty() { 0.0 } else { content.min((room - fixed).max(TEXT_LINE + 10.0)) };
        let card = Self::mac_card_rect(screen, place, window.as_ref(), w, fixed + group_h);
        self.mac_dim(cx, window.as_ref(), k);
        draw_card(&mut self.d, cx, card, CORNER, k);
        self.hits.push((card, Hit::Card));
        let x = card.pos.x + SHEET_PAD;
        let mut y = card.pos.y + SHEET_PAD;
        self.app_icons.draw(cx, &icon_name(&q.app), place.style, rect(x, y, SHEET_ICON, SHEET_ICON), 1.0, k.ink);
        let tx = x + SHEET_ICON + 14.0;
        let mut ty = y;
        for line in &title_lines {
            self.d.label(cx, rect(tx, ty, text_w, TEXT_LINE), true, TEXT, k.ink, HAlign::Left, line);
            ty += TEXT_LINE;
        }
        ty += 2.0;
        for line in &who_lines {
            self.d.label(cx, rect(tx, ty, text_w, NOTE_LINE), false, NOTE, k.muted, HAlign::Left, line);
            ty += NOTE_LINE;
        }
        y += head_h + 12.0;
        for line in &question_lines {
            self.d.label(cx, rect(x, y, inner, TEXT_LINE), false, TEXT, k.ink, HAlign::Left, line);
            y += TEXT_LINE;
        }
        let picked = self.question_pick.filter(|(id, _)| *id == q.id).map(|(_, i)| i);
        if !options.is_empty() {
            y += 10.0;
            let area = rect(x, y, inner, group_h);
            draw_group(&mut self.d, cx, area, k);
            self.scroller.layout(&format!("question {} {w}", q.id), area, content);
            self.note_clip = Some(area);
            cx.push_clip_rect(area);
            let mut oy = y - self.scroller.offset;
            for (i, lines) in wrapped_options.iter().enumerate() {
                let h = lines.len().max(1) as f64 * TEXT_LINE + 10.0;
                if i > 0 {
                    self.d.solid(cx, rect(x + ROW_INSET, oy, inner - ROW_INSET, 1.0), k.line);
                }
                let band = rect(x, oy, inner, h);
                let on = picked == Some(i);
                radio(&mut self.d, cx, rect(x + ROW_INSET, oy + 6.0, 16.0, 16.0), k, on);
                for (l, text) in lines.iter().enumerate() {
                    self.d.label(cx, rect(x + ROW_INSET + 26.0, oy + 5.0 + l as f64 * TEXT_LINE, option_w, TEXT_LINE), false, TEXT, k.ink, HAlign::Left, text);
                }
                if let Some(r) = clipped(band, area) {
                    self.hits.push((r, Hit::QuestionPick { id: q.id, index: i }));
                }
                self.note_control(&options[i], band, true);
                oy += h;
            }
            cx.pop_clip_rect();
            self.note_clip = None;
            self.scroller.draw_thumb(&mut self.d, cx, k);
        }
        let foot = card.pos.y + card.size.y - SHEET_PAD - CONTROL_H;
        let right = card.pos.x + card.size.x - SHEET_PAD;
        let dont_hit = Hit::QuestionOption { id: q.id, label: None };
        let mut left = right;
        if !options.is_empty() {
            let answer_w = button_width(&mut self.d, cx, "Answer", 72.0);
            let r = rect(right - answer_w, foot, answer_w, CONTROL_H);
            let hit = picked.and_then(|i| options.get(i)).map(|label| Hit::QuestionOption { id: q.id, label: Some(label.clone()) });
            self.mac_button(cx, r, k, if hit.is_some() { Face::Default } else { Face::Disabled }, "Answer", hit);
            left = r.pos.x - 8.0;
            let dont_w = button_width(&mut self.d, cx, dont, 72.0);
            let r = rect(left - dont_w, foot, dont_w, CONTROL_H);
            self.mac_button(cx, r, k, Face::Plain, dont, Some(dont_hit.clone()));
        } else {
            let dont_w = button_width(&mut self.d, cx, dont, 72.0);
            let r = rect(left - dont_w, foot, dont_w, CONTROL_H);
            self.mac_button(cx, r, k, Face::Default, dont, Some(dont_hit.clone()));
        }
        if x + button_width(&mut self.d, cx, &stop, 72.0) + 16.0 > left - button_width(&mut self.d, cx, dont, 72.0) {
            stop = words::STOP_SHORT.to_string();
        }
        let stop_w = button_width(&mut self.d, cx, &stop, 72.0);
        self.mac_button(cx, rect(x, foot, stop_w, CONTROL_H), k, Face::Plain, &stop, Some(Hit::StopAgent { app: q.app.clone() }));
        self.escape = Some(dont_hit);
        // Return sends what was picked: it takes a press to have picked anything.
        self.return_key = picked.and_then(|i| options.get(i)).map(|label| Hit::QuestionOption { id: q.id, label: Some(label.clone()) });
        self.shown.push(heading);
        if let Some(who) = who {
            self.shown.push(who);
        }
        self.shown.extend(question);
        self.shown.extend(options);
    }

    /// The notices at the top right, in the banners the others use and under any
    /// that are up (`shell/notifications.rs`). What expired unanswered stays until
    /// dismissed. While a rule approves everything one app asks, the menu bar's status
    /// item says so and its menu has Stop (`shell/bar.rs`, `mac_menus.rs`); while a
    /// full screen window has the bar out of sight a banner says it instead, with the
    /// minutes left and Stop: this never goes unsaid.
    fn mac_banners(&mut self, cx: &mut Cx2d, screen: Rect, f: &Frame, place: &MacPlace) {
        use crate::shell::notifications::{draw_mac_action, mac_banner_height, mac_banner_place, MAC_ACTION_H, MAC_ICON, MAC_ICON_GAP, MAC_LINE, MAC_PAD, MAC_RADIUS, MAC_WIDTH};
        // A Mac's banner title is one short line that says what happened; why is in its body.
        let mut expired: Vec<(String, String, &str, Hit)> = f.expired.iter().map(|e| ("Request Denied".to_string(), format!("{} {}", e.past, sentence(&e.reason)), e.app.as_str(), Hit::DismissExpired(e.id.clone()))).collect();
        expired.extend(f.expired_questions.iter().filter_map(|q| {
            let crate::questions::State::Expired(reason) = &q.state else { return None };
            Some(("Question Not Answered".to_string(), format!("{} asked a question. {}", capital(&words::agent_named(&q.app)), sentence(reason)), q.app.as_str(), Hit::DismissQuestion(q.id)))
        }));
        let running = f.everything.iter().filter(|_| place.bar_hidden).map(|rule| {
            let left = rule.minutes_left(f.now).unwrap_or(0);
            let text = if left == 1 { "1 minute left".to_string() } else { format!("{left} minutes left") };
            (format!("Approving everything {} asks", app_label(&rule.app)), text, rule.app.as_str(), Hit::StopRule(rule.id.clone()), "Stop")
        });
        let rows = running.chain(expired.into_iter().rev().take(3).map(|(title, text, app, hit)| (title, text, app, hit, "Dismiss")));
        let (x, mut y) = mac_banner_place(screen);
        // In a full screen window they stand under its toolbar: a box can run for an
        // hour, and the toolbar's buttons stay in reach.
        if let Some(win) = place.front.as_ref().filter(|_| place.bar_hidden) {
            y = y.max(win.rect.pos.y + win.top + 8.0);
        }
        let floor = screen.pos.y + screen.size.y - place.dock - 8.0;
        let tok = self.d.tokens(self.tokens);
        let surface = &tok.notifications.surface;
        let ink = surface.text;
        for (shown, (title, text, app, hit, action)) in rows.enumerate() {
            let action_w = (measure(&mut self.d, cx, false, 12.0, action) + 22.0).ceil();
            let width = MAC_WIDTH - MAC_PAD * 2.0 - MAC_ICON - MAC_ICON_GAP - action_w - MAC_ICON_GAP;
            let title_lines = wrapped(&mut self.d, cx, true, TEXT, &title, width);
            let lines = wrapped(&mut self.d, cx, false, TEXT, &text, width);
            let count = title_lines.len() + lines.len();
            let h = mac_banner_height(count);
            if shown > 0 && !matches!(hit, Hit::StopRule(_)) && y + h > floor {
                break;
            }
            let card = rect(x, y, MAC_WIDTH, h);
            if self.d.material().is_glass() {
                self.d.glass_pill(cx, card, MAC_RADIUS, 1.0);
            } else {
                self.d.card_faded(cx, card, surface, 1.0);
            }
            self.app_icons.draw(cx, &icon_name(app), place.style, rect(card.pos.x + MAC_PAD, card.pos.y + (h - MAC_ICON) * 0.5, MAC_ICON, MAC_ICON), 1.0, ink);
            let tx = card.pos.x + MAC_PAD + MAC_ICON + MAC_ICON_GAP;
            let mut ty = card.pos.y + (h - count as f64 * MAC_LINE) * 0.5;
            for line in &title_lines {
                self.d.label(cx, rect(tx, ty, width, MAC_LINE), true, TEXT, ink, HAlign::Left, line);
                ty += MAC_LINE;
            }
            for line in &lines {
                self.d.label(cx, rect(tx, ty, width, MAC_LINE), false, TEXT, crate::shell::fade(ink, 0.85), HAlign::Left, line);
                ty += MAC_LINE;
            }
            let button = rect(card.pos.x + MAC_WIDTH - MAC_PAD - action_w, card.pos.y + (h - MAC_ACTION_H) * 0.5, action_w, MAC_ACTION_H);
            draw_mac_action(&mut self.d, cx, button, ink, place.dark, 1.0, action);
            self.note_control(action, button, true);
            self.hits.push((card, Hit::Card));
            self.hits.push((button, hit));
            self.shown.push(title);
            self.shown.push(text);
            y += h + 8.0;
        }
    }

    /// The pointer on a Mac: a held thumb, a pop-up's menu, then the buttons.
    pub(super) fn mac_pointer(&mut self, cx: &mut Cx, event: &Event) -> bool {
        match event {
            Event::Scroll(e) => {
                if self.popup.is_none() && self.scroller.area.is_some_and(|r| contains(r, e.abs)) {
                    self.scroller.by(e.scroll.y);
                    self.redraw(cx);
                    return true;
                }
                self.modal
            }
            Event::MouseMove(e) => {
                self.pointer_at = Some(e.abs);
                self.popup_cursor = None;
                if self.scroller.dragged(e.abs) {
                    self.redraw(cx);
                    return true;
                }
                let thumb = self.scroller.pointer_at(e.abs);
                let hover = self.hits.iter().rev().find(|(r, h)| *h != Hit::Card && *h != Hit::PopupDismiss && contains(*r, e.abs)).map(|(r, _)| *r);
                if hover != self.hover || self.popup.is_some() || thumb {
                    self.hover = hover;
                    self.redraw(cx);
                }
                self.modal
            }
            Event::MouseDown(e) => self.mac_down(cx, e.abs),
            Event::MouseUp(e) => self.mac_up(cx, e.abs),
            Event::TouchUpdate(e) => {
                use makepad_widgets::makepad_platform::event::TouchState;
                let mut taken = self.modal;
                for touch in &e.touches {
                    match touch.state {
                        TouchState::Start => taken |= self.mac_down(cx, touch.abs),
                        TouchState::Stop => taken |= self.mac_up(cx, touch.abs),
                        _ => {}
                    }
                }
                taken
            }
            _ => self.modal,
        }
    }

    fn mac_down(&mut self, cx: &mut Cx, p: Vec2d) -> bool {
        if self.popup.is_none() && self.scroller.press(p) {
            return true;
        }
        self.pointer_at = Some(p);
        // A pop-up opens on the press: its release may be the end of a drag to a row.
        if let Some(hit @ Hit::Popup { .. }) = self.hit_at(p).filter(|_| self.popup.is_none()) {
            self.mac_press(hit);
            self.popup_press = Some(p);
            self.redraw(cx);
            return true;
        }
        self.down = self.hit_at(p);
        self.down_rect = self.hits.iter().rev().find(|(r, h)| *h != Hit::Card && contains(*r, p)).map(|(r, _)| *r);
        self.hover = self.down_rect;
        self.redraw(cx);
        self.modal || self.down.is_some()
    }

    fn mac_up(&mut self, cx: &mut Cx, p: Vec2d) -> bool {
        if self.scroller.release() {
            return true;
        }
        // The press that opened a pop-up: a release where it came down leaves the
        // menu open, and one after a drag chooses the row it ends on, or nothing.
        if let Some(from) = self.popup_press.take() {
            if (p.x - from.x).abs().max((p.y - from.y).abs()) >= DRAG_SLOP {
                match self.hit_at(p) {
                    Some(hit @ Hit::PopupChoose { .. }) => self.mac_press(hit),
                    _ => self.popup = None,
                }
            }
            self.redraw(cx);
            return true;
        }
        let hit = self.hit_at(p);
        let pressed = self.down.take();
        self.down_rect = None;
        if let (Some(h), Some(d)) = (&hit, &pressed) {
            if h == d {
                self.mac_press(h.clone());
            }
        }
        self.redraw(cx);
        self.modal || hit.is_some()
    }

    /// A press that landed. What only changes the sheet stays here; answers go to `act`.
    pub(super) fn mac_press(&mut self, hit: Hit) {
        match hit {
            Hit::Popup { request, button } => {
                let open = OpenPopup { request, button };
                self.popup = if self.popup.as_ref() == Some(&open) { None } else { Some(open) };
                self.popup_cursor = None;
            }
            Hit::PopupChoose { request, answer } => {
                self.choices.insert(request, answer);
                self.popup = None;
            }
            Hit::PopupDismiss => self.popup = None,
            Hit::QuestionPick { id, index } => self.question_pick = Some((id, index)),
            hit => {
                self.popup = None;
                act(hit);
            }
        }
    }

    /// The keyboard while a card is up on a Mac: Escape is the Cancel-position
    /// button; the arrows and the page keys move what scrolls. Return never
    /// approves: these cards come up on their own, while the person may be typing.
    /// It presses the default where that refuses, and sends what was picked. In an
    /// open pop-up the arrows move along its rows, Return chooses and Escape closes it.
    pub fn key(&mut self, cx: &mut Cx, e: &KeyEvent) -> bool {
        if self.mac.is_none() || !self.modal {
            return false;
        }
        if self.popup.is_some() {
            let rows: Vec<Hit> = self.hits.iter().filter(|(_, hit)| matches!(hit, Hit::PopupChoose { .. })).map(|(_, hit)| hit.clone()).collect();
            let chosen = self.popup.as_ref().and_then(|open| {
                let answer = self.choices.get(&open.request).copied().unwrap_or(Answer::Once);
                rows.iter().position(|row| matches!(row, Hit::PopupChoose { answer: a, .. } if *a == answer))
            });
            match e.key_code {
                KeyCode::Escape => self.popup = None,
                KeyCode::ArrowDown | KeyCode::ArrowUp => {
                    let by = if e.key_code == KeyCode::ArrowDown { 1 } else { -1 };
                    self.popup_cursor = menu_step(self.popup_cursor, chosen, rows.len(), by);
                    // The pointer may rest on another row: the keys have it now.
                    self.pointer_at = None;
                }
                KeyCode::ReturnKey | KeyCode::NumpadEnter | KeyCode::Space => {
                    match self.popup_cursor.and_then(|i| rows.get(i).cloned()) {
                        Some(row) => self.mac_press(row),
                        None => self.popup = None,
                    }
                }
                _ => {}
            }
            self.redraw(cx);
            return true;
        }
        let page = self.scroller.page();
        match e.key_code {
            KeyCode::Escape => {
                if let Some(hit) = self.escape.clone() {
                    self.mac_press(hit);
                    self.redraw(cx);
                }
                true
            }
            KeyCode::ReturnKey | KeyCode::NumpadEnter => {
                if let Some(hit) = self.return_key.clone() {
                    self.mac_press(hit);
                    self.redraw(cx);
                }
                true
            }
            KeyCode::ArrowDown => self.scrolled(cx, TEXT_LINE * 2.0),
            KeyCode::ArrowUp => self.scrolled(cx, -TEXT_LINE * 2.0),
            KeyCode::PageDown => self.scrolled(cx, page),
            KeyCode::PageUp => self.scrolled(cx, -page),
            _ => false,
        }
    }

    fn scrolled(&mut self, cx: &mut Cx, by: f64) -> bool {
        self.scroller.by(by);
        self.redraw(cx);
        true
    }
}

/// "no answer in 4 seconds" as a sentence: "No answer in 4 seconds."
fn sentence(text: &str) -> String {
    let text = capital(text.trim());
    if text.ends_with(['.', '!', '?']) { text } else { format!("{text}.") }
}

fn capital(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
