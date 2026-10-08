//! Same-user polkit authentication; never launches a privileged command.
use super::{Cancel, PlatformCompletion, WindowTarget};
use dbus::{
    arg::{PropMap, Variant},
    blocking::SyncConnection,
};
use std::{
    collections::HashMap,
    os::unix::fs::MetadataExt,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

const ACTION: &str = "org.octosense.approve-business-action";
const POLICY: &str = "/usr/share/polkit-1/actions/org.octosense.policy";
const POLICY_BYTES: &[u8] = include_bytes!("../../../../desktop/resources/org.octosense.policy");

fn check_policy(path: &Path) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| {
        "OS approval is unavailable: install OctoSense's administrator-owned polkit policy"
    })?;
    if !metadata.is_file() || metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
        return Err(
            "OS approval policy must be a root-owned regular file without group/other write access"
                .into(),
        );
    }
    if std::fs::read(path).map_err(|_| "Cannot read OS approval policy")? != POLICY_BYTES {
        return Err("OS approval policy differs from this build's reviewed policy".into());
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct Process {
    pid: u32,
    start: u64,
    uid: u32,
}
impl Process {
    fn current() -> Result<Self, String> {
        let stat = std::fs::read_to_string("/proc/self/stat")
            .map_err(|_| "Cannot identify this native process")?;
        let start = stat
            .rsplit_once(')')
            .ok_or("Invalid native process identity")?
            .1
            .split_whitespace()
            .nth(19)
            .ok_or("Missing process start time")?
            .parse()
            .map_err(|_| "Invalid process start time")?;
        let status = std::fs::read_to_string("/proc/self/status")
            .map_err(|_| "Cannot identify this native user")?;
        let uid = status
            .lines()
            .find_map(|line| line.strip_prefix("Uid:"))
            .ok_or("Missing native user identity")?
            .split_whitespace()
            .map(str::parse::<u32>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "Invalid native user identity")?;
        if uid.len() != 4 || uid.iter().any(|value| *value != uid[0]) {
            return Err("OS approval is unavailable for a changed-identity process".into());
        }
        Ok(Self {
            pid: std::process::id(),
            start,
            uid: uid[0],
        })
    }
    fn subject(self) -> (&'static str, PropMap) {
        let mut values = PropMap::new();
        values.insert("pid".into(), Variant(Box::new(self.pid)));
        values.insert("start-time".into(), Variant(Box::new(self.start)));
        values.insert("uid".into(), Variant(Box::new(self.uid)));
        ("unix-process", values)
    }
}

type Check = (bool, bool, HashMap<String, String>);
fn temporary(details: &HashMap<String, String>) -> bool {
    details.keys().any(|key| {
        matches!(
            key.as_str(),
            "polkit.temporary_authorization_id" | "polkit.retains_authorization_after_challenge"
        )
    })
}
fn preflight(check: &Check) -> Result<(), String> {
    if check.0 || !check.1 || temporary(&check.2) {
        return Err("OS approval requires fresh authentication; cached or unavailable authorization is refused".into());
    }
    Ok(())
}
fn verified(check: &Check) -> Result<(), String> {
    if !check.0 || check.1 || temporary(&check.2) {
        return Err(
            "OS authentication was denied, cancelled, or retained authorization unexpectedly"
                .into(),
        );
    }
    Ok(())
}

pub(super) fn begin(
    _: WindowTarget,
    message: &str,
    completion: PlatformCompletion,
) -> Result<Cancel, String> {
    // File ownership/content checks are bounded and require no system mutation.
    check_policy(Path::new(POLICY))?;
    let process = Process::current()?;
    let connection: Arc<Mutex<Option<Arc<SyncConnection>>>> = Arc::new(Mutex::new(None));
    let worker_connection = connection.clone();
    let cancellation = completion.challenge_id();
    let worker_id = cancellation.clone();
    let worker = completion.clone();
    let mut details = HashMap::new();
    details.insert("polkit.message".to_owned(), message.to_owned());
    details.insert(
        "octosense.operation-sha256".to_owned(),
        completion.operation_digest(),
    );
    details.insert("octosense.challenge".to_owned(), cancellation.clone());
    std::thread::Builder::new()
        .name("native-os-authentication".into())
        .spawn(move || {
            let result = (|| {
                if worker.is_cancelled() {
                    return Err("Approval cancelled".into());
                }
                let connection = Arc::new(
                    SyncConnection::new_system()
                        .map_err(|_| "The OS authentication authority is unavailable")?,
                );
                *worker_connection.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some(connection.clone());
                if worker.is_cancelled() {
                    return Err("Approval cancelled".into());
                }
                let proxy = connection.with_proxy(
                    "org.freedesktop.PolicyKit1",
                    "/org/freedesktop/PolicyKit1/Authority",
                    Duration::from_secs(120),
                );
                let (check,): (Check,) = proxy
                    .method_call(
                        "org.freedesktop.PolicyKit1.Authority",
                        "CheckAuthorization",
                        (
                            process.subject(),
                            ACTION,
                            details.clone(),
                            0u32,
                            format!("{worker_id}-preflight"),
                        ),
                    )
                    .map_err(|_| "The OS authentication policy or agent is unavailable")?;
                preflight(&check)?;
                if worker.is_cancelled() {
                    return Err("Approval cancelled".into());
                }
                let (check,): (Check,) = proxy
                    .method_call(
                        "org.freedesktop.PolicyKit1.Authority",
                        "CheckAuthorization",
                        (process.subject(), ACTION, details, 1u32, worker_id),
                    )
                    .map_err(|_| "OS authentication was cancelled or unavailable")?;
                verified(&check)
            })();
            worker.finish(result);
        })
        .map_err(|_| "Cannot start OS authentication")?;
    Ok(Box::new(move || {
        let connection = connection.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some(connection) = connection {
            let _ = std::thread::Builder::new()
                .name("native-os-auth-cancel".into())
                .spawn(move || {
                    let proxy = connection.with_proxy(
                        "org.freedesktop.PolicyKit1",
                        "/org/freedesktop/PolicyKit1/Authority",
                        Duration::from_secs(5),
                    );
                    for id in [format!("{cancellation}-preflight"), cancellation] {
                        let _: Result<(), dbus::Error> = proxy.method_call(
                            "org.freedesktop.PolicyKit1.Authority",
                            "CancelCheckAuthorization",
                            (id,),
                        );
                    }
                });
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_denied_and_retained_authorization_are_refused() {
        assert!(preflight(&(false, true, HashMap::new())).is_ok());
        for pair in [(true, false), (true, true), (false, false)] {
            assert!(preflight(&(pair.0, pair.1, HashMap::new())).is_err());
        }
        assert!(verified(&(true, false, HashMap::new())).is_ok());
        for pair in [(false, false), (false, true), (true, true)] {
            assert!(verified(&(pair.0, pair.1, HashMap::new())).is_err());
        }
        let retained =
            HashMap::from([("polkit.temporary_authorization_id".into(), "token".into())]);
        assert!(preflight(&(false, true, retained.clone())).is_err());
        assert!(verified(&(true, false, retained)).is_err());
    }
    #[test]
    fn missing_policy_does_not_offer_an_authentication_bypass() {
        assert!(check_policy(Path::new("/nonexistent/octosense-approval-policy")).is_err());
        let process = Process::current().unwrap();
        assert_eq!(process.pid, std::process::id());
        assert!(process.start > 0);
    }

    /// Opt-in native negative acceptance. It must never run on an installed
    /// system with a real policy, and it never opens an authentication prompt.
    #[test]
    #[ignore = "requires a test host without the installed OctoSense polkit policy"]
    fn native_missing_policy_cannot_start_approval() {
        assert!(!Path::new(POLICY).exists(), "use an isolated test host");
        let state = Arc::new(Mutex::new(super::super::State::Waiting));
        let completion = PlatformCompletion {
            state: state.clone(),
            nonce: uuid::Uuid::new_v4(),
            digest: [0; 32],
        };
        let outcome = begin(
            WindowTarget {
                slot: 1,
                generation: 1,
                native_handle: 0,
            },
            "Synthetic acceptance: do not authorize a business operation",
            completion,
        );
        assert!(outcome.is_err());
        assert!(matches!(
            *state.lock().unwrap(),
            super::super::State::Waiting
        ));
    }
}
