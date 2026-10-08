//! Native OS-authenticated approval is distinct from physical pointer input.
//! Nothing in this module is registered as a script or agent host method.
use makepad_widgets::{Cx, SignalToUI, WindowId};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

type Admission = Arc<dyn Fn(&Path, &str, &str) -> Result<String, String> + Send + Sync>;
fn admission() -> &'static Mutex<Option<Admission>> {
    static CHECK: OnceLock<Mutex<Option<Admission>>> = OnceLock::new();
    CHECK.get_or_init(|| Mutex::new(None))
}

/// Native shell callback must re-verify the admitted bundle and grant on each
/// call, returning its exact digest. Missing registration fails closed.
pub fn register_admission(
    check: impl Fn(&Path, &str, &str) -> Result<String, String> + Send + Sync + 'static,
) {
    *admission().lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(check));
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Binding {
    root: PathBuf,
    app: String,
    family: String,
    bundle: String,
    connection: String,
    account_epoch: u64,
    review: String,
    digest: [u8; 32],
}
impl Binding {
    pub(crate) fn capture(
        root: &Path,
        app: &str,
        family: &str,
        connection: &str,
        review: &str,
        snapshot: &Value,
    ) -> Result<Self, String> {
        let check = admission()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or("OS approval is unavailable: this host has no admission verifier")?;
        let bundle = check(root, app, family)?;
        if bundle.is_empty() {
            return Err("The app's current admitted identity is unavailable".into());
        }
        let current = crate::host::active_connection(root, app)
            .ok_or("The reviewed account is no longer connected")?;
        if current.handle != connection {
            return Err("The active account changed; review again".into());
        }
        Ok(Self {
            root: root.to_owned(),
            app: app.to_owned(),
            family: family.to_owned(),
            bundle,
            connection: connection.to_owned(),
            account_epoch: crate::host::authorization_epoch(root, app),
            review: review.to_owned(),
            digest: canonical_digest(snapshot)?,
        })
    }
    pub(crate) fn revalidate(&self) -> Result<(), String> {
        let check = admission()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or("Native admission verifier is unavailable")?;
        if check(&self.root, &self.app, &self.family)? != self.bundle
            || crate::host::authorization_epoch(&self.root, &self.app) != self.account_epoch
            || !crate::host::active_connection(&self.root, &self.app)
                .is_some_and(|account| account.handle == self.connection)
        {
            return Err("The reviewed app or account changed; review again".into());
        }
        Ok(())
    }
}

pub(crate) fn canonical_digest(value: &Value) -> Result<[u8; 32], String> {
    fn ordered(value: &Value, depth: usize) -> Result<Value, String> {
        if depth > 32 {
            return Err("Review data is too deeply nested".into());
        }
        Ok(match value {
            Value::Object(map) => {
                let sorted: std::collections::BTreeMap<_, _> = map.iter().collect();
                Value::Object(
                    sorted
                        .into_iter()
                        .map(|(k, v)| ordered(v, depth + 1).map(|v| (k.clone(), v)))
                        .collect::<Result<_, _>>()?,
                )
            }
            Value::Array(values) => Value::Array(
                values
                    .iter()
                    .map(|v| ordered(v, depth + 1))
                    .collect::<Result<_, _>>()?,
            ),
            other => other.clone(),
        })
    }
    let bytes = serde_json::to_vec(&ordered(value, 0)?)
        .map_err(|_| "Cannot fingerprint the reviewed operation")?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("Reviewed operation is too large".into());
    }
    Ok(Sha256::digest(bytes).into())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct WindowTarget {
    pub slot: usize,
    pub generation: u64,
    pub native_handle: usize,
}
fn target(cx: &Cx, id: WindowId) -> Result<WindowTarget, String> {
    if !cx.windows.is_valid(id) || !cx.windows[id].is_created {
        return Err("The review window is no longer available".into());
    }
    #[cfg(target_os = "windows")]
    let native_handle = windows::native_handle(cx, id)?;
    #[cfg(not(target_os = "windows"))]
    let native_handle = 0;
    Ok(WindowTarget {
        slot: id.0,
        generation: id.1,
        native_handle,
    })
}

enum State {
    Waiting,
    Finished(Result<(), String>),
    Claimed,
    Cancelled,
}
#[derive(Clone)]
pub(super) struct PlatformCompletion {
    state: Arc<Mutex<State>>,
    nonce: uuid::Uuid,
    digest: [u8; 32],
}
impl PlatformCompletion {
    pub fn challenge_id(&self) -> String {
        self.nonce.to_string()
    }
    pub fn operation_digest(&self) -> String {
        self.digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
    pub fn is_cancelled(&self) -> bool {
        !matches!(
            *self.state.lock().unwrap_or_else(|e| e.into_inner()),
            State::Waiting
        )
    }
    /// Platform adapters call this only after an actual OS result. Neither the
    /// message text nor a script/tool response is evidence of authentication.
    pub fn finish(&self, outcome: Result<(), String>) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(*state, State::Waiting) {
            *state = State::Finished(outcome);
            SignalToUI::set_ui_signal();
        }
    }
}
pub(super) type Cancel = Box<dyn FnOnce() + Send>;

fn platform_begin(
    window: WindowTarget,
    message: &str,
    completion: PlatformCompletion,
) -> Result<Cancel, String> {
    #[cfg(target_os = "linux")]
    {
        linux::begin(window, message, completion)
    }
    #[cfg(target_os = "windows")]
    {
        windows::begin(window, message, completion)
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = (window, message, completion);
        Err("Use physical approval on this platform".into())
    }
}

/// Held only by the owning native review. Dropping it cancels the OS request;
/// a late callback cannot authorize another review or a reused native window.
pub struct PendingApproval {
    nonce: uuid::Uuid,
    binding: Binding,
    owner: String,
    window: WindowTarget,
    deadline: Instant,
    state: Arc<Mutex<State>>,
    cancel: Option<Cancel>,
}
impl PendingApproval {
    pub(crate) fn begin(
        cx: &Cx,
        id: WindowId,
        owner: &str,
        binding: Binding,
        message: &str,
    ) -> Result<Self, String> {
        if owner.is_empty() || message.is_empty() || message.len() > 160 {
            return Err("Invalid native approval owner or message".into());
        }
        let window = target(cx, id)?;
        let nonce = uuid::Uuid::new_v4();
        let state = Arc::new(Mutex::new(State::Waiting));
        let completion = PlatformCompletion {
            state: state.clone(),
            nonce,
            digest: binding.digest,
        };
        let cancel = platform_begin(window, message, completion)?;
        Ok(Self {
            nonce,
            binding,
            owner: owner.to_owned(),
            window,
            deadline: Instant::now() + Duration::from_secs(120),
            state,
            cancel: Some(cancel),
        })
    }

    pub(crate) fn poll(
        &mut self,
        cx: &Cx,
        id: WindowId,
        owner: &str,
        binding: &Binding,
    ) -> Result<Option<AuthenticatedReview>, String> {
        let window = match target(cx, id) {
            Ok(window) => window,
            Err(error) => {
                self.cancel();
                return Err(error);
            }
        };
        self.validate_context(window, owner, binding, Instant::now())?;
        #[cfg(target_os = "windows")]
        windows::pump(&self.nonce.to_string());
        self.claim(window, owner, binding, Instant::now())
    }

    fn validate_context(
        &mut self,
        window: WindowTarget,
        owner: &str,
        binding: &Binding,
        now: Instant,
    ) -> Result<(), String> {
        if now >= self.deadline
            || self.window != window
            || self.owner != owner
            || &self.binding != binding
        {
            self.cancel();
            return Err("The reviewed operation, account or window changed; review again".into());
        }
        Ok(())
    }

    fn claim(
        &mut self,
        window: WindowTarget,
        owner: &str,
        binding: &Binding,
        now: Instant,
    ) -> Result<Option<AuthenticatedReview>, String> {
        self.validate_context(window, owner, binding, now)?;
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        match &*state {
            State::Waiting => Ok(None),
            State::Finished(Ok(())) => {
                *state = State::Claimed;
                Ok(Some(AuthenticatedReview {
                    nonce: self.nonce,
                    binding: self.binding.clone(),
                    deadline: self.deadline,
                }))
            }
            State::Finished(Err(error)) => {
                let error = error.clone();
                *state = State::Cancelled;
                Err(error)
            }
            State::Claimed | State::Cancelled => {
                Err("This authentication request was already consumed or cancelled".into())
            }
        }
    }

    pub fn cancel(&mut self) {
        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if !matches!(*state, State::Claimed) {
                *state = State::Cancelled;
            }
        }
        if let Some(cancel) = self.cancel.take() {
            cancel();
        }
    }
}
impl Drop for PendingApproval {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Private, non-cloneable and non-serializable evidence. Consuming it checks
/// the immutable operation again immediately before its existing one-shot save.
pub(crate) struct AuthenticatedReview {
    nonce: uuid::Uuid,
    binding: Binding,
    deadline: Instant,
}
impl AuthenticatedReview {
    pub(crate) fn consume(self, binding: &Binding) -> Result<ApprovedOperation, String> {
        if self.nonce.is_nil() || &self.binding != binding {
            return Err("Authentication belongs to a different reviewed operation".into());
        }
        Ok(ApprovedOperation {
            binding: self.binding,
            deadline: self.deadline,
        })
    }
}
pub(crate) struct ApprovedOperation {
    binding: Binding,
    deadline: Instant,
}
impl ApprovedOperation {
    /// Preserve the earlier of the authentication and original review limits.
    pub(crate) fn limit_to(mut self, deadline: Instant) -> Self {
        self.deadline = self.deadline.min(deadline);
        self
    }
    pub(crate) fn revalidate(self) -> Result<(), String> {
        self.revalidate_at(Instant::now())
    }
    fn revalidate_at(&self, now: Instant) -> Result<(), String> {
        if now >= self.deadline {
            return Err("OS approval expired before execution; review again".into());
        }
        self.binding.revalidate()
    }
    pub(crate) fn validate(
        self,
        app: &str,
        connection: &str,
        snapshot: &Value,
    ) -> Result<(), String> {
        self.revalidate_at(Instant::now())?;
        if self.binding.app != app
            || self.binding.connection != connection
            || self.binding.digest != canonical_digest(snapshot)?
        {
            return Err("OS approval belongs to a different reply snapshot".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pending() -> (PendingApproval, PlatformCompletion) {
        let state = Arc::new(Mutex::new(State::Waiting));
        let binding = Binding {
            root: PathBuf::from("fixture"),
            app: "example.app".into(),
            family: "github".into(),
            bundle: "digest".into(),
            connection: "synthetic".into(),
            account_epoch: 1,
            review: "review".into(),
            digest: canonical_digest(&serde_json::json!({"body":"reviewed"})).unwrap(),
        };
        let window = WindowTarget {
            slot: 1,
            generation: 2,
            native_handle: 3,
        };
        (
            PendingApproval {
                nonce: uuid::Uuid::new_v4(),
                binding,
                owner: "host-sheet".into(),
                window,
                deadline: Instant::now() + Duration::from_secs(60),
                state: state.clone(),
                cancel: None,
            },
            PlatformCompletion {
                state,
                nonce: uuid::Uuid::new_v4(),
                digest: [0; 32],
            },
        )
    }
    #[test]
    fn os_result_is_required_and_one_shot() {
        let (mut p, done) = pending();
        let binding = p.binding.clone();
        assert!(p
            .claim(p.window, "host-sheet", &binding, Instant::now())
            .unwrap()
            .is_none());
        done.finish(Ok(()));
        let proof = p
            .claim(p.window, "host-sheet", &binding, Instant::now())
            .unwrap()
            .unwrap();
        proof.consume(&binding).unwrap();
        assert!(p
            .claim(p.window, "host-sheet", &binding, Instant::now())
            .is_err());
    }
    #[test]
    fn queued_approval_rechecks_auth_and_review_expiry_after_execution_lock() {
        for expire_review_first in [false, true] {
            let (mut p, done) = pending();
            let binding = p.binding.clone();
            let now = Instant::now();
            done.finish(Ok(()));
            let review_deadline = if expire_review_first {
                now + Duration::from_secs(1)
            } else {
                p.deadline + Duration::from_secs(1)
            };
            let expected_deadline = p.deadline.min(review_deadline);
            let approved = p
                .claim(p.window, "host-sheet", &binding, now)
                .unwrap()
                .unwrap()
                .consume(&binding)
                .unwrap()
                .limit_to(review_deadline);
            assert_eq!(approved.deadline, expected_deadline);
            let clock = Arc::new(Mutex::new(now));
            let worker_clock = clock.clone();
            let root =
                std::env::temp_dir().join(format!("approval-expiry-{}", uuid::Uuid::new_v4()));
            let app = "org.example.expiry";
            let lock = crate::host::operation_lock(&root, app);
            let held = lock.lock().unwrap();
            let (checked_tx, checked_rx) = std::sync::mpsc::channel();
            let worker = std::thread::spawn(move || {
                crate::host::with_provider_api_checked(
                    &root,
                    app,
                    || {
                        checked_tx.send(()).unwrap();
                        approved.revalidate_at(*worker_clock.lock().unwrap())
                    },
                    |_| -> Result<(), String> {
                        panic!("Expired OS approval must not reach a provider")
                    },
                )
            });
            let join_deadline = Instant::now() + Duration::from_secs(5);
            while Arc::strong_count(&lock) < 2 {
                assert!(Instant::now() < join_deadline);
                std::thread::yield_now();
            }
            assert!(matches!(
                checked_rx.try_recv(),
                Err(std::sync::mpsc::TryRecvError::Empty)
            ));
            *clock.lock().unwrap() = expected_deadline;
            drop(held);
            assert_eq!(
                worker.join().unwrap().unwrap_err(),
                "OS approval expired before execution; review again"
            );
            checked_rx.recv().unwrap();
        }
    }
    #[test]
    fn changed_operation_account_bundle_and_owner_deny() {
        for field in 0..6 {
            let (mut p, done) = pending();
            let mut binding = p.binding.clone();
            let mut window = p.window;
            let mut owner = "host-sheet";
            match field {
                0 => binding.digest[0] ^= 1,
                1 => binding.account_epoch += 1,
                2 => binding.connection.push('x'),
                3 => binding.bundle.push('x'),
                4 => window.generation += 1,
                _ => owner = "another-sheet",
            }
            done.finish(Ok(()));
            assert!(p.claim(window, owner, &binding, Instant::now()).is_err());
        }
    }
    #[test]
    fn cancelled_expired_and_failed_callbacks_never_authorize() {
        let (mut p, done) = pending();
        let binding = p.binding.clone();
        p.cancel();
        done.finish(Ok(()));
        assert!(p
            .claim(p.window, "host-sheet", &binding, Instant::now())
            .is_err());
        let (mut p, done) = pending();
        let binding = p.binding.clone();
        done.finish(Ok(()));
        assert!(p
            .claim(p.window, "host-sheet", &binding, p.deadline)
            .is_err());
        let (mut p, done) = pending();
        let binding = p.binding.clone();
        done.finish(Err("Denied".into()));
        assert!(p
            .claim(p.window, "host-sheet", &binding, Instant::now())
            .is_err());
    }
    #[test]
    fn digest_covers_destinations_content_and_resource_version() {
        let baseline =
            canonical_digest(&serde_json::json!({"to":"first","body":"one","version":"a"}))
                .unwrap();
        for value in [
            serde_json::json!({"to":"other","body":"one","version":"a"}),
            serde_json::json!({"to":"first","body":"two","version":"a"}),
            serde_json::json!({"to":"first","body":"one","version":"b"}),
        ] {
            assert_ne!(baseline, canonical_digest(&value).unwrap());
        }
    }
}
