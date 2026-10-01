//! Erasing an app's agent (ADR 0004 §11, octos UPCR-2026-034 `peer/purge`,
//! octos#2649).
//!
//! When the person removes an account from an app, or uninstalls the app,
//! the host's storage lifecycle deletes the folders first; then the host
//! asks the kernel to erase the (app, account) peer: its transcripts (the
//! peer's own session and every request context), its memory namespace and
//! its blackboard. The kernel frees the (app, account) binding, so adding
//! the account again (or installing the app again) makes a NEW agent with
//! fresh memory. Signing out is not this: it keeps the peer, suspended on
//! the shell's side (the broker refuses its inputs and calls).
//!
//! - **What is purged** is what the host recorded: each peer's record
//!   (`<state dir>/<namespace>.peer`, [`crate::peer_record`]) holds the
//!   host token and the name a resume finds it by. `peer/purge` names the
//!   peer by that name, with the token, owned by the system agent's session.
//!   A peer the host has no record for was never made here: nothing to do.
//! - **Busy**: a turn of the peer that does not stop within octos's 10 s
//!   fails the purge with `peer_purge_busy` (the peer stays closed, nothing
//!   is erased); a purge already running answers `peer_purge_in_progress`.
//!   Both are retried ([`RETRY_WAITS`]).
//! - **Done**: `purged`, `already_purged` and `peer_not_found` all mean no
//!   peer is left; the record is dropped, and every live broker of the app
//!   forgets it (a broker bound to it prepares a new peer next time).
//!   Anything else keeps the record (the peer stays suspended) and is
//!   returned as the error.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

use crate::broker::{app_namespace, raw_tag, Connector, Link};
use crate::peer_record;

/// The raw AppUI method (UPCR-2026-034).
pub const PEER_PURGE: &str = "peer/purge";

/// The waits before each retry of a purge refused as busy.
pub const RETRY_WAITS: [Duration; 3] = [Duration::from_secs(1), Duration::from_secs(3), Duration::from_secs(6)];

/// How long one `peer/purge` may take (octos waits up to 10 s for the
/// peer's turns to stop, then erases).
const PURGE_TIMEOUT: Duration = Duration::from_secs(45);

/// The host's side of a purge: whose peers, and where it keeps them.
#[derive(Clone, Debug)]
pub struct PurgeHost {
    /// The kernel profile (`_main` in a shell).
    pub profile_id: String,
    /// The session that owns the app peers (the system agent's).
    pub originator: String,
    /// Where the host keeps each peer's record (`BrokerConfig::state_dir`).
    pub state_dir: PathBuf,
    /// Waits before each retry of a busy purge ([`RETRY_WAITS`]).
    pub retry_waits: Vec<Duration>,
}

impl PurgeHost {
    pub fn new(profile_id: impl Into<String>, originator: impl Into<String>, state_dir: impl Into<PathBuf>) -> Self {
        Self { profile_id: profile_id.into(), originator: originator.into(), state_dir: state_dir.into(), retry_waits: RETRY_WAITS.to_vec() }
    }
}

/// What one purge did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Purged {
    /// The namespaces whose peers are gone (their records dropped).
    pub erased: Vec<String>,
    /// The ones that failed, with why (their records kept).
    pub failed: Vec<(String, String)>,
}

impl Purged {
    pub fn ok(&self) -> bool {
        self.failed.is_empty()
    }
}

/// The record namespaces of `app_id`'s peer for `account` (`None`: every
/// account the host has a record for).
pub fn recorded(state_dir: &std::path::Path, app_id: &str, account: Option<&str>) -> Vec<String> {
    match account {
        Some(account) => {
            let mut keys = vec![app_namespace(app_id, account)];
            // A peer made before accounts were normalized.
            let raw = format!("app/{app_id}/acct-{}", raw_tag(account));
            if !keys.contains(&raw) {
                keys.push(raw);
            }
            keys.into_iter().filter(|k| peer_record::load(state_dir, k).is_some()).collect()
        }
        None => peer_record::namespaces_under(state_dir, &format!("app/{app_id}/acct-")),
    }
}

/// Erase `app_id`'s peer for `account` (`None`: every recorded account of
/// the app) on the kernel `connector` reaches. `labels` are the app labels
/// a broker of the app may have named a peer with (the broker's name for a
/// record saved before records carried the name is `<label> <8 hex>`, and
/// the label depends on how the app was hosted); the label of a broker of
/// the app that ran in this process is tried first
/// ([`crate::broker::known_label`]).
pub async fn purge_app(connector: &dyn Connector, host: &PurgeHost, app_id: &str, labels: &[String], account: Option<&str>) -> Purged {
    let mut done = Purged { erased: Vec::new(), failed: Vec::new() };
    let namespaces = recorded(&host.state_dir, app_id, account);
    if namespaces.is_empty() {
        return done;
    }
    let mut link = match connector.connect().await {
        Ok(link) => link,
        Err(e) => {
            done.failed = namespaces.into_iter().map(|n| (n, format!("no kernel: {e}"))).collect();
            return done;
        }
    };
    for namespace in namespaces {
        let mut candidates: Vec<String> = crate::broker::known_label(app_id).into_iter().collect();
        for label in labels {
            if !candidates.contains(label) {
                candidates.push(label.clone());
            }
        }
        match purge_one(link.as_mut(), host, &candidates, &namespace).await {
            Ok(()) => {
                peer_record::remove(&host.state_dir, &namespace);
                crate::broker::forget_purged(connector.kernel_id().as_deref(), app_id, &namespace);
                done.erased.push(namespace);
            }
            Err(e) => {
                eprintln!("app-peers: {app_id}: erasing the agent of {namespace} failed: {e}");
                done.failed.push((namespace, e));
            }
        }
    }
    done
}

/// [`purge_app`] on a thread of its own (the shell's lifecycle runs on the
/// UI thread). `done` runs on THAT background thread: it must only touch
/// thread-safe state (the shell's `Storage` locks its own state).
pub fn purge_in_background(
    connector: Arc<dyn Connector>,
    host: PurgeHost,
    app_ids: Vec<String>,
    labels: Vec<String>,
    account: Option<String>,
    done: impl FnOnce(Purged) + Send + 'static,
) {
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
            Ok(rt) => rt,
            Err(e) => return done(Purged { erased: Vec::new(), failed: vec![(String::new(), format!("no runtime: {e}"))] }),
        };
        let mut all = Purged { erased: Vec::new(), failed: Vec::new() };
        for app in &app_ids {
            let purged = runtime.block_on(purge_app(connector.as_ref(), &host, app, &labels, account.as_deref()));
            all.erased.extend(purged.erased);
            all.failed.extend(purged.failed);
        }
        done(all);
    });
}

/// The names a peer recorded under `namespace` may have: its recorded name
/// (`exact`), else the broker's name for a record saved before names were,
/// `<label> <first 8 hex of the tag>`, for each candidate label (guesses).
pub fn candidate_names(record: &peer_record::PeerRecord, namespace: &str, labels: &[String]) -> (Vec<String>, bool) {
    if let Some(name) = &record.name {
        return (vec![name.clone()], true);
    }
    let tag = record.namespace.as_deref().unwrap_or(namespace).rsplit("acct-").next().unwrap_or_default().to_owned();
    let short = &tag[..tag.len().min(8)];
    (labels.iter().map(|label| format!("{label} {short}")).collect(), false)
}

/// One peer: `peer/purge`, retried while busy. `peer_not_found` counts as
/// gone only for the peer's RECORDED name; for a guessed name (a record
/// from before names were kept) it means the guess was wrong, so the next
/// candidate is tried, and when none is found the purge FAILS: the record
/// is kept and the agent stays suspended, never forgotten while octos may
/// still hold its memory.
async fn purge_one(link: &mut dyn Link, host: &PurgeHost, labels: &[String], namespace: &str) -> Result<(), String> {
    let record = peer_record::load(&host.state_dir, namespace).ok_or("its record is gone")?;
    let (names, exact) = candidate_names(&record, namespace, labels);
    for name in &names {
        match purge_named(link, host, name, &record.token).await? {
            true => return Ok(()),
            false if exact => return Ok(()),
            false => continue,
        }
    }
    Err(format!("peer_not_found: no peer under any name the host could derive ({names:?}); the record is kept"))
}

/// `peer/purge` of the peer called `name`: `Ok(true)` purged (or already),
/// `Ok(false)` the kernel has no such peer.
async fn purge_named(link: &mut dyn Link, host: &PurgeHost, name: &str, token: &str) -> Result<bool, String> {
    let params = json!({
        "session_id": host.originator,
        "peer": name,
        "host_token": token,
        "profile_id": host.profile_id,
    });
    let mut waits = host.retry_waits.iter();
    loop {
        match request(link, PEER_PURGE, params.clone()).await {
            Ok(_) => return Ok(true),
            Err(Refused { kind, .. }) if kind == "peer_not_found" => return Ok(false),
            Err(Refused { kind, message }) if kind == "peer_purge_busy" || kind == "peer_purge_in_progress" => match waits.next() {
                Some(wait) => tokio::time::sleep(*wait).await,
                None => return Err(format!("{kind}: {message}")),
            },
            Err(Refused { kind, message }) => return Err(if kind.is_empty() { message } else { format!("{kind}: {message}") }),
        }
    }
}

struct Refused {
    kind: String,
    message: String,
}

async fn request(link: &mut dyn Link, method: &str, params: Value) -> Result<Value, Refused> {
    let id = format!("app-peers-purge-{}", uuid::Uuid::new_v4());
    let frame = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
    link.send(frame.to_string()).map_err(|e| Refused { kind: String::new(), message: e })?;
    let reply = tokio::time::timeout(PURGE_TIMEOUT, async {
        loop {
            let text = link.recv().await?;
            let Ok(frame) = serde_json::from_str::<Value>(&text) else { continue };
            if frame["id"] == id.as_str() && frame.get("method").is_none() {
                return Ok(frame);
            }
        }
    })
    .await
    .map_err(|_| Refused { kind: String::new(), message: format!("{method} timed out") })?
    .map_err(|e: String| Refused { kind: String::new(), message: e })?;
    match reply.get("error") {
        Some(error) => Err(Refused {
            kind: error["data"]["kind"].as_str().unwrap_or_default().to_owned(),
            message: error["message"].as_str().unwrap_or("refused").to_owned(),
        }),
        None => Ok(reply["result"].clone()),
    }
}
