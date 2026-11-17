//! System apps' text against the host's page in every style, light and dark.
//! On a phone a hosted page shows the host's background
//! (`theme.color_bg_app`), not the app's own, so a fixed dark text colour
//! vanishes when the host is dark (apps/AGENTS.md).
use crate::octosense::style::{load_sheet, DesktopStyle};
use makepad_widgets::{desktop_style, *};

const NEWS: &str = include_str!("../../../apps/news/bundle/main.splash");

#[test]
fn mail_touch_composer_retains_edits_and_does_not_revive_a_discarded_review() {
    let source = include_str!("../../../apps/mail/bundle/main.splash");
    let function = |name: &str| {
        let start = source.find(&format!("\nfn {name}(")).expect("Mail function") + 1;
        let end = source[start..].find("\n}\n").expect("Mail function end") + 2;
        &source[start..start + end]
    };
    let mut cx = Cx::new(Box::new(|_, _| {}));
    cx.with_vm(|vm| {
        makepad_widgets::script_mod(vm);
        vm.bx.captured_errors = Some(Vec::new());
        let value = vm.eval(ScriptMod {
            file: "mail_touch_composer_test.splash".into(),
            code: format!(r#"
use mod.std.regex
let ui_desk = false
let account = {{id: "local-account" address: "local@example.com"}}
let folder = {{id: "INBOX" name: "Inbox"}}
let open_id = ""
let reply_message = ""
let reading = {{address: "sender@example.com" subject: "Hello" body: "Quoted words"}}
let compose_id = ""
let compose_retrying = ""
let compose_revision = 0
let compose_generation = 0
let preparing_review = false
let touch_composing = false
let touch_ask = false
let touch_original_to = ""
let touch_original_subject = ""
let touch_original_body = ""
let touch_note = ""
let touch_offer_sent = false
let touch_checking_sent = false
let touch_sent_request = 0
let touch_review_timer = nil
let touch_waiting = false
let touch_sent_to = ""
let touch_sent_subject = ""
let touch_sent_body = ""
let touch_status_timer = nil
let touch_status_busy = false
let touch_status_started = 0
let touch_status_serial = 0
let touch_run = 1
mod.mail_touch_run = 1
let to = ""
let subject = ""
let body = ""
let notice = ""
let discard_note = ""
let asking = false
let pane_name = ""
let status_text = ""
let review_on = true
let review_off = false
let sent_on = false
let sent_off = false
let mailbox_title = ""
let messages = []
let callbacks = []
let requests = []
let timeouts = []
let ui = {{
    c_to: {{text: fn() {{ to }} set_text: fn(value) {{ to = value }} }}
    c_subject: {{text: fn() {{ subject }} set_text: fn(value) {{ subject = value }} }}
    c_body: {{text: fn() {{ body }} set_text: fn(value) {{ body = value }} }}
    c_note: {{set_text: fn(value) {{ notice = value }} }}
    touch_discard: {{set_visible: fn(value) {{ asking = value }} }}
    touch_discard_note: {{set_text: fn(value) {{ discard_note = value }} }}
    touch_review_on: {{set_visible: fn(value) {{ review_on = value }} }}
    touch_review_off: {{set_visible: fn(value) {{ review_off = value }} }}
    touch_check_sent: {{set_visible: fn(value) {{ sent_on = value }} }}
    touch_check_sent_off: {{set_visible: fn(value) {{ sent_off = value }} }}
    title: {{set_text: fn(value) {{ mailbox_title = value }} }}
    rows: {{render: fn() {{}} }}
}}
let host = {{request: fn(method, args, callback) {{ requests.push({{method: method args: args}}) callbacks.push(callback) }} }}
fn pane(name) {{ pane_name = name }}
fn back_to_inbox() {{ pane_name = "inbox" }}
fn status(value) {{ status_text = value }}
fn start_interval(seconds, callback) {{ 11 }}
fn start_timeout(seconds, callback) {{ timeouts.push(callback) 12 }}
fn stop_timer(id) {{}}
fn time_now() {{ 100 }}
fn open_folder(value) {{ folder = value pane_name = "inbox" }}
fn touch_next_folder_generation() {{}}
fn touch_forget_reading() {{}}
fn touch_remove_blocking() {{ false }}
fn list() {{}}
fn sync() {{}}
{}
{}
{}
{}
{}
{}
{}
{}
{}
{}
{}
{}
{}
{}
{}
{}
{}
{}
{}
{}
{}
{}
{}
{}
{}
compose("local@example.com", "Subject", "")
body = "Unsent text"
touch_remember()
send()
send()
let pending = preparing_review && mod.mail_touch_compose.preparing && callbacks.len() == 1
// Simulate a new script run while the review request is still pending.
touch_composing = false
preparing_review = false
to = ""
subject = ""
body = ""
let restored = touch_restore() && pane_name == "composer" && body == "Unsent text" && preparing_review
// The earlier run's callback may finish before another edit in this run.
let saved = mod.mail_touch_compose
saved.preparing = false
saved.waiting = true
saved.compose_id = "review-id"
saved.revision = 2
saved.note = "Review opened"
mod.mail_touch_compose = saved
body = "More text"
touch_remember()
let reconciled = !preparing_review && touch_waiting && !review_on && review_off && compose_id == "review-id" && compose_revision == 2 &&
    mod.mail_touch_compose.body == "More text" && notice == "Review opened"
touch_cancel()
let asked = asking && touch_ask && body == "More text" &&
    discard_note == "Your changes won't be kept. This doesn't cancel a send review."
touch_keep_editing()
let kept = !asking && body == "More text"
touch_cancel()
touch_discard_message()
let callback = callbacks[0]
callback({{is_ok: true data: {{compose_id: "late" revision: 3}}}})
let discarded = pane_name == "inbox" && mod.mail_touch_compose == nil
compose("", "", "")
touch_cancel()
let clean_closed = pane_name == "inbox" && mod.mail_touch_compose == nil
compose("local@example.com", "Subject", "")
send()
let other_account = mod.mail_touch_compose
other_account.account = "different-account"
mod.mail_touch_compose = other_account
let stale = callbacks[1]
stale({{is_ok: true data: {{compose_id: "wrong-account" revision: 4}}}})
touch_follow_review()
let account_bound = mod.mail_touch_compose.account == "different-account" && mod.mail_touch_compose.preparing && touch_review_timer == nil
mod.mail_touch_compose = nil
touch_composing = false
preparing_review = false
open_id = "message-one"
mod.mail_touch_reading = {{account: account.id account_data: account folder: folder id: open_id message: reading}}
reply()
touch_composing = false
reply_message = ""
open_id = ""
let reply_restored = touch_restore() && reply_message == "message-one" && open_id == "message-one"
touch_cancel()
let untouched_reply_closed = pane_name == "inbox" && mod.mail_touch_compose == nil
compose("", "", "")
body = "Unsent text"
touch_remember()
touch_cancel()
let ordinary_note = discard_note == "Your changes won't be kept."
touch_discard_message()
compose("local@example.com", "Status", "Original")
send()
let opened = callbacks[callbacks.len() - 1]
opened({{is_ok: true data: {{compose_id: "status-review" revision: 1}}}})
let before_duplicate = callbacks.len()
send()
let waiting_blocks_retry = touch_waiting && !review_on && review_off && callbacks.len() == before_duplicate
touch_status_result("outcome_unknown")
let unknown = !touch_waiting && review_on && body == "Original" &&
    notice == "Mail can't tell whether this was sent. Check Sent before you try again."
touch_discard_message()
compose("local@example.com", "Status", "Original")
send()
let newer_opened = callbacks[callbacks.len() - 1]
newer_opened({{is_ok: true data: {{compose_id: "newer-review" revision: 1}}}})
body = "Newer edits"
touch_remember()
touch_status_result("accepted")
let newer_kept = pane_name == "composer" && body == "Newer edits" && !touch_waiting &&
    notice == "Message sent. Your newer changes weren't sent."
touch_discard_message()
compose("local@example.com", "Status", "Original")
send()
let accepted_opened = callbacks[callbacks.len() - 1]
accepted_opened({{is_ok: true data: {{compose_id: "accepted-review" revision: 1}}}})
touch_status_result("accepted")
let accepted = pane_name == "inbox" && status_text == "Message sent" && mod.mail_touch_compose == nil
compose("local@example.com", "Status", "Original")
send()
let draft_opened = callbacks[callbacks.len() - 1]
draft_opened({{is_ok: true data: {{compose_id: "draft-review" revision: 1}}}})
touch_status_result("draft")
let cancelled = !touch_waiting && review_on && compose_id == "draft-review" &&
    notice == "Not sent. You can change the message and try again."
touch_discard_message()
compose("local@example.com", "Status", "Original")
send()
let failed_opened = callbacks[callbacks.len() - 1]
failed_opened({{is_ok: true data: {{compose_id: "failed-review" revision: 1}}}})
touch_status_result("failed_before_delivery")
let failed = !touch_waiting && review_on && body == "Original" &&
    notice == "Not sent: the mail server didn't take it. Check the addresses and try again."
touch_discard_message()
compose("local@example.com", "Status", "Original")
send()
let stale_opened = callbacks[callbacks.len() - 1]
stale_opened({{is_ok: true data: {{compose_id: "stale-review" revision: 1}}}})
touch_poll_status()
let late_status = callbacks[callbacks.len() - 1]
touch_discard_message()
compose("", "", "")
late_status({{is_ok: true data: {{status: "accepted"}}}})
let status_bound = touch_composing && pane_name == "composer" && mod.mail_touch_compose != nil
touch_cancel()
compose("local@example.com", "Status", "Original")
send()
let offline_opened = callbacks[callbacks.len() - 1]
offline_opened({{is_ok: true data: {{compose_id: "offline-review" revision: 1}}}})
let before_poll = callbacks.len()
touch_poll_status()
let offline_status = callbacks[callbacks.len() - 1]
touch_poll_status()
let no_overlap = callbacks.len() == before_poll + 1
offline_status({{is_ok: false error: "offline"}})
let check_failed = touch_waiting && !review_on && notice == "Mail couldn't check the send status. Check your connection."
touch_poll_status()
let recovered_status = callbacks[callbacks.len() - 1]
recovered_status({{is_ok: true data: {{status: "awaiting_approval"}}}})
let check_recovered = touch_waiting && !review_on && notice == "Waiting for your approval."
touch_discard_message()
compose("local@example.com", "Status", "Original")
send()
let old_run_opened = callbacks[callbacks.len() - 1]
old_run_opened({{is_ok: true data: {{compose_id: "old-run-review" revision: 1}}}})
touch_poll_status()
let old_run_status = callbacks[callbacks.len() - 1]
let old_owner = touch_run
mod.mail_touch_run = old_owner + 1
old_run_status({{is_ok: true data: {{status: "accepted"}}}})
touch_poll_status()
let old_run_ignored = touch_composing && pane_name == "composer" && touch_status_timer == nil
mod.mail_touch_run = old_owner
touch_discard_message()
compose("local@example.com", "Status", "Original")
send()
let old_review = callbacks[callbacks.len() - 1]
mod.mail_touch_run = touch_run + 1
old_review({{is_ok: true data: {{compose_id: "review-during-switch" revision: 1}}}})
body = "Typed during switch"
touch_remember()
let transition_merged = mod.mail_touch_compose.body == "Typed during switch" &&
    !mod.mail_touch_compose.preparing && mod.mail_touch_compose.waiting &&
    mod.mail_touch_compose.compose_id == "review-during-switch"
touch_discard_message()
compose("local@example.com", "Existing", "Original")
body = "Keep this message"
touch_remember()
let prior_generation = compose_generation
touch_composing = false
let made_another = compose("other@example.com", "New", "")
let existing_preserved = !made_another && body == "Keep this message" && to == "local@example.com"
touch_discard_message()
compose_generation = 0
mod.mail_touch_run = touch_run + 1
compose("new@example.com", "New", "")
let generation_unique = compose_generation > prior_generation && mod.mail_touch_compose.generation == compose_generation
touch_discard_message()
compose("local@example.com", "Early", "")
body = "Early interaction"
touch_remember()
let current_generation = compose_generation
account = nil
touch_composing = false
compose_generation = 0
touch_cancel()
let early_control = account.id == "local-account" && touch_ask && asking && body == "Early interaction"
touch_keep_editing()
compose_generation = current_generation - 1
touch_composing = true
touch_cancel()
let stale_control_ignored = !mod.mail_touch_compose.asking
compose_generation = current_generation
touch_discard_message()
folder = {{id: "Archive" name: "Archive"}}
open_id = "original-message"
compose("local@example.com", "Check Sent", "Keep this message")
reply_message = "original-message"
touch_remember()
send()
let sent_opened = callbacks[callbacks.len() - 1]
sent_opened({{is_ok: true data: {{compose_id: "uncertain-review" revision: 1}}}})
touch_status_result("outcome_unknown")
touch_check_sent()
let failed_folders = callbacks[callbacks.len() - 1]
let old_timeout = timeouts[timeouts.len() - 1]
let before_duplicate_sent = callbacks.len()
send()
let sent_checking = touch_checking_sent && sent_off && !sent_on && !review_on && callbacks.len() == before_duplicate_sent
failed_folders({{is_ok: false error: "offline"}})
let sent_failure = !touch_checking_sent && sent_on && review_on &&
    notice == "Mail can't tell whether this was sent. Couldn't open Sent; check your connection and try again."
touch_check_sent()
let folders = callbacks[callbacks.len() - 1]
old_timeout()
let sent_timer_isolated = touch_checking_sent && sent_off && !sent_on
folders({{is_ok: true data: [{{id: "Sent" name: "Sent" role: "sent"}}]}})
let sent_browse = mod.mail_touch_compose.browsing && folder.id == "Sent" && pane_name == "inbox" &&
    mod.mail_touch_compose.body == "Keep this message"
touch_return_message()
let sent_return = pane_name == "composer" && folder.id == "Archive" && reply_message == "original-message" &&
    open_id == "original-message" && body == "Keep this message" &&
    notice == "Mail can't tell whether this was sent. Check Sent before you try again."
touch_check_sent()
let expired = timeouts[timeouts.len() - 1]
expired()
let sent_timeout = !touch_checking_sent && sent_on && review_on &&
    notice == "Mail can't tell whether this was sent. Couldn't open Sent; check your connection and try again."
touch_check_sent()
let late_folders = callbacks[callbacks.len() - 1]
touch_discard_message()
late_folders({{is_ok: true data: [{{id: "Sent" name: "Sent" role: "sent"}}]}})
let sent_stale = mod.mail_touch_compose == nil && pane_name == "inbox"
compose("local@example.com", "Reply", "Original words")
reply_message = "moved-message"
touch_remember()
send()
let moved_source = callbacks[callbacks.len() - 1]
moved_source({{is_ok: false error: "There is no such reply source"}})
let moved_explained = !preparing_review && review_on && reply_message == "" && body == "Original words" &&
    notice == "The original message is no longer available. Review this as a new message."
send()
let new_review = requests[requests.len() - 1].args
let moved_retry = new_review.message == "" && new_review.folder == "Archive" && new_review.body == "Original words"
let internal_error = callbacks[callbacks.len() - 1]
internal_error({{is_ok: false error: "internal backend detail"}})
let ordinary_error = review_on && notice == "Mail couldn't prepare this message. Try again."
send()
let invalid_address = callbacks[callbacks.len() - 1]
invalid_address({{is_ok: false error: "Enter a recipient email address."}})
let actionable_error = notice == "Enter a recipient email address."
touch_discard_message()
// The inbox may be pressed while account discovery after a restyle is still
// in flight. Preserve a new composer for the last host-confirmed account.
account = nil
mod.mail_touch_account = {{id: "local-account" address: "local@example.com"}}
let early_open = compose("recipient@example.com", "Early message", "Unsent text") &&
    mod.mail_touch_compose.account == "local-account" && mod.mail_touch_compose.body == "Unsent text"
touch_discard_message()
account = nil
mod.mail_touch_account = nil
let removed_not_restored = !compose("recipient@example.com", "Early message", "Unsent text") && mod.mail_touch_compose == nil
let checks = 0
if pending {{ checks += 1 }}
if restored {{ checks += 2 }}
if reconciled {{ checks += 4 }}
if asked {{ checks += 8 }}
if kept {{ checks += 16 }}
if discarded {{ checks += 32 }}
if clean_closed {{ checks += 64 }}
if account_bound {{ checks += 128 }}
if reply_restored && untouched_reply_closed {{ checks += 256 }}
if ordinary_note {{ checks += 512 }}
if waiting_blocks_retry {{ checks += 1024 }}
if unknown {{ checks += 2048 }}
if newer_kept {{ checks += 4096 }}
if accepted {{ checks += 8192 }}
if cancelled {{ checks += 16384 }}
if failed {{ checks += 32768 }}
if status_bound {{ checks += 65536 }}
if no_overlap && check_failed {{ checks += 131072 }}
if check_recovered {{ checks += 262144 }}
if old_run_ignored {{ checks += 524288 }}
if transition_merged {{ checks += 1048576 }}
if existing_preserved {{ checks += 2097152 }}
if generation_unique {{ checks += 4194304 }}
if early_control && stale_control_ignored {{ checks += 8388608 }}
if sent_checking && sent_failure && sent_timer_isolated && sent_browse && sent_return && sent_timeout && sent_stale {{ checks += 16777216 }}
if moved_explained && moved_retry && ordinary_error && actionable_error {{ checks += 33554432 }}
if early_open && removed_not_restored {{ checks += 67108864 }}
checks
;"#, function("compose"), function("touch_stop_review_watch"), function("touch_current"),
                function("touch_claim_run"), function("touch_next_generation"), function("touch_adopt"), function("touch_sync_review"),
                function("touch_stop_status_watch"), function("touch_watch_status"), function("touch_poll_status"), function("touch_status_result"), function("touch_follow_review"),
                function("touch_watch_review"), function("touch_remember"), function("touch_restore"),
                function("touch_check_sent"), function("touch_finish_sent"), function("touch_return_message"),
                function("touch_cancel"), function("touch_discard_words"), function("touch_keep_editing"), function("touch_discard_message"),
                function("send"), function("reply"), function("plain_send_failure")),
            ..Default::default()
        });
        let errors = vm.take_errors();
        assert!(errors.is_empty(), "Mail touch errors: {errors:?}");
        assert_eq!(value.as_number(), Some(134217727.0));
    });
}

#[test]
fn mail_touch_restores_account_loading_after_an_inbox_appearance_change() {
    let source = include_str!("../../../apps/mail/bundle/main.splash");
    let function = |name: &str| {
        let start = source.find(&format!("\nfn {name}(")).expect("Mail function") + 1;
        let end = source[start..].find("\n}\n").expect("Mail function end") + 2;
        &source[start..start + end]
    };
    let mut cx = Cx::new(Box::new(|_, _| {}));
    cx.with_vm(|vm| {
        makepad_widgets::script_mod(vm);
        vm.bx.captured_errors = Some(Vec::new());
        let value = vm.eval(ScriptMod {
            file: "mail_touch_inbox_restyle_test.splash".into(),
            code: format!(r#"
let ui_desk = false
let touch_run = 2
mod.mail_touch_run = 1
let account = nil
let restored = false
let kept_window = nil
let address = ""
let pane_name = ""
let loaded = 0
let callbacks = []
let methods = []
let ui = {{address: {{set_text: fn(value) {{ address = value }} }} }}
let host = {{request: fn(method, args, callback) {{ methods.push(method) callbacks.push(callback) }} }}
fn desk_recall() {{ false }}
fn desk_kept() {{ nil }}
fn desk_reopen() {{}}
fn desk_refresh() {{}}
fn pane(name) {{ pane_name = name }}
fn touch_restore() {{ false }}
fn touch_restore_remove() {{ false }}
fn list() {{ loaded = loaded + 1 }}
fn sync() {{ loaded = loaded + 1 }}
fn load_folders() {{ loaded = loaded + 1 }}
{}
{}
{}
// The reapply restored run 1's mod snapshot after run 2 evaluated.
touch_claim_boot()
load_accounts()
let requested = methods.len() == 1 && methods[0] == "mail.accounts" && touch_current()
callbacks[0]({{is_ok: true data: [{{id: "account-one" address: "local@example.com"}}]}})
let opened = account.id == "account-one" && address == "local@example.com" && pane_name == "inbox" && loaded == 3
// A delayed timer from run 1 must not reclaim run 2 or start a request.
touch_run = 1
touch_claim_boot()
load_accounts()
let old_ignored = mod.mail_touch_run == 2 && methods.len() == 1
// Nor may a later run reclaim an even newer one.
touch_run = 3
mod.mail_touch_run = 4
touch_claim_boot()
load_accounts()
requested && opened && old_ignored && mod.mail_touch_run == 4 && methods.len() == 1
;"#, function("touch_current"), function("touch_claim_boot"), function("load_accounts")),
            ..Default::default()
        });
        let errors = vm.take_errors();
        assert!(errors.is_empty(), "Mail inbox restyle errors: {errors:?}");
        assert_eq!(value.as_bool(), Some(true));
    });
}

#[test]
fn mail_touch_sign_in_finishes_after_an_appearance_change() {
    let source = include_str!("../../../apps/mail/bundle/main.splash");
    let function = |name: &str| {
        let start = source.find(&format!("\nfn {name}(")).expect("Mail function") + 1;
        let end = source[start..].find("\n}\n").expect("Mail function end") + 2;
        &source[start..start + end]
    };
    let mut cx = Cx::new(Box::new(|_, _| {}));
    cx.with_vm(|vm| {
        makepad_widgets::script_mod(vm);
        vm.bx.captured_errors = Some(Vec::new());
        let value = vm.eval(ScriptMod {
            file: "mail_touch_sign_in_test.splash".into(),
            code: format!(r#"
let ui_desk = false
let touch_run = 1
mod.mail_touch_run = 1
let account = nil
let restored = false
let kept_window = nil
let address = ""
let pane_name = ""
let loaded = 0
let callbacks = []
let methods = []
let ui = {{
    welcome_note: {{set_text: fn(value) {{}} }}
    address: {{set_text: fn(value) {{ address = value }} }}
}}
let host = {{request: fn(method, args, callback) {{ methods.push(method) callbacks.push(callback) }} }}
fn touch_stop_review_watch() {{}}
fn touch_stop_status_watch() {{}}
fn desk_recall() {{ false }}
fn desk_kept() {{ nil }}
fn desk_reopen() {{}}
fn desk_refresh() {{}}
fn desk_sign_in(args) {{}}
fn status(value) {{}}
fn pane(name) {{ pane_name = name }}
fn touch_restore() {{ false }}
fn touch_restore_remove() {{ false }}
fn touch_remove_blocking() {{ false }}
fn list() {{ loaded = loaded + 1 }}
fn sync() {{ loaded = loaded + 1 }}
fn load_folders() {{ loaded = loaded + 1 }}
{}
{}
{}
{}
add_account()
// The retained sign-in handler completes after another script run takes over.
mod.mail_touch_run = 2
callbacks[0]({{is_ok: true}})
let accounts_requested = methods.len() == 2 && methods[1] == "mail.accounts" && touch_current()
callbacks[1]({{is_ok: true data: [{{id: "account-one" address: "local@example.com"}}]}})
let opened = account.id == "account-one" && address == "local@example.com" && pane_name == "inbox" && loaded == 3
// An older ordinary account-list response must not replace the new view.
load_accounts()
mod.mail_touch_run = touch_run + 1
callbacks[2]({{is_ok: true data: [{{id: "stale-account" address: "old@example.com"}}]}})
let stale_ignored = account.id == "account-one" && loaded == 3
mod.mail_touch_run = touch_run
load_accounts()
mod.mail_touch_account_generation = 1
account = nil
callbacks[3]({{is_ok: true data: [{{id: "account-one" address: "local@example.com"}}]}})
let removed_stays_removed = account == nil && loaded == 3
accounts_requested && opened && stale_ignored && removed_stays_removed
;"#, function("touch_current"), function("touch_claim_run"), function("load_accounts"), function("add_account")),
            ..Default::default()
        });
        let errors = vm.take_errors();
        assert!(errors.is_empty(), "Mail sign-in errors: {errors:?}");
        assert_eq!(value.as_bool(), Some(true));
    });
}

#[test]
fn mail_touch_account_removal_requires_confirmation_and_keeps_failures_visible() {
    let source = include_str!("../../../apps/mail/bundle/main.splash");
    let function = |name: &str| {
        let start = source.find(&format!("\nfn {name}(")).expect("Mail function") + 1;
        let end = source[start..].find("\n}\n").expect("Mail function end") + 2;
        &source[start..start + end]
    };
    let mut cx = Cx::new(Box::new(|_, _| {}));
    cx.with_vm(|vm| {
        makepad_widgets::script_mod(vm);
        vm.bx.captured_errors = Some(Vec::new());
        let value = vm.eval(ScriptMod {
            file: "mail_touch_remove_account_test.splash".into(),
            code: format!(r#"
let ui_desk = false
let touch_run = 1
mod.mail_touch_run = 1
let account = {{id: "account-one" address: "local@example.com"}}
mod.mail_touch_account = account
let messages = []
let folders = []
let folder = {{id: "INBOX" name: "Inbox"}}
let open_id = ""
let touch_composing = false
let touch_waiting = false
let touch_checking_sent = false
let touch_offer_sent = false
let overlay = false
let confirm_on = false
let confirm_off = false
let keep_on = false
let keep_off = false
let error = ""
let progress = ""
let pane_name = ""
let callbacks = []
let timeouts = []
let reading_forgotten = 0
let ui = {{
    touch_remove_overlay: {{set_visible: fn(value) {{ overlay = value }} }}
    touch_remove_error: {{set_text: fn(value) {{ error = value }} set_visible: fn(value) {{}} }}
    touch_remove_progress: {{set_text: fn(value) {{ progress = value }} set_visible: fn(value) {{}} }}
    touch_remove_confirm: {{set_visible: fn(value) {{ confirm_on = value }} }}
    touch_remove_confirm_off: {{set_visible: fn(value) {{ confirm_off = value }} }}
    touch_remove_keep: {{set_visible: fn(value) {{ keep_on = value }} }}
    touch_remove_keep_off: {{set_visible: fn(value) {{ keep_off = value }} }}
    address: {{set_text: fn(value) {{}} }}
    title: {{set_text: fn(value) {{}} }}
    welcome_note: {{set_text: fn(value) {{}} }}
}}
let host = {{request: fn(method, args, callback) {{ callbacks.push(callback) }} }}
fn pane(name) {{ pane_name = name }}
fn touch_stop_review_watch() {{}}
fn touch_stop_status_watch() {{}}
fn touch_forget_reading() {{ reading_forgotten = reading_forgotten + 1 }}
fn touch_next_folder_generation() {{}}
fn start_timeout(seconds, callback) {{ timeouts.push(callback) 10 }}
{}
{}
{}
{}
{}
{}
{}
{}
{}
{}
{}
{}
{}
touch_ask_remove()
let asks_first = overlay && confirm_on && keep_on && callbacks.len() == 0 && reading_forgotten == 1
touch_keep_account()
let shielded = overlay && mod.mail_touch_remove.closing && touch_remove_blocking() && !touch_restore_remove()
let first_hide = timeouts[timeouts.len() - 1]
first_hide()
let kept = shielded && !overlay && mod.mail_touch_remove == nil && callbacks.len() == 0
touch_ask_remove()
first_hide()
let stale_hide_ignored = overlay && mod.mail_touch_remove != nil
let generation = mod.mail_touch_remove.generation
account = nil
mod.mail_touch_run = touch_run + 1
let restored = touch_early_restore_remove(generation) && account.id == "account-one" && overlay && touch_current()
touch_confirm_remove()
let failed = callbacks[0]
let slow = timeouts[timeouts.len() - 1]
touch_confirm_remove()
touch_keep_account()
let pending = overlay && confirm_off && keep_off && !confirm_on && !keep_on && callbacks.len() == 1
slow()
let slow_explained = progress == "Removal is taking longer than expected. It may still finish."
mod.mail_touch_run = touch_run + 1
failed({{is_ok: false error: "fixture storage failure"}})
let failure_kept = overlay && confirm_on && keep_on && account.id == "account-one" &&
    error == "Mail couldn't finish removing this account. Try again."
touch_keep_account()
let failure_hide = timeouts[timeouts.len() - 1]
failure_hide()
touch_ask_remove()
touch_confirm_remove()
let success = callbacks[1]
failed({{is_ok: true}})
slow()
let stale_ignored = mod.mail_touch_remove.pending && account.id == "account-one" && overlay
success({{is_ok: true}})
let success_shielded = overlay && pane_name == "welcome" && mod.mail_touch_remove.closing && touch_remove_blocking()
let final_hide = timeouts[timeouts.len() - 1]
final_hide()
let removed = success_shielded && account == nil && mod.mail_touch_remove == nil && !overlay && mod.mail_touch_account_generation == 1 && mod.mail_folders_request == 1
asks_first && kept && stale_hide_ignored && restored && pending && slow_explained && failure_kept && stale_ignored && removed
;"#, function("touch_current"), function("touch_claim_run"), function("touch_remove_blocking"), function("touch_restore_remove"), function("touch_ask_remove"),
                function("touch_keep_account"), function("touch_hide_remove"), function("touch_confirm_remove"), function("touch_remove_slow"),
                function("touch_remove_finished"), function("remove_account"), function("touch_early_restore_remove"), function("next_folders_request")),
            ..Default::default()
        });
        let errors = vm.take_errors();
        assert!(errors.is_empty(), "Mail account-removal errors: {errors:?}");
        assert_eq!(value.as_bool(), Some(true));
    });
}

#[test]
fn mail_touch_reply_uses_the_open_message_and_ignores_late_reads() {
    let source = include_str!("../../../apps/mail/bundle/main.splash");
    let function = |name: &str| {
        let start = source.find(&format!("\nfn {name}(")).expect("Mail function") + 1;
        let end = source[start..].find("\n}\n").expect("Mail function end") + 2;
        &source[start..start + end]
    };
    let mut cx = Cx::new(Box::new(|_, _| {}));
    cx.with_vm(|vm| {
        makepad_widgets::script_mod(vm);
        vm.bx.captured_errors = Some(Vec::new());
        let value = vm.eval(ScriptMod {
            file: "mail_touch_reply_test.splash".into(),
            code: format!(r#"
let ui_desk = false
let account = {{id: "account-one" address: "local@example.com"}}
let folder = {{id: "INBOX" name: "Inbox"}}
let open_id = ""
let reply_message = ""
let reading = nil
let title = ""
let pane_name = ""
let reply_to = ""
let reply_subject = ""
let reply_body = ""
let callbacks = []
let field = {{set_text: fn(value) {{}} set_visible: fn(value) {{}} }}
let ui = {{
    r_subject: {{set_text: fn(value) {{ title = value }} }}
    r_sender: field r_time: field r_html: field r_body: field r_attachments: field
}}
let host = {{request: fn(method, args, callback) {{ callbacks.push(callback) }} }}
fn pane(name) {{ pane_name = name }}
fn status(value) {{}}
fn kb(size) {{ "" }}
fn compose(to, subject, body) {{ reply_to = to reply_subject = subject reply_body = body true }}
fn touch_remember() {{}}
{}
{}
{}
{}
open_message({{id: "first" unread: true}})
open_message({{id: "second" unread: true}})
let first = {{subject: "First" sender: "A" address: "a@example.com" time: "" html: "" body: "A" attachments: []}}
let second = {{subject: "Second" sender: "B" address: "b@example.com" time: "" html: "" body: "B" attachments: []}}
callbacks[1]({{is_ok: true data: second}})
callbacks[0]({{is_ok: true data: first}})
let newest_wins = title == "Second" && mod.mail_touch_reading.id == "second"
touch_forget_reading()
pane_name = "inbox"
callbacks[1]({{is_ok: true data: second}})
let closed_stays_closed = pane_name == "inbox" && mod.mail_touch_reading == nil
open_message({{id: "third" unread: true}})
callbacks[2]({{is_ok: true data: second}})
// Simulate a retained Reply control from a run without its local reader state.
reading = nil
account = nil
open_id = ""
reply()
let right_reply = account.id == "account-one" && folder.id == "INBOX" && reply_message == "third" &&
    reply_to == "b@example.com" && reply_subject == "Re: Second" && reply_body == "\n\n> B"
account = {{id: "other-account" address: "other@example.com"}}
reply_to = ""
reply()
let other_account_ignored = reply_to == ""
account = {{id: "account-one" address: "local@example.com"}}
touch_forget_reading()
pane_name = "inbox"
open_message({{id: "pending" unread: true}})
mod.mail_touch_remove = {{generation: 1}}
let before_blocked_open = callbacks.len()
open_message({{id: "blocked" unread: true}})
let click_blocked = callbacks.len() == before_blocked_open
callbacks[3]({{is_ok: true data: first}})
let late_read_blocked = pane_name == "inbox" && mod.mail_touch_reading == nil
mod.mail_touch_remove = nil
open_message({{id: "pending-before-sheet" unread: true}})
touch_forget_reading()
mod.mail_touch_remove = {{generation: 2}}
mod.mail_touch_remove = nil
callbacks[4]({{is_ok: true data: first}})
let late_after_close_blocked = pane_name == "inbox" && mod.mail_touch_reading == nil
open_message({{id: "after-sheet" unread: true}})
callbacks[5]({{is_ok: true data: second}})
let later_read_opens = pane_name == "reader" && mod.mail_touch_reading.id == "after-sheet"
newest_wins && closed_stays_closed && right_reply && other_account_ignored && click_blocked && late_read_blocked && late_after_close_blocked && later_read_opens
;"#, function("touch_remove_blocking"), function("touch_forget_reading"), function("open_message"), function("reply")),
            ..Default::default()
        });
        let errors = vm.take_errors();
        assert!(errors.is_empty(), "Mail reader errors: {errors:?}");
        assert_eq!(value.as_bool(), Some(true));
    });
}

#[test]
fn mail_desktop_explains_a_moved_reply_before_reviewing_as_new() {
    let source = include_str!("../../../apps/mail/bundle/main.splash");
    let function = |name: &str| {
        let start = source.find(&format!("\nfn {name}(")).expect("Mail function") + 1;
        let end = source[start..].find("\n}\n").expect("function end") + 2;
        &source[start..start + end]
    };
    let mut cx = Cx::new(Box::new(|_, _| {}));
    cx.with_vm(|vm| {
        makepad_widgets::script_mod(vm);
        vm.bx.captured_errors = Some(Vec::new());
        let value = vm.eval(ScriptMod {
            file: "mail_desktop_moved_reply_test.splash".into(),
            code: format!(r#"
use mod.std.regex
let account = {{id: "account"}}
let composing = true
let preparing_review = false
let compose_retrying = ""
let compose_generation = 7
let compose_id = ""
let compose_revision = 0
let compose_state = ""
let compose_folder = "INBOX"
let reply_message = "old-message"
let compose_ask = false
let compose_sent_to = ""
let compose_sent_cc = ""
let compose_sent_subject = ""
let compose_sent_body = ""
let desk_review_serial = 0
let desk_review_timer = nil
let desk_review_run = 1
mod.mail_desk_review_run = 1
let note = ""
let bad = false
let updates = 0
let requests = []
let callbacks = []
let ui = {{c_to: {{text: fn() {{ "reader@example.com" }} }}
    c_cc: {{text: fn() {{ "copy@example.com" }} }}
    c_subject: {{text: fn() {{ "Re: Hello" }} }} }}
let host = {{request: fn(method, args, callback) {{ requests.push(args) callbacks.push(callback) }} }}
fn compose_changed() {{ updates = updates + 1 }}
fn compose_note(text, error) {{ note = text bad = error }}
fn compose_body() {{ "The words and quote" }}
fn compose_typed() {{ "The typed words" }}
fn compose_watch() {{}}
fn compose_discard_words() {{}}
fn plain_send_failure(error) {{ "ordinary failure" }}
fn start_interval(seconds, callback) {{ 1 }}
fn stop_timer(timer) {{}}
{}
{}
{}
{}
{}
{}
compose_send()
// The answer can be written by the old run and consumed by a new one.
desk_review_run = 2
mod.mail_desk_review_run = 2
callbacks[0]({{is_ok: false error: "There is no such reply source"}})
desk_review_follow()
let explained = requests.len() == 1 && !preparing_review && reply_message == "" && bad &&
    note == "The original message is no longer available. Review this as a new message."
compose_send()
let retry = requests.len() == 2 && requests[1].message == "" && requests[1].folder == "INBOX" &&
    requests[1].to == "reader@example.com" && requests[1].cc == "copy@example.com" &&
    requests[1].subject == "Re: Hello" && requests[1].body == "The words and quote"
callbacks[1]({{is_ok: false error: "other failure"}})
desk_review_follow()
let ordinary_error = !preparing_review && note == "ordinary failure" && bad && updates >= 3
reply_message = "another-source"
compose_send()
callbacks[0]({{is_ok: false error: "There is no such reply source"}})
let old_reply_ignored = mod.mail_desk_review.pending && mod.mail_desk_review.serial == desk_review_serial
// Closing the message invalidates the handoff, even if the reply arrives later.
mod.mail_desk_review = nil
composing = false
callbacks[2]({{is_ok: true data: {{compose_id: "late" revision: 1}}}})
let closed_stays_closed = mod.mail_desk_review == nil && !composing
explained && retry && ordinary_error && old_reply_ignored && closed_stays_closed
;"#, function("desk_review_current"), function("desk_review_stop"), function("desk_review_watch"), function("desk_review_follow"),
        function("compose_send"), function("compose_review")),
            ..Default::default()
        });
        let errors = vm.take_errors();
        assert!(errors.is_empty(), "Mail desktop moved-reply errors: {errors:?}");
        assert_eq!(value.as_bool(), Some(true));
    });
}

#[test]
fn mail_desktop_keeps_edits_made_after_a_review_was_requested() {
    let source = include_str!("../../../apps/mail/bundle/main.splash");
    let start = source.find("\nfn compose_heard(").expect("Mail status function") + 1;
    let end = source[start..].find("\n}\n").expect("function end") + 2;
    let mut cx = Cx::new(Box::new(|_, _| {}));
    cx.with_vm(|vm| {
        makepad_widgets::script_mod(vm);
        vm.bx.captured_errors = Some(Vec::new());
        let value = vm.eval(ScriptMod {
            file: "mail_desktop_accepted_edits_test.splash".into(),
            code: format!(r#"
let to = "reader@example.com"
let cc = "copy@example.com"
let subject = "A subject"
let body = "The reviewed body"
let compose_sent_to = to
let compose_sent_cc = cc
let compose_sent_subject = subject
let compose_sent_body = body
let compose_state = "awaiting_approval"
let compose_id = "sent-id"
let compose_revision = 1
let compose_retrying = ""
let preparing_review = true
let compose_ask = false
let closed = 0
let unwatched = 0
let note = ""
let ui = {{c_to: {{text: fn() {{ to }} }} c_cc: {{text: fn() {{ cc }} }}
    c_subject: {{text: fn() {{ subject }} }}
    status: {{set_text: fn(value) {{}} set_visible: fn(value) {{}} }} }}
fn compose_typed() {{ body }}
fn compose_close() {{ closed = closed + 1 }}
fn compose_unwatch() {{ unwatched = unwatched + 1 }}
fn compose_changed() {{}}
fn compose_note(text, bad) {{ note = text }}
fn compose_discard_words() {{}}
fn desk_bar() {{}}
fn start_timeout(seconds, callback) {{}}
{}
compose_heard("accepted")
let unchanged_closes = closed == 1
compose_state = "awaiting_approval"
compose_id = "sent-id"
compose_revision = 1
preparing_review = true
body = "A later edit"
compose_heard("accepted")
let body_kept = closed == 1 && unwatched == 1 && !preparing_review && compose_id == "" && compose_revision == 0 &&
    note == "Message sent. Your newer changes weren't sent." && body == "A later edit"
body = compose_sent_body
to = "other@example.com"
compose_state = "awaiting_approval"
compose_heard("accepted")
let recipient_kept = closed == 1
to = compose_sent_to
cc = "other-copy@example.com"
compose_state = "awaiting_approval"
compose_heard("accepted")
let copy_kept = closed == 1
cc = compose_sent_cc
subject = "A different subject"
compose_state = "awaiting_approval"
compose_heard("accepted")
let subject_kept = closed == 1
unchanged_closes && body_kept && recipient_kept && copy_kept && subject_kept
;"#, &source[start..start + end]),
            ..Default::default()
        });
        let errors = vm.take_errors();
        assert!(errors.is_empty(), "Mail accepted-edit errors: {errors:?}");
        assert_eq!(value.as_bool(), Some(true));
    });
}

#[test]
fn mail_folder_lists_do_not_return_from_an_old_account_or_request() {
    let source = include_str!("../../../apps/mail/bundle/main.splash");
    let function = |name: &str| {
        let start = source.find(&format!("\nfn {name}(")).expect("Mail function") + 1;
        let end = source[start..].find("\n}\n").expect("function end") + 2;
        &source[start..start + end]
    };
    let mut cx = Cx::new(Box::new(|_, _| {}));
    cx.with_vm(|vm| {
        makepad_widgets::script_mod(vm);
        vm.bx.captured_errors = Some(Vec::new());
        let value = vm.eval(ScriptMod {
            file: "mail_folder_list_order_test.splash".into(),
            code: format!(r#"
let ui_desk = true
let account = {{id: "one"}}
let folder = {{id: "INBOX"}}
let folders = []
let folder_kept = false
let callbacks = []
let renders = 0
let ui = {{folder_rows: {{render: fn() {{ renders = renders + 1 }} }} }}
let host = {{request: fn(method, args, callback) {{ callbacks.push(callback) }} }}
fn desk_folders() {{ renders = renders + 1 }}
fn desk_rows() {{}}
fn desk_remember() {{}}
fn count_unread() {{}}
fn open_folder(value) {{ folder = value }}
{}
{}
load_folders()
account = {{id: "two"}}
load_folders()
callbacks[1]({{is_ok: true data: [{{id: "new-account"}}]}})
callbacks[0]({{is_ok: true data: [{{id: "old-account"}}]}})
let current_account = folders[0].id == "new-account" && renders == 1
load_folders()
load_folders()
callbacks[3]({{is_ok: true data: [{{id: "newest"}}]}})
callbacks[2]({{is_ok: true data: [{{id: "stale"}}]}})
let newest_request = folders[0].id == "newest" && renders == 2
ui_desk = false
load_folders()
account = nil
callbacks[4]({{is_ok: true data: [{{id: "removed-account"}}]}})
let removed_account = folders[0].id == "newest" && renders == 2
current_account && newest_request && removed_account
;"#, function("next_folders_request"), function("load_folders")),
            ..Default::default()
        });
        let errors = vm.take_errors();
        assert!(errors.is_empty(), "Mail folder-list errors: {errors:?}");
        assert_eq!(value.as_bool(), Some(true));
    });
}

#[test]
fn mail_desktop_mailbox_requests_follow_the_latest_account_and_folder() {
    let source = include_str!("../../../apps/mail/bundle/main.splash");
    let function = |name: &str| {
        let start = source.find(&format!("\nfn {name}(")).expect("Mail function") + 1;
        let end = source[start..].find("\n}\n").expect("Mail function end") + 2;
        &source[start..start + end]
    };
    let mut cx = Cx::new(Box::new(|_, _| {}));
    cx.with_vm(|vm| {
        makepad_widgets::script_mod(vm);
        vm.bx.captured_errors = Some(Vec::new());
        let value = vm.eval(ScriptMod {
            file: "mail_desktop_mailbox_navigation_test.splash".into(),
            code: format!(r#"
let ui_desk = true
let account = {{id: "one"}}
let folder = {{id: "INBOX"}}
let messages = []
let total = 0
let busy = false
let loaded = false
let failure = ""
let then_open = ""
let renders = 0
let callbacks = []
let methods = []
let host = {{request: fn(method, args, callback) {{ methods.push(method) callbacks.push(callback) }} }}
fn desk_list() {{ renders = renders + 1 }}
fn desk_bar() {{}}
fn desk_states() {{}}
fn plain_failure(error) {{ error }}
fn open_message(message) {{}}
{}
{}
{}
{}
list()
folder = {{id: "Archive"}}
list()
callbacks[1]({{is_ok: true data: {{folder: "Archive" messages: [] total: 0}}}})
callbacks[0]({{is_ok: false error: "old inbox failure"}})
let old_failure_ignored = failure == "" && renders == 1
folder = {{id: "INBOX"}}
list()
folder = {{id: "Archive"}}
list()
folder = {{id: "INBOX"}}
list()
callbacks[4]({{is_ok: true data: {{folder: "INBOX" messages: [{{subject: "new"}}] total: 1}}}})
callbacks[2]({{is_ok: true data: {{folder: "INBOX" messages: [{{subject: "old"}}] total: 1}}}})
callbacks[3]({{is_ok: false error: "old archive failure"}})
let return_to_folder = messages[0].subject == "new" && failure == ""
list()
list()
callbacks[6]({{is_ok: true data: {{folder: "INBOX" messages: [{{subject: "latest"}}] total: 1}}}})
callbacks[5]({{is_ok: true data: {{folder: "INBOX" messages: [{{subject: "older"}}] total: 1}}}})
let newest_list_wins = messages[0].subject == "latest"
list()
account = {{id: "two"}}
callbacks[7]({{is_ok: false error: "old account failure"}})
let old_account_ignored = failure == ""
account = {{id: "one"}}
sync()
desk_next_sync_request()
busy = false
folder = {{id: "Archive"}}
sync()
callbacks[8]({{is_ok: false error: "old sync failure"}})
let old_sync_ignored = busy && failure == ""
callbacks[9]({{is_ok: true data: {{new: 0}}}})
let new_sync_completes = !busy && methods[10] == "mail.list"
old_failure_ignored && return_to_folder && newest_list_wins && old_account_ignored && old_sync_ignored && new_sync_completes
;"#, function("desk_next_list_request"), function("desk_next_sync_request"), function("list"), function("sync")),
            ..Default::default()
        });
        let errors = vm.take_errors();
        assert!(errors.is_empty(), "Mail desktop mailbox navigation errors: {errors:?}");
        assert_eq!(value.as_bool(), Some(true));
    });
}

#[test]
fn mail_touch_mailbox_navigation_ignores_late_results() {
    let source = include_str!("../../../apps/mail/bundle/main.splash");
    let function = |name: &str| {
        let start = source.find(&format!("\nfn {name}(")).expect("Mail function") + 1;
        let end = source[start..].find("\n}\n").expect("Mail function end") + 2;
        &source[start..start + end]
    };
    let mut cx = Cx::new(Box::new(|_, _| {}));
    cx.with_vm(|vm| {
        makepad_widgets::script_mod(vm);
        vm.bx.captured_errors = Some(Vec::new());
        let value = vm.eval(ScriptMod {
            file: "mail_touch_mailbox_navigation_test.splash".into(),
            code: format!(r#"
let ui_desk = false
let account = {{id: "account-one"}}
let folder = {{id: "Sent"}}
let messages = []
let total = 0
let busy = false
let renders = 0
let callbacks = []
let methods = []
let ui = {{rows: {{render: fn() {{ renders = renders + 1 }} }} }}
let host = {{request: fn(method, args, callback) {{ methods.push(method) callbacks.push(callback) }} }}
fn status(value) {{}}
fn touch_remove_blocking() {{ false }}
{}
{}
mod.mail_touch_folder_generation = 1
list()
let old_sent = callbacks[0]
folder = {{id: "INBOX"}}
mod.mail_touch_folder_generation = 2
list()
let inbox = callbacks[1]
inbox({{is_ok: true data: {{folder: "INBOX" messages: [{{subject: "Inbox message"}}] total: 1}}}})
old_sent({{is_ok: true data: {{folder: "Sent" messages: [{{subject: "Old Sent message"}}] total: 1}}}})
let list_bound = renders == 1 && messages[0].subject == "Inbox message"
folder = {{id: "Sent"}}
mod.mail_touch_folder_generation = 3
sync()
let old_sync = callbacks[2]
folder = {{id: "INBOX"}}
mod.mail_touch_folder_generation = 4
old_sync({{is_ok: true data: {{new: 1}}}})
let sync_bound = methods.len() == 3 && !busy
list_bound && sync_bound
;"#, function("list"), function("sync")),
            ..Default::default()
        });
        let errors = vm.take_errors();
        assert!(errors.is_empty(), "Mail mailbox navigation errors: {errors:?}");
        assert_eq!(value.as_bool(), Some(true));
    });
}

#[test]
fn mail_touch_restores_a_retained_composer_before_account_reload() {
    let source = include_str!("../../../apps/mail/bundle/main.splash");
    let function = |name: &str| {
        let start = source.find(&format!("\nfn {name}(")).expect("Mail function") + 1;
        let end = source[start..].find("\n}\n").expect("Mail function end") + 2;
        &source[start..start + end]
    };
    let mut cx = Cx::new(Box::new(|_, _| {}));
    cx.with_vm(|vm| {
        makepad_widgets::script_mod(vm);
        vm.bx.captured_errors = Some(Vec::new());
        let value = vm.eval(ScriptMod {
            file: "mail_touch_early_restore_test.splash".into(),
            code: format!(r#"
let ui_desk = false
let touch_run = 1
mod.mail_touch_run = 1
let account = nil
let address = ""
let restores = 0
let ui = {{address: {{set_text: fn(value) {{ address = value }} }} }}
fn touch_restore() {{ restores = restores + 1 true }}
fn touch_stop_review_watch() {{}}
fn touch_stop_status_watch() {{}}
{}
{}
{}
let absent = !touch_early_restore(7) && account == nil && restores == 0
mod.mail_touch_compose = {{account: "account-one" generation: 7 account_data: {{id: "account-one" address: "local@example.com"}}}}
let restored = touch_early_restore(7) && account.id == "account-one" && address == "local@example.com" && restores == 1
account = nil
mod.mail_touch_run = 2
let reclaimed = touch_early_restore(7) && account.id == "account-one" && touch_current() && restores == 2
account = nil
mod.mail_touch_compose = {{account: "account-one" generation: 8 account_data: {{id: "account-one" address: "local@example.com"}}}}
let stale_ignored = !touch_early_restore(7) && account == nil && restores == 2
mod.mail_touch_compose = {{account: "other-account" generation: 7 account_data: {{id: "account-one" address: "local@example.com"}}}}
let mismatch_ignored = !touch_early_restore(7) && account == nil && restores == 2
absent && restored && reclaimed && stale_ignored && mismatch_ignored
;"#, function("touch_current"), function("touch_claim_run"), function("touch_early_restore")),
            ..Default::default()
        });
        let errors = vm.take_errors();
        assert!(errors.is_empty(), "Mail early restore errors: {errors:?}");
        assert_eq!(value.as_bool(), Some(true));
    });
}

#[test]
fn mail_desktop_discard_explains_send_state_and_updates_while_open() {
    let source = include_str!("../../../apps/mail/bundle/main.splash");
    let function = |name: &str| {
        let start = source.find(&format!("\nfn {name}(")).expect("Mail function") + 1;
        let end = source[start..].find("\n}\n").expect("Mail function end") + 2;
        &source[start..start + end]
    };
    let mut cx = Cx::new(Box::new(|_, _| {}));
    cx.with_vm(|vm| {
        makepad_widgets::script_mod(vm);
        vm.bx.captured_errors = Some(Vec::new());
        let value = vm.eval(ScriptMod {
            file: "mail_desktop_discard_state_test.splash".into(),
            code: format!(r#"
let compose_ask = false
let compose_state = ""
let preparing_review = false
let compose_id = ""
let compose_retrying = ""
let compose_revision = 0
let note = ""
let visible = false
let ui = {{
    compose_discard_note: {{set_text: fn(value) {{ note = value }} }}
    compose_ask: {{set_visible: fn(value) {{ visible = value }} }}
    status: {{set_text: fn(value) {{}} set_visible: fn(value) {{}} }}
}}
fn desk_remember() {{}}
fn compose_unwatch() {{}}
fn compose_changed() {{}}
fn compose_note(value, bad) {{}}
fn compose_close() {{}}
fn desk_bar() {{}}
fn start_timeout(seconds, callback) {{ 1 }}
{}
{}
{}
compose_asking(true)
let ordinary = visible && note == "Your changes won't be kept."
preparing_review = true
compose_state = "awaiting_approval"
compose_discard_words()
let pending = note == "Your changes won't be kept. This doesn't cancel a send review."
compose_heard("sending")
let sending = note == "Closing this window won't cancel a send in progress. Your changes won't be kept."
compose_heard("outcome_unknown")
let unknown = note == "Mail can't tell whether this was sent. Your changes won't be kept."
preparing_review = true
compose_discard_words()
let retry = note == "Mail can't tell whether the earlier message was sent. Closing won't cancel the new send review. Your changes won't be kept."
compose_heard("draft")
let cancelled = note == "Your changes won't be kept."
ordinary && pending && sending && unknown && retry && cancelled
;"#, function("compose_discard_words"), function("compose_asking"), function("compose_heard")),
            ..Default::default()
        });
        let errors = vm.take_errors();
        assert!(errors.is_empty(), "Mail discard errors: {errors:?}");
        assert_eq!(value.as_bool(), Some(true));
    });
}

#[test]
fn mail_window_save_failure_stays_visible_and_retry_keeps_the_current_state() {
    let source = include_str!("../../../apps/mail/bundle/main.splash");
    let function = |name: &str| {
        let start = source.find(&format!("\nfn {name}(")).expect("Mail function") + 1;
        let end = source[start..].find("\n}\n").expect("Mail function end") + 2;
        &source[start..start + end]
    };
    let mut cx = Cx::new(Box::new(|_, _| {}));
    cx.with_vm(|vm| {
        makepad_widgets::script_mod(vm);
        vm.bx.captured_errors = Some(Vec::new());
        let value = vm.eval(ScriptMod {
            file: "mail_window_save_failure_test.splash".into(),
            code: format!(r#"
use mod.std.assert
let restored = true
let account = {{id: "local-account"}}
let folder = {{id: "INBOX"}}
let composing = true
let compose_kind = "new"
let reply_message = ""
let compose_folder = "INBOX"
let compose_head = ""
let compose_quote = ""
let compose_start = ""
let compose_missing_attachments = false
let compose_said = ""
let compose_said_bad = false
let compose_state = ""
let then_open = ""
let open_id = ""
let side_wanted = true
let WINDOW_FILE = "window.json"
let keep_timer = nil
let keep_error = false
let list_warning = false
let compose_warning = false
let stopped = nil
let deny = true
let disk = nil
let ui = {{
    c_to: {{text: fn() {{ "local@example.com" }} }}
    c_cc: {{text: fn() {{ "" }} }}
    c_subject: {{text: fn() {{ "Local draft" }} }}
    keep_warning: {{set_visible: fn(value) {{ list_warning = value }} }}
    c_keep_warning: {{set_visible: fn(value) {{ compose_warning = value }} }}
}}
let fs = {{write: fn(path, data) {{
    if deny {{ assert(false) }}
    disk = data.parse_json()
}}}}
fn compose_typed() {{ "A local draft" }}
fn stop_timer(id) {{ stopped = id }}
{}
{}
{}
desk_keep_now()
let refused = keep_error && compose_warning && !list_warning && disk == nil
// Discard while storage is unavailable keeps a visible warning in the list.
composing = false
sync_keep_warning()
let warned_after_close = list_warning && !compose_warning
keep_timer = 17
deny = false
desk_retry_keep()
let closed_saved = stopped == 17 && !keep_error && !list_warning && disk["compose"] == nil
composing = true
desk_keep_now()
refused && warned_after_close && closed_saved && !compose_warning && disk["compose"]["body"] == "A local draft"
;"#, function("sync_keep_warning"), function("desk_retry_keep"), function("desk_keep_now")),
            ..Default::default()
        });
        let errors = vm.take_errors();
        assert!(errors.is_empty(), "Mail app errors: {errors:?}");
        assert_eq!(value.as_bool(), Some(true));
    });
}

#[test]
fn camera_recording_failure_settles_controls_without_stopping_a_healthy_recording() {
    let source = include_str!("../../../apps/camera/bundle/main.splash");
    let logic = source.split_once("\nstart_timeout(").expect("Camera boot boundary").0;
    let mut cx = Cx::new(Box::new(|_, _| {}));
    cx.with_vm(|vm| {
        makepad_widgets::script_mod(vm);
        vm.bx.captured_errors = Some(Vec::new());
        // Execute the shipped app's callbacks in the native VM. Camera,
        // timers and rendering endpoints are test doubles: this verifies app
        // state and endpoint calls, not pixels or successful hardware capture.
        let value = vm.eval(ScriptMod {
            file: "camera_recording_failure_test.splash".into(),
            code: format!(r#"
use mod.std.assert
let fs = {{}}
let native_recording = false
let native_error = ""
let timer_visible = false
let retry_visible = false
let toast = ""
let timers = []
let stopped = []
let stop_requests = 0
let microphone_allowed = true
let host = {{request: fn(service, args, reply){{
    assert(service == "microphone.permission.request")
    reply({{is_ok: true data: {{app_consent: microphone_allowed os_permission: "granted"}}}})
}}}}
fn time_now() {{ 123 }}
fn start_interval(seconds, callback) {{ timers.push(callback); timers.len() }}
fn stop_timer(timer) {{ stopped.push(timer) }}
let widget = {{render: fn(){{}} set_text: fn(text){{}}}}
let ui = {{
    cam: {{
        record_start: fn(options){{ assert(options.audio && !options.library); native_recording = true; true }}
        record_stop: fn(){{ stop_requests += 1; native_recording = false }}
        is_running: fn(){{ true }}
        is_recording: fn(){{ native_recording }}
        error: fn(){{ native_error }}
        set_aspect: fn(aspect){{}}
    }}
    rec: widget
    rec_box: {{set_visible: fn(value){{ timer_visible = value }}}}
    shutter_face: widget modes: widget
    toast: {{set_text: fn(value){{ toast = value }}}}
    retry_camera: {{set_visible: fn(value){{ retry_visible = value }}}}
}}
{logic}
set_mode("video")
start_recording()
assert(recording && timer_visible && rec_timer == 1)
// Android queues a request, then reports that recording is unsupported.
native_recording = false
native_error = "video failed: recording is not implemented on Android yet"
failed()
assert(!recording && !timer_visible && rec_timer == nil)
assert(stopped.len() == 1 && stopped[0] == 1)
assert(stop_requests == 0 && retry_visible)
assert(toast == native_error)
set_mode("photo")
assert(mode == "photo")
failed()
assert(stopped.len() == 1)
// An unrelated error while the native recorder remains active must not
// discard the UI state or permit a mode change during that recording.
set_mode("video")
start_recording()
native_error = "photo failed: synthetic still failure"
failed()
assert(recording && timer_visible && rec_timer == 2)
assert(stopped.len() == 1 && stop_requests == 0)
set_mode("photo")
assert(mode == "video")
stop_recording()
assert(!recording && !timer_visible && rec_timer == nil)
assert(stopped.len() == 2 && stopped[1] == 2 && stop_requests == 1)
// Rejected app consent cannot start another native recording or UI timer.
microphone_allowed = false
start_recording()
assert(!recording && !native_recording && !timer_visible && rec_timer == nil)
assert(timers.len() == 2 && retry_visible && !permission_pending)
true
;"#),
            ..Default::default()
        });
        let errors = vm.take_errors();
        assert!(errors.is_empty(), "Camera app errors: {errors:?}");
        assert!(!value.is_err(), "Camera callbacks returned {value:?}");
    });
}

/// News's styles: its `let`s, without the boot before them or the view after.
fn news_styles() -> String {
    let after_boot = NEWS.split_once("\nlet ink = ").unwrap().1;
    format!("{}\nlet ink = {}", include_str!("../../../apps/interface.splash"), after_boot.split_once("\nHostedView{").unwrap().0)
}

/// Each colour in `expressions`, evaluated after News's styles in a VM
/// themed as a hosted app is in `style` and appearance.
fn news_colors(style: DesktopStyle, dark: bool, expressions: &[&str]) -> Vec<u32> {
    script_colors(style, dark, &news_styles(), expressions)
}

fn script_colors(style: DesktopStyle, dark: bool, styles: &str, expressions: &[&str]) -> Vec<u32> {
    let mut cx = Cx::new(Box::new(|_, _| {}));
    cx.with_vm(|vm| {
        makepad_widgets::script_mod(vm);
        desktop_style::install(vm, load_sheet(style, dark));
        vm.with_reload(makepad_widgets::script_mod);
        expressions
            .iter()
            .map(|expression| {
                vm.bx.captured_errors = Some(Vec::new());
                let value = vm.eval(ScriptMod {
                    file: "news_theme_test.splash".into(),
                    code: format!("use mod.prelude.widgets.*\n{styles}\n{expression}\n;"),
                    ..Default::default()
                });
                let errors = vm.take_errors();
                assert!(errors.is_empty(), "{expression}: {errors:?}");
                Vec4f::script_from_value(vm, value).to_u32()
            })
            .collect()
    })
}

#[test]
fn youtube_committed_query_restores_field_without_overwriting_unsent_typing() {
    let source = include_str!("../../../apps/youtube/bundle/main.splash");
    let logic = source.split_once("// END shared app interface\n").unwrap().1
        .split_once("\nstart_timeout(").unwrap().0;
    for (stored, expected) in [("\"NASA\"", "NASA"), ("nil", "lofi hip hop radio")] {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(|vm| {
            makepad_widgets::script_mod(vm);
            vm.bx.captured_errors = Some(Vec::new());
            // Run the shipped callbacks with local storage/network/UI spies.
            // No provider request or actual persisted user file is involved.
            let value = vm.eval(ScriptMod {
                file: "youtube_query_restore_test.splash".into(),
                code: format!(r#"
use mod.std.assert
let stored_query = {stored}
let writes = []
let field_text = ""
let field_sets = 0
let status_text = ""
let video_count = "—"
let ui_desk = false
let list_visible = true
let empty_visible = false
let reader_url = ""
let reader_opens = 0
let player_notice = false
let history_notice_visible = false
let query_notice_visible = false
let search_retry_visible = false
let requests = []
let retries = []
let fail_history_write = false
let fail_query_write = false
let fs = {{
    exists: fn(path){{ path == "accounts/device/query.json" && stored_query != nil }}
    read: fn(path){{ assert(path == "accounts/device/query.json"); {{q: stored_query}}.to_json() }}
    write: fn(path, data){{
        if fail_history_write && path == "accounts/device/history.json" {{ assert(false) }}
        if fail_query_write && path == "accounts/device/query.json" {{ assert(false) }}
        writes.push({{path: path data: data}})
    }}
    remove: fn(path){{ assert(false) }}
}}
let sys = {{
    video: fn(query, index, field){{ if field == "count" {{ requests.push(query); return video_count }}; "" }}
    video_retry: fn(query){{ retries.push(query) }}
}}
let widget = {{render: fn(){{}} set_visible: fn(value){{}}}}
let ui = {{
    search: {{set_text: fn(value){{ field_text = value; field_sets += 1 }}}}
    status: {{set_text: fn(value){{ status_text = value }}}}
    main: widget player_pane: widget tabs: widget
    list: {{render: fn(){{}} set_visible: fn(value){{ list_visible = value }}}}
    history_empty: {{set_visible: fn(value){{ empty_visible = value }}}}
    player_title: {{set_text: fn(value){{}}}} player_meta: {{set_text: fn(value){{}}}}
    player_notice: {{set_visible: fn(value){{ player_notice = value }}}}
    history_notice: {{set_visible: fn(value){{ history_notice_visible = value }}}}
    query_notice: {{set_visible: fn(value){{ query_notice_visible = value }}}}
    search_retry_box: {{set_visible: fn(value){{ search_retry_visible = value }}}}
    reader: {{close: fn(){{}} open: fn(url){{ reader_url = url; reader_opens += 1; true }} open_embed: fn(player, page){{ reader_url = page; reader_opens += 1; true }}}}
}}
{logic}
boot()
assert(q == "{expected}" && field_text == q)
assert(status_text == "Searching YouTube for “{expected}”…")
assert(writes.len() == 0 && field_sets == 1)
// A committed chip/search replaces the field and persists its trimmed query.
search_for("  NASA science  ")
assert(q == "NASA science" && field_text == q)
assert(writes.len() == 1 && writes[0].path == "accounts/device/query.json")
assert(writes[0].data.parse_json().q == q && field_sets == 2)
// Result polling and tab changes must not discard a draft still being typed.
field_text = "Unsubmitted next query"
video_count = "0"
read_hits()
sync_status()
show("history")
show("search")
assert(loaded_q == q && field_text == "Unsubmitted next query")
assert(field_sets == 2 && writes.len() == 1)
search_for("   ")
assert(q == "NASA science" && field_text == "Unsubmitted next query")
assert(field_sets == 2 && writes.len() == 1)
// The touch widgets survive a restyle while script variables start over.
touch_search_changed("Unsubmitted next query")
show("history")
screen = "search"
field_text = ""
boot()
assert(screen == "history" && field_text == "Unsubmitted next query")
history = [1]
render_list()
assert(list_visible && !empty_visible)
history = []
render_list()
assert(!list_visible && empty_visible)
screen = "search"
hits = []
render_list()
assert(!list_visible && !empty_visible)
hits = [1]
render_list()
assert(list_visible && !empty_visible)
history = []
screen = "history"
let video = {{id: "fixture" title: "Video" channel: "Local" views: "" age: "" embed: "http://127.0.0.1/video"}}
play(video)
assert(screen == "player" && reader_url == video.embed && reader_opens == 1)
touch_player_failed()
assert(player_notice)
screen = "search"
playing = nil
boot()
assert(screen == "player" && playing.embed == video.embed)
touch_player_load()
assert(reader_opens == 2 && reader_url == video.embed && !player_notice)
close_player()
touch_player_failed()
assert(screen == "history" && playing == nil && !player_notice)
screen = "search"
q = "recover"
failed = true
loaded_q = q
mod.youtube_touch_query = q
video_count = "0"
sync_status()
assert(search_retry_visible)
// Retry can be tapped before the restyled script's delayed boot runs.
q = "lofi hip hop radio"
failed = false
loaded_q = ""
touch_search_retry()
assert(!failed && !search_retry_visible && loaded_q == "recover")
assert(requests[requests.len() - 1] == "recover" && retries[retries.len() - 1] == "recover")
failed = true
search_for("recover")
assert(requests[requests.len() - 1] == "recover" && retries[retries.len() - 1] == "recover")
search_for("another")
assert(requests[requests.len() - 1] == "another")
// A refused clear keeps the previous rows, explains the failure and can be retried.
screen = "history"
history = [video]
render_list()
fail_history_write = true
clear_history()
assert(history.len() == 1 && list_visible)
assert(status_text == "Couldn't clear history. Try again.")
assert(mod.youtube_history_error == status_text)
sync_status()
assert(status_text == "Couldn't clear history. Try again.")
fail_history_write = false
clear_history()
assert(history.len() == 0 && !list_visible && empty_visible && status_text == "")
assert(mod.youtube_history_error == "" && writes[writes.len() - 1].data == "[]")
// A storage failure must not stop the reader opening, and retrying after the
// file becomes writable clears the notice.
history = [video]
screen = "history"
fail_history_write = true
let opens_before = reader_opens
play(video)
assert(screen == "player" && reader_opens == opens_before + 1 && history.len() == 1)
assert(history_notice_visible && history_error == "Couldn't update History. Check storage, then reopen this video.")
fail_history_write = false
close_player()
play(video)
assert(!history_notice_visible && history_error == "")
// Search remains usable when its last-query file is temporarily read-only.
screen = "history"
fail_query_write = true
search_for("  storage failure query  ")
assert(screen == "search" && q == "storage failure query" && query_notice_visible)
assert(requests[requests.len() - 1] == q && mod.youtube_query_error != "")
q = "different after restyle"
boot()
assert(q == "storage failure query" && query_notice_visible)
fail_query_write = false
search_for("storage failure query")
assert(!query_notice_visible && mod.youtube_query_error == "")
assert(writes[writes.len() - 1].data.parse_json().q == q)
ui_desk = true
boot()
assert(field_text == q)
true
;"#),
                ..Default::default()
            });
            let errors = vm.take_errors();
            assert!(errors.is_empty(), "YouTube stored={stored}: {errors:?}");
            assert!(!value.is_err(), "YouTube callbacks returned {value:?}");
        });
    }
}

#[test]
fn youtube_recent_searches_report_failed_mutations_and_preserve_rows() {
    let source = include_str!("../../../apps/youtube/bundle/main.splash");
    let function = |name: &str| {
        let marker = format!("fn {name}(");
        let start = source.find(&marker).expect("YouTube function");
        source[start..].split_once("\nfn ").expect("next YouTube function").0.to_owned()
    };
    let logic = ["desk_recent_notice_sync", "desk_searches_save", "desk_searches_add",
        "desk_search_forget", "desk_searches_clear"].map(function).join("\n");
    let mut cx = Cx::new(Box::new(|_, _| {}));
    cx.with_vm(|vm| {
        makepad_widgets::script_mod(vm);
        vm.bx.captured_errors = Some(Vec::new());
        let value = vm.eval(ScriptMod {
            file: "youtube_recent_search_failure_test.splash".into(),
            code: format!(r#"
use mod.std.assert
let desk_searches = ["a" "b"]
let desk_searches_error = ""
let q = "a"
let fail_write = false
let stored = ["a" "b"]
let notice = ""
let visible = false
let gone = 0
let remembered = 0
let fs = {{write: fn(path, data){{
    if fail_write {{ assert(false) }}
    stored = data.parse_json()
}}}}
let ui = {{
    recent_notice_text: {{set_text: fn(text){{ notice = text }}}}
    recent_notice: {{set_visible: fn(value){{ visible = value }}}}
    tabs: {{render: fn(){{}}}}
}}
fn data_path(name) {{ name }}
fn desk_remember() {{ remembered += 1 }}
fn desk_q_gone() {{ gone += 1 }}
fn is_topic(text) {{ text == "NASA" }}
{logic}
fail_write = true
desk_search_forget("a")
assert(desk_searches.len() == 2 && stored.len() == 2 && gone == 0)
assert(visible && notice == "Couldn't remove this search. Try again.")
desk_searches_clear()
assert(desk_searches.len() == 2 && stored.len() == 2 && gone == 0)
assert(visible && notice == "Couldn't clear recent searches. Try again.")
desk_searches_add("new")
assert(desk_searches.len() == 2 && stored.len() == 2)
assert(visible && notice == "Couldn't save recent searches. Check storage.")
fail_write = false
desk_search_forget("a")
assert(desk_searches.len() == 1 && stored[0] == "b" && gone == 1 && !visible)
desk_searches_clear()
assert(desk_searches.len() == 0 && stored.len() == 0 && gone == 2 && !visible)
true
;"#),
            ..Default::default()
        });
        let errors = vm.take_errors();
        assert!(errors.is_empty(), "YouTube recent-search callbacks: {errors:?}");
        assert!(!value.is_err(), "YouTube recent-search callbacks returned {value:?}");
    });
}

#[test]
fn youtube_search_text_and_placeholder_read_in_normal_hover_and_focus_states() {
    let source = include_str!("../../../apps/youtube/bundle/main.splash");
    let prelude = source.split_once("// END shared app interface").unwrap().0;
    let field = source.split_once("search := ").unwrap().1;
    let mut depth = 0;
    let end = field.char_indices().find_map(|(index, ch)| {
        if ch == '{' { depth += 1; }
        if ch == '}' {
            depth -= 1;
            if depth == 0 { return Some(index + 1); }
        }
        None
    }).unwrap();
    // Use the actual shipped field and its actual shared prelude. Only
    // its network callback is replaced; the local colour overrides are
    // retained so a fixed dark background fails the light-theme checks.
    let styles = format!("{prelude}\nlet ink = ui_ink\nlet secondary = ui_muted\nfn search_for(text) {{}}\nfn touch_search_changed(text) {{}}\nlet Search = {}", &field[..end]);
    for style in DesktopStyle::ALL {
        for dark in [false, true] {
            if dark && !style.supports_dark() { continue; }
            let colors = script_colors(style, dark, &styles, &[
                "Search.draw_bg.color", "Search.draw_text.color",
                "Search.draw_bg.color_hover", "Search.draw_text.color_hover",
                "Search.draw_bg.color_focus", "Search.draw_text.color_focus",
                "Search.draw_bg.color_empty", "Search.draw_text.color_empty",
                "Search.draw_bg.color_hover", "Search.draw_text.color_empty_hover",
                "Search.draw_bg.color_focus", "Search.draw_text.color_empty_focus",
            ]);
            for (state, pair) in colors.chunks_exact(2).enumerate() {
                assert!(contrast(pair[0], pair[1]) >= 4.5, "{} dark={dark}, state={state}: {:08x} on {:08x}", style.id(), pair[1], pair[0]);
            }
        }
    }
}

#[test]
fn system_app_interface_keeps_text_and_controls_readable_in_every_appearance() {
    for style in DesktopStyle::ALL {
        for dark in [false, true] {
            if dark && !style.supports_dark() { continue; }
            let colors = news_colors(style, dark, &[
                "ui_page", "ui_surface", "ui_field", "ui_ink", "ui_muted", "ui_link", "ui_success", "ui_danger", "ui_primary",
            ]);
            let at = format!("{} dark={dark}", style.id());
            for surface in &colors[..3] {
                for ink in &colors[3..8] {
                    assert!(contrast(*ink, *surface) >= 4.5, "{at}: {ink:08x} on {surface:08x}: {}", contrast(*ink, *surface));
                }
            }
            assert!(contrast(0xffffffff, colors[8]) >= 4.5, "{at}: primary action");
        }
    }
}

/// On a Mac's sheets the sidebar, toolbar and hover wash carry text too, and
/// the selection carries a row's title and caption.
#[test]
fn the_macs_chrome_keeps_text_readable_in_both_appearances() {
    for dark in [false, true] {
        let colors = news_colors(DesktopStyle::Macos, dark, &[
            "ui_sidebar", "ui_bar", "ui_hover", "ui_selected", "ui_ink", "ui_muted", "ui_link", "ui_success", "ui_danger",
        ]);
        for surface in &colors[..3] {
            for ink in &colors[4..] {
                assert!(contrast(*ink, *surface) >= 4.5, "dark={dark}: {ink:08x} on {surface:08x}: {}", contrast(*ink, *surface));
            }
        }
        for ink in &colors[4..6] {
            assert!(contrast(*ink, colors[3]) >= 4.5, "dark={dark}: {ink:08x} on the selection {:08x}", colors[3]);
        }
    }
}

/// What the shared interface makes of a style: a desktop's pointer metrics, or
/// a phone's touch metrics exactly as they were.
#[test]
fn the_shared_interface_draws_for_a_pointer_on_a_desktop_and_for_touch_on_a_phone() {
    let prelude = include_str!("../../../apps/interface.splash");
    for style in DesktopStyle::ALL {
        for dark in [false, true] {
            if dark && !style.supports_dark() { continue; }
            let mut cx = Cx::new(Box::new(|_, _| {}));
            let (desk, button, field, title) = cx.with_vm(|vm| {
                makepad_widgets::script_mod(vm);
                desktop_style::install(vm, load_sheet(style, dark));
                vm.with_reload(makepad_widgets::script_mod);
                let mut read = |expression: &str| {
                    vm.bx.captured_errors = Some(Vec::new());
                    let value = vm.eval(ScriptMod {
                        file: "interface_metrics_test.splash".into(),
                        code: format!("use mod.prelude.widgets.*\n{prelude}\n{expression}\n;"),
                        ..Default::default()
                    });
                    let errors = vm.take_errors();
                    assert!(errors.is_empty(), "{expression}: {errors:?}");
                    value
                };
                let desk = read("ui_desk").as_bool().unwrap_or(false);
                let button = read("ui_by(44.0, 28.0)").as_f64().unwrap_or_default();
                let field = read("ui_by(48.0, 28.0)").as_f64().unwrap_or_default();
                let title = read("ui_fs(28.0)").as_f64().unwrap_or_default();
                (desk, button, field, title)
            });
            let at = format!("{} dark={dark}", style.id());
            assert_eq!(desk, !style.mobile(), "{at}");
            if style.mobile() {
                assert_eq!((button, field, title), (44.0, 48.0, 28.0), "{at}: a phone keeps its touch metrics");
            } else {
                assert_eq!((button, field, title), (28.0, 28.0, 16.5), "{at}: 28-point controls, a 22-point title");
            }
        }
    }
}

/// WCAG contrast ratio of two opaque `0xRRGGBBAA` colours.
fn contrast(a: u32, b: u32) -> f64 {
    let luminance = |c: u32| {
        let channel = |shift: u32| {
            let v = ((c >> shift) & 0xff) as f64 / 255.0;
            if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * channel(24) + 0.7152 * channel(16) + 0.0722 * channel(8)
    };
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

#[test]
fn news_source_tabs_read_on_the_page_in_light_and_dark() {
    for style in DesktopStyle::ALL {
        for dark in [false, true] {
            if dark && !style.supports_dark() {
                continue;
            }
            let [page, tab, active_text, active_pill] = news_colors(
                style,
                dark,
                &["theme.color_bg_app", "TabButton.draw_text.color", "TabButtonActive.draw_text.color", "TabButtonActive.draw_bg.color"],
            )[..] else {
                unreachable!()
            };
            let at = format!("{} {}", style.id(), if dark { "dark" } else { "light" });
            assert!(contrast(tab, page) >= 4.5, "{at}: a source tab's text {tab:08x} on the page {page:08x}");
            assert!(contrast(active_text, active_pill) >= 4.5, "{at}: the open tab's text {active_text:08x} on its pill {active_pill:08x}");
            assert!(contrast(active_pill, page) >= 3.0, "{at}: the open tab's pill {active_pill:08x} on the page {page:08x}");
        }
    }
}
