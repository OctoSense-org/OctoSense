//! Native-host library. A script or model never receives a Grant or a client.
//! Network adapters must deduplicate on operation.id and verify their evidence.
use rand::RngCore;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug)]
pub enum Error {
    Authorization,
    Expired,
    Conflict,
    Integrity,
    Invalid(String),
    Storage(String),
    State(String),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Self::Storage(e.to_string())
    }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::Invalid(e.to_string())
    }
}
pub fn new_id() -> String {
    let mut b = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut b);
    hex::encode(b)
}

struct AuthorityState {
    id: String,
    account: Mutex<Option<String>>,
    generation: AtomicU64,
}
#[derive(Clone)]
pub struct Authority(Arc<AuthorityState>);
impl Default for Authority {
    fn default() -> Self {
        Self(Arc::new(AuthorityState {
            id: new_id(),
            account: Mutex::new(None),
            generation: AtomicU64::new(0),
        }))
    }
}
/// Intentionally opaque and not serializable; only a trusted host issues it.
#[derive(Clone)]
pub struct Grant {
    authority: Arc<AuthorityState>,
    account: String,
    app: String,
    instance: String,
    generation: u64,
    expires: u64,
    revoked: Arc<AtomicBool>,
    permissions: BTreeSet<String>,
}
impl Authority {
    pub fn set_account(&self, account: Option<&str>) {
        let mut current = self.0.account.lock().unwrap();
        if current.as_deref() != account {
            *current = account.map(str::to_owned);
            self.0.generation.fetch_add(1, Ordering::SeqCst);
        }
    }
    /// Call after the host's consent panel; `now` comes from the host clock.
    pub fn grant(&self, app: &str, permissions: &[&str], now: u64, lifetime: u64) -> Result<Grant> {
        let current = self.0.account.lock().unwrap();
        let account = current.clone().ok_or(Error::Authorization)?;
        let generation = self.0.generation.load(Ordering::SeqCst);
        drop(current);
        if app.is_empty() || permissions.is_empty() || lifetime == 0 {
            return Err(Error::Invalid("empty grant".into()));
        }
        Ok(Grant {
            authority: self.0.clone(),
            account,
            app: app.into(),
            instance: new_id(),
            generation,
            expires: now.saturating_add(lifetime.min(3600)),
            revoked: Arc::new(AtomicBool::new(false)),
            permissions: permissions.iter().map(|s| s.to_string()).collect(),
        })
    }
}
impl Grant {
    pub fn account(&self) -> &str {
        &self.account
    }
    pub fn app(&self) -> &str {
        &self.app
    }
    pub fn revoke(&self) {
        self.revoked.store(true, Ordering::SeqCst);
    }
    /// Native adapters use this before local state changes as well as effects.
    /// It does not serialize or manufacture an identity for guest code.
    pub fn check(&self, permission: &str, now: u64) -> Result<()> {
        let account = self.authority.account.lock().unwrap();
        if self.revoked.load(Ordering::SeqCst)
            || self.authority.generation.load(Ordering::SeqCst) != self.generation
            || account.as_deref() != Some(self.account.as_str())
            || !self.permissions.contains(permission)
        {
            return Err(Error::Authorization);
        }
        if now >= self.expires {
            return Err(Error::Expired);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Action {
    pub permission: String,
    pub target: String,
    pub summary: String,
    pub payload: Value,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Prepared,
    Queued,
    Dispatching,
    Confirmed,
    Failed,
    Unknown,
    Cancelled,
    Expired,
}
impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::Queued => "queued",
            Self::Dispatching => "dispatching",
            Self::Confirmed => "confirmed",
            Self::Failed => "failed",
            Self::Unknown => "unknown",
            Self::Cancelled => "cancelled",
            Self::Expired => "expired",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Evidence {
    pub operation_id: String,
    pub external_id: String,
    pub account: String,
    pub target: String,
    pub digest: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Receipt {
    pub status: Status,
    pub evidence: Option<Evidence>,
    pub message: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Operation {
    pub id: String,
    pub app: String,
    pub account: String,
    pub action: Action,
    pub revision: u64,
    pub digest: String,
    pub created_at: u64,
    pub expires_at: u64,
    pub status: Status,
    pub receipt: Option<Receipt>,
    #[serde(skip_serializing)]
    pub(crate) authority_id: String,
    #[serde(skip_serializing)]
    pub(crate) generation: u64,
    #[serde(skip_serializing)]
    pub(crate) instance: String,
}
pub enum Dispatch {
    Verified(Evidence),
    Rejected(String),
    Uncertain(String),
}
/// Implementations own the SDK client. Never expose a raw client to an app.
pub trait Adapter {
    fn validate(&self, action: &Action, expected_revision: u64) -> Result<()>;
    fn dispatch(&self, operation: &Operation) -> Dispatch;
    fn lookup(&self, operation: &Operation) -> Result<Option<Evidence>>;
}
fn digest(action: &Action) -> Result<String> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(action)?)))
}

pub struct Journal {
    db: Connection,
}
impl Journal {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let db = Connection::open(path)?;
        db.busy_timeout(std::time::Duration::from_secs(3))?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;")?;
        let version: u32 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > 1 {
            return Err(Error::Storage(
                "unsupported journal schema; original file preserved".into(),
            ));
        }
        db.execute_batch("CREATE TABLE IF NOT EXISTS operations(id TEXT PRIMARY KEY, app TEXT NOT NULL, account TEXT NOT NULL, status TEXT NOT NULL, body TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS operations_scope ON operations(app, account);
            CREATE INDEX IF NOT EXISTS operations_unresolved_scope ON operations(app, account) WHERE status IN ('dispatching','unknown');
            PRAGMA user_version=1;")?;
        Ok(Self { db })
    }
    fn body(op: &Operation) -> Result<String> {
        // Context bindings are host metadata, stored separately from public JSON.
        let mut body = serde_json::to_value(op)?;
        body["authority_id"] = op.authority_id.clone().into();
        body["generation"] = op.generation.into();
        body["instance"] = op.instance.clone().into();
        Ok(body.to_string())
    }
    fn get(&self, id: &str) -> Result<Operation> {
        let raw: String = self
            .db
            .query_row("SELECT body FROM operations WHERE id=?1", [id], |r| {
                r.get(0)
            })
            .optional()?
            .ok_or(Error::Invalid("operation not found".into()))?;
        let op: Operation = serde_json::from_str(&raw)?;
        if op.id != id || digest(&op.action)? != op.digest {
            return Err(Error::Integrity);
        }
        Ok(op)
    }
    fn transition(&self, op: &Operation, from: &[Status]) -> Result<()> {
        let mut body = serde_json::to_value(op)?;
        body["authority_id"] = op.authority_id.clone().into();
        body["generation"] = op.generation.into();
        body["instance"] = op.instance.clone().into();
        let allowed = from
            .iter()
            .map(|s| format!("'{}'", s.as_str()))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "UPDATE operations SET status=?2,body=?3 WHERE id=?1 AND status IN ({allowed})"
        );
        if self
            .db
            .execute(&sql, params![op.id, op.status.as_str(), body.to_string()])?
            != 1
        {
            return Err(Error::Conflict);
        }
        Ok(())
    }
    fn owns(grant: &Grant, op: &Operation, now: u64) -> Result<()> {
        grant.check(&op.action.permission, now)?;
        if grant.app != op.app || grant.account != op.account {
            return Err(Error::Authorization);
        }
        Ok(())
    }
    fn same_context(grant: &Grant, op: &Operation) -> Result<()> {
        if grant.authority.id != op.authority_id
            || grant.generation != op.generation
            || grant.instance != op.instance
        {
            return Err(Error::Authorization);
        }
        Ok(())
    }
    pub fn prepare(
        &mut self,
        grant: &Grant,
        action: Action,
        revision: u64,
        now: u64,
        ttl: u64,
    ) -> Result<Operation> {
        grant.check(&action.permission, now)?;
        if action.target.trim().is_empty()
            || action.summary.trim().is_empty()
            || action.summary.len() > 1024
            || serde_json::to_vec(&action)?.len() > 32768
            || !(1..=120).contains(&ttl)
        {
            return Err(Error::Invalid(
                "invalid action or confirmation lifetime".into(),
            ));
        }
        let op = Operation {
            id: new_id(),
            app: grant.app.clone(),
            account: grant.account.clone(),
            digest: digest(&action)?,
            action,
            revision,
            created_at: now,
            expires_at: now.saturating_add(ttl).min(grant.expires),
            status: Status::Prepared,
            receipt: None,
            authority_id: grant.authority.id.clone(),
            generation: grant.generation,
            instance: grant.instance.clone(),
        };
        // One current preview per app/account. Editing content creates a new ID
        // and invalidates earlier unattempted confirmations in one transaction.
        let body = Self::body(&op)?;
        let tx = self.db.transaction()?;
        tx.execute("UPDATE operations SET status='cancelled',body=json_set(body,'$.status','cancelled') WHERE app=?1 AND account=?2 AND status IN ('prepared','queued')",params![op.app,op.account])?;
        tx.execute(
            "INSERT INTO operations(id,app,account,status,body) VALUES(?1,?2,?3,?4,?5)",
            params![op.id, op.app, op.account, op.status.as_str(), body],
        )?;
        tx.commit()?;
        Ok(op)
    }
    /// Unsent previews may be reused only within the exact active consent.
    pub fn preview_current(grant: &Grant, op: &Operation, now: u64) -> bool {
        Self::owns(grant, op, now).is_ok()
            && Self::same_context(grant, op).is_ok()
            && matches!(op.status, Status::Prepared | Status::Queued)
            && now < op.expires_at
    }
    pub fn confirm(
        &mut self,
        grant: &Grant,
        id: &str,
        adapter: &impl Adapter,
        now: u64,
    ) -> Result<Operation> {
        let mut op = self.get(id)?;
        Self::owns(grant, &op, now)?;
        Self::same_context(grant, &op)?;
        if matches!(
            op.status,
            Status::Queued | Status::Dispatching | Status::Unknown | Status::Confirmed
        ) {
            return Ok(op);
        }
        if op.status != Status::Prepared {
            return Err(Error::State("cannot confirm this operation".into()));
        }
        if now >= op.expires_at {
            op.status = Status::Expired;
            self.transition(&op, &[Status::Prepared])?;
            return Err(Error::Expired);
        }
        adapter.validate(&op.action, op.revision)?;
        op.status = Status::Queued;
        self.transition(&op, &[Status::Prepared])?;
        Ok(op)
    }
    pub fn cancel(&mut self, grant: &Grant, id: &str, now: u64) -> Result<()> {
        let mut op = self.get(id)?;
        Self::owns(grant, &op, now)?;
        Self::same_context(grant, &op)?;
        if !matches!(op.status, Status::Prepared | Status::Queued) {
            return Err(Error::State(
                "an attempted operation must be reconciled".into(),
            ));
        }
        op.status = Status::Cancelled;
        self.transition(&op, &[Status::Prepared, Status::Queued])
    }
    /// A current host read grant may retire all unsent previews for its own
    /// app/account, including previews bound to an earlier consent. Attempted
    /// operations keep their original IDs and must still be reconciled.
    pub fn invalidate_previews(&mut self, grant: &Grant, now: u64) -> Result<usize> {
        grant.check("read", now)?;
        Ok(self.db.execute(
            "UPDATE operations SET status='cancelled',body=json_set(body,'$.status','cancelled') WHERE app=?1 AND account=?2 AND status IN ('prepared','queued')",
            params![grant.app, grant.account],
        )?)
    }
    pub fn execute(
        &mut self,
        grant: &Grant,
        id: &str,
        adapter: &impl Adapter,
        now: u64,
    ) -> Result<Operation> {
        let mut op = self.get(id)?;
        Self::owns(grant, &op, now)?;
        // A completed effect is observable under a fresh consent for the same account.
        if op.status == Status::Confirmed {
            return Ok(op);
        }
        Self::same_context(grant, &op)?;
        if op.status != Status::Queued {
            return Err(Error::State(
                "reconcile attempted operations; never send them blindly".into(),
            ));
        }
        if now >= op.expires_at {
            op.status = Status::Expired;
            self.transition(&op, &[Status::Queued])?;
            return Err(Error::Expired);
        }
        adapter.validate(&op.action, op.revision)?;
        // Conditional acquisition also protects multiple native-host connections.
        op.status = Status::Dispatching;
        let mut body = serde_json::to_value(&op)?;
        body["authority_id"] = op.authority_id.clone().into();
        body["generation"] = op.generation.into();
        body["instance"] = op.instance.clone().into();
        let changed = self.db.execute(
            "UPDATE operations SET status='dispatching',body=?2 WHERE id=?1 AND status='queued'",
            params![id, body.to_string()],
        )?;
        if changed != 1 {
            return Err(Error::Conflict);
        }
        grant.check(&op.action.permission, now)?;
        let result = adapter.dispatch(&op);
        self.finish(op, result)
    }
    fn finish(&mut self, mut op: Operation, result: Dispatch) -> Result<Operation> {
        let (status, evidence, message) = match result {
            Dispatch::Verified(e)
                if e.operation_id == op.id
                    && e.account == op.account
                    && e.target == op.action.target
                    && e.digest == op.digest
                    && !e.external_id.trim().is_empty() =>
            {
                (
                    Status::Confirmed,
                    Some(e),
                    "host verified the external result".into(),
                )
            }
            Dispatch::Verified(_) => (
                Status::Unknown,
                None,
                "external evidence did not match the confirmed operation".into(),
            ),
            Dispatch::Rejected(m) => (Status::Failed, None, m),
            Dispatch::Uncertain(m) => (Status::Unknown, None, m),
        };
        op.status = status;
        op.receipt = Some(Receipt {
            status,
            evidence,
            message,
        });
        match self.transition(&op, &[Status::Dispatching, Status::Unknown]) {
            Ok(()) => Ok(op),
            Err(Error::Conflict) => {
                let current = self.get(&op.id)?;
                if current.status == Status::Confirmed {
                    Ok(current)
                } else {
                    Err(Error::Conflict)
                }
            }
            Err(e) => Err(e),
        }
    }
    /// Read-only external lookup: valid even after the action deadline; never sends.
    pub fn reconcile(
        &mut self,
        grant: &Grant,
        id: &str,
        adapter: &impl Adapter,
        now: u64,
    ) -> Result<Operation> {
        let op = self.get(id)?;
        Self::owns(grant, &op, now)?;
        if op.status == Status::Confirmed {
            return Ok(op);
        }
        if !matches!(op.status, Status::Unknown | Status::Dispatching) {
            return Err(Error::State(
                "operation has no uncertain external effect".into(),
            ));
        }
        match adapter.lookup(&op)? {
            Some(e) => self.finish(op, Dispatch::Verified(e)),
            None => self.finish(
                op,
                Dispatch::Uncertain("no external evidence yet; nothing was resent".into()),
            ),
        }
    }
    pub fn receipt(&self, grant: &Grant, id: &str, now: u64) -> Result<Operation> {
        let op = self.get(id)?;
        Self::owns(grant, &op, now)?;
        Ok(op)
    }
    /// All unresolved attempts owned by this account, including records older
    /// than the visible history. A fresh consent may look up, never re-send.
    pub fn pending(&self, grant: &Grant, now: u64) -> Result<Vec<Operation>> {
        let permission = grant
            .permissions
            .iter()
            .next()
            .ok_or(Error::Authorization)?;
        grant.check(permission, now)?;
        let mut statement=self.db.prepare("SELECT id FROM operations WHERE app=?1 AND account=?2 AND status IN ('dispatching','unknown') ORDER BY rowid")?;
        let ids = statement
            .query_map(params![grant.app, grant.account], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ids.into_iter()
            .map(|id| self.receipt(grant, &id, now))
            .collect()
    }
    pub fn recent(&self, grant: &Grant, now: u64) -> Result<Vec<Operation>> {
        // An empty journal must not turn an expired or revoked grant into a valid read.
        let permission = grant
            .permissions
            .iter()
            .next()
            .ok_or(Error::Authorization)?;
        grant.check(permission, now)?;
        let mut statement = self.db.prepare(
            "SELECT id FROM operations WHERE app=?1 AND account=?2 ORDER BY rowid DESC LIMIT 40",
        )?;
        let ids = statement
            .query_map(params![grant.app, grant.account], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ids.into_iter()
            .map(|id| self.receipt(grant, &id, now))
            .collect()
    }
    /// Task recovery is independent of the forty-row display history. Every
    /// referenced receipt is still checked against the host's current grant.
    pub fn recent_with_references(
        &self,
        grant: &Grant,
        references: &[String],
        now: u64,
    ) -> Result<Vec<Operation>> {
        let mut operations = self.recent(grant, now)?;
        let mut seen = operations
            .iter()
            .map(|op| op.id.clone())
            .collect::<BTreeSet<_>>();
        for id in references {
            if seen.insert(id.clone()) {
                operations.push(self.receipt(grant, id, now)?);
            }
        }
        Ok(operations)
    }
}
