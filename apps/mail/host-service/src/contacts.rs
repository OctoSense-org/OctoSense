//! Who the person writes to, for the shell's approval rules ("recipients in
//! my contacts", ADR 0004 §8). Read-only, host-side, and only when the
//! person turned on "Use my contacts in approval rules": no app calls this.
//!
//! The known set is the person's own account addresses plus every address
//! they have sent mail to: the recipients this service sent for them
//! ([`SENT_TO_FILE`], per account) and the `To`/`Cc` of the messages in
//! each account's sent folder. It reads `accounts.json`, `folders-<id>.json`,
//! the sent folders' `box-<id>-….json` and [`SENT_TO_FILE`]; never the
//! secret store.

use crate::{mailbox_file, text, write_atomic, INBOX};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::Path;

/// Relative to `<host_dir>/mail`: `{"<account id>": ["ana@example.org", …]}`.
pub const SENT_TO_FILE: &str = "sent-to.json";

/// The addresses of the person's accounts and of everyone they sent mail
/// to, lowercased and sorted. `host_dir` is the one the Card runner hands
/// every host service (the mail lives in `<host_dir>/mail`).
pub fn known_addresses(host_dir: &Path) -> Vec<String> {
    let dir = host_dir.join("mail");
    let mut known = BTreeSet::new();
    let accounts: Vec<Value> = read(&dir.join("accounts.json")).and_then(|v| v.as_array().cloned()).unwrap_or_default();
    for account in &accounts {
        add(&mut known, text(account, "address"));
        let id = text(account, "id");
        if id.is_empty() {
            continue;
        }
        let folders = read(&dir.join(format!("folders-{id}.json"))).and_then(|v| v.as_array().cloned()).unwrap_or_default();
        for folder in folders.iter().filter(|f| text(f, "role") == "sent") {
            let name = text(folder, "id");
            if name.is_empty() || name == INBOX {
                continue;
            }
            let path = mailbox_file(&dir, id, name);
            let messages = read(&path).and_then(|m| m["messages"].as_array().cloned()).unwrap_or_default();
            for message in &messages {
                for key in ["to", "cc"] {
                    add_header(&mut known, text(message, key));
                }
            }
        }
    }
    if let Some(Value::Object(sent)) = read(&dir.join(SENT_TO_FILE)) {
        for addresses in sent.values() {
            for a in addresses.as_array().into_iter().flatten() {
                add(&mut known, a.as_str().unwrap_or(""));
            }
        }
    }
    known.into_iter().collect()
}

/// After a send went out: remember its recipient under the account.
pub(crate) fn record_sent(mail_dir: &Path, account_id: &str, to: &str) {
    let path = mail_dir.join(SENT_TO_FILE);
    let mut sent = read(&path).filter(Value::is_object).unwrap_or_else(|| json!({}));
    let mut recipients = BTreeSet::new();
    add_header(&mut recipients, to);
    if recipients.is_empty() {
        return;
    }
    let list = sent[account_id].as_array().cloned().unwrap_or_default();
    let mut all: BTreeSet<String> = list.iter().filter_map(|v| v.as_str().map(str::to_string)).collect();
    let before = all.len();
    all.extend(recipients);
    if all.len() != before {
        sent[account_id] = json!(all.into_iter().collect::<Vec<_>>());
        let _ = write_atomic(&path, &serde_json::to_vec_pretty(&sent).unwrap());
    }
}

/// An account is forgotten: so are the people it sent to.
pub(crate) fn forget(mail_dir: &Path, account_id: &str) {
    let path = mail_dir.join(SENT_TO_FILE);
    if let Some(Value::Object(mut sent)) = read(&path) {
        if sent.remove(account_id).is_some() {
            let _ = write_atomic(&path, &serde_json::to_vec_pretty(&Value::Object(sent)).unwrap());
        }
    }
}

fn read(path: &Path) -> Option<Value> {
    std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok())
}

fn add(known: &mut BTreeSet<String>, address: &str) {
    let a = address.trim().to_lowercase();
    if a.contains('@') && !a.contains(char::is_whitespace) {
        known.insert(a);
    }
}

/// "Ana <ana@example.org>, bo@example.org" → each address.
fn add_header(known: &mut BTreeSet<String>, header: &str) {
    if header.trim().is_empty() {
        return;
    }
    match mailparse::addrparse(header) {
        Ok(list) => {
            for addr in list.iter() {
                match addr {
                    mailparse::MailAddr::Single(s) => add(known, &s.addr),
                    mailparse::MailAddr::Group(g) => g.addrs.iter().for_each(|s| add(known, &s.addr)),
                }
            }
        }
        Err(_) => header.split([',', ';']).for_each(|a| add(known, a)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sent_folder_names_whom_the_person_wrote_to() {
        let host = std::env::temp_dir().join(format!("mail-contacts-{}", std::process::id()));
        let dir = host.join("mail");
        let _ = std::fs::remove_dir_all(&host);
        std::fs::create_dir_all(&dir).unwrap();
        let w = |p: std::path::PathBuf, v: Value| std::fs::write(p, v.to_string()).unwrap();
        w(dir.join("accounts.json"), json!([{"id": "a1", "address": "Me@Example.com", "apps": ["os.mail"]}]));
        w(dir.join("folders-a1.json"), json!([{"id": INBOX, "role": "inbox"}, {"id": "Sent Items", "role": "sent"}, {"id": "Junk", "role": "junk"}]));
        w(mailbox_file(&dir, "a1", "Sent Items"), json!({"messages": [{"to": "Ana <ana@example.org>, bo@example.org", "cc": "chen@example.org"}]}));
        // Senders in the inbox and junk are not contacts.
        w(mailbox_file(&dir, "a1", INBOX), json!({"messages": [{"address": "stranger@example.net", "to": "me@example.com"}]}));
        w(mailbox_file(&dir, "a1", "Junk"), json!({"messages": [{"to": "spam@example.net"}]}));
        record_sent(&dir, "a1", "dee@example.org");
        assert_eq!(known_addresses(&host), ["ana@example.org", "bo@example.org", "chen@example.org", "dee@example.org", "me@example.com"]);
        forget(&dir, "a1");
        assert!(!known_addresses(&host).contains(&"dee@example.org".to_string()));
        let _ = std::fs::remove_dir_all(&host);
    }
}
