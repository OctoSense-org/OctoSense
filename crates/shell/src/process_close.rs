//! Closing a PROCESS client that may ask the person first (OctoSense#179).
//!
//! makepad#65 made the terminal refuse a `WmEvent::CloseRequested` while
//! jobs run and ask in its own window; it quits itself on a yes. The
//! shell used to follow `CloseRequested` with `Kill` in the same breath,
//! so closing a process terminal's tile still ended its jobs. The fix
//! needs the shell to WAIT for the app's answer, and to know a live app
//! that is asking from a hung one. Two additive `wm_api` requests carry
//! that (makepad `WmRequest::AsksBeforeClose`, `WmRequest::CloseRefused`):
//!
//! - An app that sends `AsksBeforeClose` (in reply to `Hosted`) is
//!   *declared*. Only a declared app is asked and waited for; every other
//!   app keeps today's close — `CloseRequested` + `Kill`, the tile leaves
//!   at once, the reaper as fallback — so apps that never learned the new
//!   words close exactly as promptly as before.
//! - A close of a declared app sends `CloseRequested` alone. Its tile and
//!   process stay. The app answers yes with its own `WmRequest::Close` (a
//!   hosted app cannot end its process with `cx.quit()`; the Studio
//!   runtime ignores it), which the shell carries out at once and without
//!   asking back, or with `CloseRefused` (it is showing its question: the
//!   close waits, with no deadline, for as long as the person takes; a no
//!   leaves everything as it was, a yes later is again a `Close`). An exit
//!   also answers.
//! - The fallback for an app that does not answer: after
//!   [`ANSWER_TIMEOUT`] with neither an exit nor a refusal it is ended as
//!   a close always ended apps. The person need not wait for that: closing
//!   the same tile again while the first close is still unanswered ends it
//!   at once (a repeat inside [`REPEAT_WINDOW`] is the same click, not
//!   insistence).
//! - A refusal is never a licence to hang forever: every close is a fresh
//!   question. Closing a tile whose app is asking asks again; a live app
//!   answers again at once (the terminal re-shows its question), a frozen
//!   one does not and falls to the fallback above. So an app that is
//!   visibly showing its question is not killed for one close, and a hung
//!   one is always closable.
//! - **Insisting ends it** ([`Insistence`]): an app that keeps refusing
//!   could otherwise never be closed, and would hold a quit forever. The
//!   [`FORCE_CLOSES`]th close of the same app inside [`FORCE_WINDOW`]
//!   seconds (clicks inside [`REPEAT_WINDOW`] of each other count once)
//!   ends it whatever it answers; the shell says so on the close before.
//!   A quit counts the same way for each app it asks, so pressing Quit
//!   three times ends the ones still refusing. Hosted modules follow the
//!   same rule (`module_host::CloseGate`).
//!
//! A shell quit (the menu's Quit, Cmd+Q, the window's close button) asks
//! every declared app the same way and waits while any is answering or
//! asking; it goes ahead when the last one has said yes (or gone). A termination
//! signal or a crash of the shell asks nobody, as before.

use crate::hub::ClientId;
use crate::module_host::CloseGate;
use std::collections::{HashMap, HashSet};

/// How long a declared app has to answer a close (exit or refuse) before
/// the shell ends it anyway. Generous on purpose: the only apps it catches
/// are ones that promised to answer and did not (hung), while killing a
/// slow but live app here would lose exactly what the question protects.
/// The person can end a hung app sooner by closing it again.
pub const ANSWER_TIMEOUT: f64 = 5.0;

/// A second close of the same tile this soon after the first is the same
/// gesture (a double click), not a demand to force it.
pub const REPEAT_WINDOW: f64 = 0.5;

/// This many closes of one app inside [`FORCE_WINDOW`] end it, whatever it
/// answers.
pub const FORCE_CLOSES: usize = 3;

/// Seconds within which [`FORCE_CLOSES`] closes count as insisting.
pub const FORCE_WINDOW: f64 = 5.0;

/// The person's closes of each app, to tell insisting from asking once.
#[derive(Debug, Default)]
pub struct Insistence {
    closes: HashMap<ClientId, Vec<f64>>,
}

impl Insistence {
    /// The person closes `client` at `now`: how many of their closes of it
    /// count inside [`FORCE_WINDOW`], this one included. A close inside
    /// [`REPEAT_WINDOW`] of the last counted one is the same click.
    pub fn close(&mut self, client: ClientId, now: f64) -> usize {
        let closes = self.closes.entry(client).or_default();
        closes.retain(|t| now - *t < FORCE_WINDOW);
        if closes.last().is_none_or(|last| now - *last >= REPEAT_WINDOW) {
            closes.push(now);
        }
        closes.len()
    }

    /// Whether that many closes end the app.
    pub fn forced(closes: usize) -> bool {
        closes >= FORCE_CLOSES
    }

    /// The closes counted for `client` so far (for the hint before the last).
    pub fn count(&self, client: ClientId) -> usize {
        self.closes.get(&client).map_or(0, Vec::len)
    }

    pub fn forget(&mut self, client: ClientId) {
        self.closes.remove(&client);
    }
}

/// Where one declared app's close stands.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Pending {
    /// `CloseRequested` went out at `since`; no answer yet.
    Awaiting { since: f64 },
    /// The app refused and is asking the person.
    Asking,
}

/// What the shell does for one close of a process client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseStep {
    /// Not declared: today's close (`CloseRequested` + `Kill`, the tile
    /// leaves the layout at once, the reaper kills after `CLOSE_GRACE`).
    Legacy,
    /// Send `CloseRequested` alone and wait; the tile stays, in front.
    Ask,
    /// Already asked a moment ago: nothing new to do.
    Wait,
    /// Asked, unanswered, and asked again: end it now.
    Force,
}

/// What a shell quit asked of the declared apps.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct QuitAsk {
    /// Sent `CloseRequested`; the quit waits on their answers.
    pub asked: Vec<ClientId>,
    /// Unanswered since an earlier ask: end them now.
    pub forced: Vec<ClientId>,
}

/// An app's word about closes (makepad `WmRequest::AsksBeforeClose` /
/// `WmRequest::CloseRefused`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseWord {
    AsksBeforeClose,
    CloseRefused,
}

/// Recognise the two close words on the wire by their exact text, so the
/// shell understands an app that sends them whether or not the makepad it
/// is pinned to has the variants yet (an older `WmRequest::parse` rejects
/// them and they would fall through to the AI bus as noise).
pub fn parse_close_word(json: &str) -> Option<CloseWord> {
    if !json.contains("\"wm\"") {
        return None;
    }
    let compact: String = json.chars().filter(|c| !c.is_whitespace()).collect();
    match compact.as_str() {
        r#"{"wm":{"AsksBeforeClose":[]}}"# | r#"{"wm":"AsksBeforeClose"}"# => Some(CloseWord::AsksBeforeClose),
        r#"{"wm":{"CloseRefused":[]}}"# | r#"{"wm":"CloseRefused"}"# => Some(CloseWord::CloseRefused),
        _ => None,
    }
}

/// The shell's book of declared process apps and the closes they are
/// answering (see the module docs). Pure: the caller sends, kills and
/// removes tiles; this only decides.
#[derive(Debug, Default)]
pub struct ProcessCloseGate {
    declared: HashSet<ClientId>,
    pending: HashMap<ClientId, Pending>,
    quit_waiting: bool,
    insistence: Insistence,
}

impl ProcessCloseGate {
    /// The app sent `AsksBeforeClose`: its closes are asked and awaited.
    pub fn declare(&mut self, client: ClientId) {
        self.declared.insert(client);
    }

    pub fn is_declared(&self, client: ClientId) -> bool {
        self.declared.contains(&client)
    }

    /// The person closes one app. A quit that was waiting is abandoned
    /// (the person turned to something else), as for a hosted module.
    pub fn close(&mut self, client: ClientId, now: f64) -> CloseStep {
        self.quit_waiting = false;
        self.step(client, now)
    }

    fn step(&mut self, client: ClientId, now: f64) -> CloseStep {
        if !self.declared.contains(&client) {
            return CloseStep::Legacy;
        }
        // Insisting ends it, whatever it answered (an app that keeps
        // refusing is never unclosable).
        if Insistence::forced(self.insistence.close(client, now)) {
            self.pending.remove(&client);
            self.insistence.forget(client);
            return CloseStep::Force;
        }
        match self.pending.get(&client) {
            Some(Pending::Awaiting { since }) if now - since < REPEAT_WINDOW => CloseStep::Wait,
            Some(Pending::Awaiting { .. }) => {
                self.pending.remove(&client);
                CloseStep::Force
            }
            // Idle, or asking the person: ask (again). A live app answers.
            Some(Pending::Asking) | None => {
                self.pending.insert(client, Pending::Awaiting { since: now });
                CloseStep::Ask
            }
        }
    }

    /// The app sent `CloseRefused`. `true` when it answered a close this
    /// gate is waiting on (it is asking the person now); a refusal nobody
    /// asked for is ignored.
    pub fn refused(&mut self, client: ClientId) -> bool {
        match self.pending.get_mut(&client) {
            Some(state @ Pending::Awaiting { .. }) => {
                *state = Pending::Asking;
                true
            }
            Some(Pending::Asking) => true,
            None => false,
        }
    }

    /// The closes nobody answered within [`ANSWER_TIMEOUT`]: the caller
    /// ends each. An app that is asking the person is never among them.
    pub fn overdue(&mut self, now: f64) -> Vec<ClientId> {
        let mut late: Vec<ClientId> = self
            .pending
            .iter()
            .filter(|(_, p)| matches!(p, Pending::Awaiting { since } if now - since >= ANSWER_TIMEOUT))
            .map(|(client, _)| *client)
            .collect();
        late.sort_unstable();
        for client in &late {
            self.pending.remove(client);
        }
        late
    }

    /// The client is gone or going on its own word (exited, killed,
    /// removed, or it sent `WmRequest::Close`). `true` when a close of it
    /// was pending: that was its answer, not a crash.
    pub fn gone(&mut self, client: ClientId) -> bool {
        self.declared.remove(&client);
        self.insistence.forget(client);
        self.pending.remove(&client).is_some()
    }

    /// Whether the next close of `client` ends it (the shell says so when
    /// it asks this time).
    pub fn next_close_forces(&self, client: ClientId) -> bool {
        self.declared.contains(&client) && Insistence::forced(self.insistence.count(client) + 1)
    }

    /// Whether a close of `client` is waiting on its answer or its person.
    pub fn is_pending(&self, client: ClientId) -> bool {
        self.pending.contains_key(&client)
    }

    /// How many apps are asking the person right now.
    pub fn asking(&self) -> usize {
        self.pending.values().filter(|p| **p == Pending::Asking).count()
    }

    /// Whether `client` refused and is asking the person.
    pub fn is_asking(&self, client: ClientId) -> bool {
        self.pending.get(&client) == Some(&Pending::Asking)
    }

    /// The shell is quitting: ask every live declared app among `clients`
    /// (the caller passes the windows the person has). Undeclared apps
    /// are not asked: the shutdown ends them as it always did.
    pub fn quit(&mut self, clients: impl IntoIterator<Item = ClientId>, now: f64) -> QuitAsk {
        let mut clients: Vec<ClientId> = clients.into_iter().collect();
        clients.sort_unstable();
        clients.dedup();
        let mut ask = QuitAsk::default();
        for client in clients {
            match self.step(client, now) {
                CloseStep::Ask => ask.asked.push(client),
                CloseStep::Force => ask.forced.push(client),
                CloseStep::Wait | CloseStep::Legacy => {}
            }
        }
        self.quit_waiting = !self.pending.is_empty();
        ask
    }

    /// A quit is waiting on apps here.
    pub fn quit_waiting(&self) -> bool {
        self.quit_waiting
    }

    /// Nothing is pending: no app is answering or asking.
    pub fn idle(&self) -> bool {
        self.pending.is_empty()
    }

    /// Drop a waiting quit (a new close of one app, or the quit went).
    pub fn abandon_quit(&mut self) {
        self.quit_waiting = false;
    }
}

/// A shell quit waits on both kinds of client that may ask: hosted
/// modules (`modules`, #186) and process apps (`apps`). It may go once one
/// of them is waiting and neither has anyone left answering or asking;
/// taking it clears both.
pub fn take_quit_ready(modules: &mut CloseGate, apps: &mut ProcessCloseGate) -> bool {
    let waiting = modules.quit_waiting() || apps.quit_waiting();
    if !waiting || !modules.idle() || !apps.idle() {
        return false;
    }
    modules.close_asked();
    apps.abandon_quit();
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use makepad_widgets::WidgetUid;

    fn declared(clients: &[ClientId]) -> ProcessCloseGate {
        let mut gate = ProcessCloseGate::default();
        for client in clients {
            gate.declare(*client);
        }
        gate
    }

    #[test]
    fn the_close_words_are_read_off_the_wire() {
        // What makepad's `WmRequest::{AsksBeforeClose,CloseRefused}.to_json()` sends.
        assert_eq!(parse_close_word(r#"{"wm":{"AsksBeforeClose":[]}}"#), Some(CloseWord::AsksBeforeClose));
        assert_eq!(parse_close_word(r#"{"wm":{"CloseRefused":[]}}"#), Some(CloseWord::CloseRefused));
        assert_eq!(parse_close_word(r#"{ "wm" : "CloseRefused" }"#), Some(CloseWord::CloseRefused));
        assert_eq!(parse_close_word(r#"{"wm":{"Close":[]}}"#), None);
        assert_eq!(parse_close_word(r#"{"wm":{"Title":{"title":"CloseRefused"}}}"#), None);
        assert_eq!(parse_close_word("CloseRefused"), None);
    }

    #[test]
    fn an_app_that_never_declared_closes_as_before() {
        let mut gate = ProcessCloseGate::default();
        assert_eq!(gate.close(7, 0.0), CloseStep::Legacy);
        assert!(!gate.is_pending(7), "nothing waits on it");
        assert!(gate.overdue(100.0).is_empty());
        assert!(!gate.refused(7), "a stray refusal from it is ignored");
    }

    #[test]
    fn a_declared_app_is_asked_and_its_exit_is_the_yes() {
        let mut gate = declared(&[3]);
        assert_eq!(gate.close(3, 10.0), CloseStep::Ask);
        assert!(gate.is_pending(3));
        // It quit itself (nothing running, or the person said yes).
        assert!(gate.gone(3), "the exit answered a pending close: not a crash");
        assert!(!gate.is_pending(3) && !gate.is_declared(3));
        assert!(gate.idle());
    }

    #[test]
    fn a_refusal_keeps_it_with_no_deadline_and_a_confirmed_exit_removes_it() {
        let mut gate = declared(&[3]);
        assert_eq!(gate.close(3, 0.0), CloseStep::Ask);
        assert!(gate.refused(3));
        assert!(gate.is_asking(3));
        // The person takes their time, or says no: never ended for it.
        assert!(gate.overdue(ANSWER_TIMEOUT * 100.0).is_empty(), "an app asking is never killed");
        assert!(gate.is_asking(3), "a no leaves the question's mark");
        // Later: the person said yes in the app; it quits; the tile goes.
        assert!(gate.gone(3));
    }

    #[test]
    fn an_app_that_never_answers_is_ended_after_the_timeout() {
        let mut gate = declared(&[3, 4]);
        assert_eq!(gate.close(3, 0.0), CloseStep::Ask);
        assert_eq!(gate.close(4, 1.0), CloseStep::Ask);
        assert!(gate.overdue(ANSWER_TIMEOUT - 0.01).is_empty(), "not yet");
        assert_eq!(gate.overdue(ANSWER_TIMEOUT), vec![3]);
        assert!(!gate.is_pending(3), "handed to the caller to end");
        assert_eq!(gate.overdue(ANSWER_TIMEOUT + 1.0), vec![4]);
        assert!(gate.idle());
    }

    #[test]
    fn a_second_close_of_an_unanswered_app_forces_it() {
        let mut gate = declared(&[3]);
        assert_eq!(gate.close(3, 0.0), CloseStep::Ask);
        assert_eq!(gate.close(3, REPEAT_WINDOW / 2.0), CloseStep::Wait, "a double click is one close");
        assert_eq!(gate.close(3, REPEAT_WINDOW), CloseStep::Force);
        assert!(!gate.is_pending(3));
    }

    #[test]
    fn a_second_close_of_an_app_that_is_asking_asks_again() {
        let mut gate = declared(&[3]);
        gate.close(3, 0.0);
        gate.refused(3);
        // Its question is up; the person closes the tile again. The app is
        // asked again, not killed: a live app re-answers at once.
        assert_eq!(gate.close(3, 30.0), CloseStep::Ask);
        assert!(gate.refused(3));
        assert!(gate.is_asking(3));
        // A frozen one does not re-answer: the next close forces it, or the
        // timeout ends it.
        assert_eq!(gate.close(3, 40.0), CloseStep::Ask);
        assert_eq!(gate.close(3, 41.0), CloseStep::Force);
        gate.close(3, 50.0);
        assert_eq!(gate.overdue(50.0 + ANSWER_TIMEOUT), vec![3]);
    }

    /// An app that keeps refusing is never unclosable: the third close
    /// inside the window ends it, and the one before says so.
    #[test]
    fn an_app_that_keeps_refusing_is_ended_by_the_third_close() {
        let mut gate = declared(&[3]);
        assert_eq!(gate.close(3, 0.0), CloseStep::Ask);
        assert!(gate.refused(3));
        assert!(!gate.next_close_forces(3));
        assert_eq!(gate.close(3, 1.0), CloseStep::Ask, "asked again: a live app re-answers");
        assert!(gate.refused(3));
        assert!(gate.next_close_forces(3), "the shell warns before the last one");
        assert_eq!(gate.close(3, 2.0), CloseStep::Force, "the third close ends it");
        assert!(!gate.is_pending(3) && gate.idle());
    }

    #[test]
    fn closes_spread_out_or_double_clicked_do_not_force() {
        let mut gate = declared(&[3]);
        // A double click counts once.
        assert_eq!(gate.close(3, 0.0), CloseStep::Ask);
        gate.refused(3);
        assert_eq!(gate.close(3, 0.1), CloseStep::Ask);
        gate.refused(3);
        assert_eq!(gate.close(3, 0.2), CloseStep::Ask, "one click, not three");
        gate.refused(3);
        // Closes further apart than the window never add up.
        for t in [10.0, 16.0, 22.0, 28.0] {
            assert_eq!(gate.close(3, t), CloseStep::Ask, "{t}");
            gate.refused(3);
        }
        // An app that went starts from nothing if its id came back.
        gate.gone(3);
        gate.declare(3);
        assert!(!gate.next_close_forces(3));
    }

    #[test]
    fn pressing_quit_three_times_ends_the_apps_still_refusing() {
        let mut gate = declared(&[1, 2]);
        assert_eq!(gate.quit([1, 2], 0.0).asked, vec![1, 2]);
        gate.refused(1);
        gate.refused(2);
        assert_eq!(gate.quit([1, 2], 1.0).asked, vec![1, 2]);
        gate.refused(1);
        gate.gone(2); // the person said yes in 2
        let third = gate.quit([1], 2.0);
        assert_eq!(third, QuitAsk { asked: vec![], forced: vec![1] });
        assert!(gate.idle(), "nothing holds the quit now");
    }

    #[test]
    fn quit_waits_for_every_declared_app_and_goes_when_the_last_exits() {
        let mut gate = declared(&[1, 2, 3]);
        // 9 never declared: the shutdown ends it, nobody waits on it.
        let ask = gate.quit([3, 1, 2, 9], 0.0);
        assert_eq!(ask, QuitAsk { asked: vec![1, 2, 3], forced: vec![] });
        assert!(gate.quit_waiting());
        gate.gone(1); // nothing running: quit itself
        assert!(gate.refused(2)); // asking the person
        assert!(gate.refused(3));
        assert!(!gate.idle(), "the quit waits");
        gate.gone(2); // yes
        assert!(!gate.idle());
        gate.gone(3); // yes
        assert!(gate.idle() && gate.quit_waiting(), "the quit may go now");
        gate.abandon_quit();
        assert!(!gate.quit_waiting());
    }

    #[test]
    fn quit_waits_for_modules_and_process_apps_together() {
        let mut modules = CloseGate::default();
        let mut apps = declared(&[5]);
        let root = WidgetUid(42);
        // One in-process terminal refuses; one process terminal is asked.
        assert!(!modules.quit_asked([(4, root)]));
        assert_eq!(apps.quit([5], 0.0).asked, vec![5]);
        assert!(!take_quit_ready(&mut modules, &mut apps));
        // The process one says yes and exits: the module still asks.
        apps.gone(5);
        assert!(!take_quit_ready(&mut modules, &mut apps), "the module is still asking");
        // The module confirms: now the quit goes, once.
        assert_eq!(modules.confirmed(root), Some(4));
        assert!(take_quit_ready(&mut modules, &mut apps));
        assert!(!take_quit_ready(&mut modules, &mut apps), "taken");

        // The other way round: only process apps hold it.
        let mut modules = CloseGate::default();
        let mut apps = declared(&[6]);
        assert!(modules.quit_asked(std::iter::empty()));
        apps.quit([6], 0.0);
        apps.refused(6);
        assert!(!take_quit_ready(&mut modules, &mut apps));
        apps.gone(6);
        assert!(take_quit_ready(&mut modules, &mut apps));

        // Nobody waiting: nothing to take (an ordinary close emptied it).
        let mut apps = declared(&[7]);
        apps.close(7, 0.0);
        apps.gone(7);
        assert!(!take_quit_ready(&mut CloseGate::default(), &mut apps));
    }

    #[test]
    fn a_quit_with_no_declared_app_goes_at_once() {
        let mut gate = ProcessCloseGate::default();
        assert_eq!(gate.quit([1, 2], 0.0), QuitAsk::default());
        assert!(!gate.quit_waiting() && gate.idle());
    }

    #[test]
    fn a_no_in_one_app_holds_the_quit_and_a_close_abandons_it() {
        let mut gate = declared(&[1, 2]);
        gate.quit([1, 2], 0.0);
        gate.refused(1);
        gate.gone(2);
        assert!(!gate.idle(), "1 is still asking (the person said no)");
        // The person turns to closing one app: the waiting quit is dropped,
        // so confirming later ends only that app, never the shell.
        assert_eq!(gate.close(1, 20.0), CloseStep::Ask);
        assert!(!gate.quit_waiting());
    }

    #[test]
    fn a_hung_app_holding_a_quit_falls_to_the_timeout_or_a_second_quit() {
        let mut gate = declared(&[1, 2]);
        gate.quit([1, 2], 0.0);
        gate.refused(1);
        // 2 hangs. A second Cmd+Q forces it; 1 (asking) is asked again.
        let again = gate.quit([1, 2], 2.0);
        assert_eq!(again, QuitAsk { asked: vec![1], forced: vec![2] });
        assert!(gate.quit_waiting(), "1 still has to answer");
        // Or, left alone, the timeout ends an unanswered one.
        assert_eq!(gate.overdue(2.0 + ANSWER_TIMEOUT), vec![1]);
        assert!(gate.idle());
    }
}
