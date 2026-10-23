//! WASI DNS with resolver work bounded until native resolution really ends.
//!
//! Cancelling a guest cannot cancel a running getaddrinfo call. Each resolver
//! therefore keeps both a process-wide worker permit and its app's socket
//! permit until it returns. Dropping a guest resource never opens an unbounded
//! queue of detached resolver tasks.
use super::sockets::Permit;
use super::State;
use std::collections::VecDeque;
use std::net::{IpAddr, ToSocketAddrs};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use tokio::sync::Notify;
use wasmtime::component::{Linker, Resource, ResourceType};
use wasmtime::{StoreContextMut, Trap};
use wasmtime_wasi::p2::bindings::sockets::network::{ErrorCode, IpAddress, Network};
use wasmtime_wasi::p2::{subscribe, DynPollable, Pollable};

const MAX_RESOLVERS: usize = 8;
const MAX_ADDRESSES: usize = 256;
static RESOLVERS: AtomicUsize = AtomicUsize::new(0);

type Answer = Result<VecDeque<IpAddr>, ErrorCode>;
struct Shared {
    answer: Mutex<Option<Answer>>,
    ready: Notify,
}
impl Shared {
    fn new(answer: Option<Answer>) -> Self {
        Self {
            answer: Mutex::new(answer),
            ready: Notify::new(),
        }
    }
    fn is_ready(&self) -> bool {
        self.answer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }
    fn complete(&self, answer: Answer) {
        *self.answer.lock().unwrap_or_else(|e| e.into_inner()) = Some(answer);
        self.ready.notify_waiters();
    }
}

/// Kept by the native worker, not just its guest-facing handle.
struct WorkerPermit;
impl WorkerPermit {
    fn acquire() -> Option<Self> {
        RESOLVERS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                (used < MAX_RESOLVERS).then_some(used + 1)
            })
            .ok()
            .map(|_| Self)
    }
}
impl Drop for WorkerPermit {
    fn drop(&mut self) {
        RESOLVERS.fetch_sub(1, Ordering::AcqRel);
    }
}

struct DnsLookup {
    shared: Arc<Shared>,
    // The worker holds another Arc. A finished lookup keeps its permit while
    // the resource lives; an abandoned lookup keeps it while its worker runs.
    _permit: Arc<Permit>,
}

#[wasmtime_wasi::async_trait]
impl Pollable for DnsLookup {
    async fn ready(&mut self) {
        loop {
            let notified = self.shared.ready.notified();
            tokio::pin!(notified);
            // Register before checking completion: no lost wakeup if the
            // resolver completes between the check and awaiting Notify.
            notified.as_mut().enable();
            if self.shared.is_ready() {
                return;
            }
            notified.await;
        }
    }
}

impl DnsLookup {
    fn literal(address: IpAddr, permit: Permit) -> Self {
        Self {
            shared: Arc::new(Shared::new(Some(Ok(VecDeque::from([address]))))),
            _permit: Arc::new(permit),
        }
    }

    fn start(
        permit: Permit,
        resolve: impl FnOnce() -> Answer + Send + 'static,
    ) -> Result<Self, ErrorCode> {
        let worker_permit = WorkerPermit::acquire().ok_or(ErrorCode::NewSocketLimit)?;
        let permit = Arc::new(permit);
        let shared = Arc::new(Shared::new(None));
        let worker_state = shared.clone();
        let worker_socket_permit = permit.clone();
        std::thread::Builder::new()
            .name("wasm-dns".into())
            .spawn(move || {
                let _worker_permit = worker_permit;
                let _socket_permit = worker_socket_permit;
                worker_state.complete(resolve());
            })
            .map_err(|_| ErrorCode::NewSocketLimit)?;
        Ok(Self {
            shared,
            _permit: permit,
        })
    }

    fn next(&mut self) -> Result<Option<IpAddress>, ErrorCode> {
        let mut answer = self.shared.answer.lock().unwrap_or_else(|e| e.into_inner());
        match answer.as_mut() {
            None => Err(ErrorCode::WouldBlock),
            Some(Err(error)) => Err(*error),
            Some(Ok(addresses)) => Ok(addresses.pop_front().map(Into::into)),
        }
    }
}

enum Name {
    Literal(IpAddr),
    Domain(String),
}

fn name(value: &str) -> Result<Name, ErrorCode> {
    if value.is_empty() || value.len() > 4096 || value.contains('\0') {
        return Err(ErrorCode::InvalidArgument);
    }
    // Preserve Wasmtime's IDNA and numeric-host handling, including bare IPv6.
    match url::Host::parse(value) {
        Ok(url::Host::Ipv4(address)) => Ok(Name::Literal(address.into())),
        Ok(url::Host::Ipv6(address)) => Ok(Name::Literal(address.into())),
        Ok(url::Host::Domain(domain)) if !domain.is_empty() && domain.len() <= 253 => {
            Ok(Name::Domain(domain))
        }
        Err(_) => value
            .parse::<std::net::Ipv6Addr>()
            .map(|address| Name::Literal(address.into()))
            .map_err(|_| ErrorCode::InvalidArgument),
        _ => Err(ErrorCode::InvalidArgument),
    }
}

fn resolve(domain: String) -> Answer {
    (domain.as_str(), 0)
        .to_socket_addrs()
        .map(|addresses| {
            addresses
                .map(|address| address.ip().to_canonical())
                .take(MAX_ADDRESSES)
                .collect()
        })
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::InvalidInput => ErrorCode::InvalidArgument,
            std::io::ErrorKind::PermissionDenied => ErrorCode::AccessDenied,
            std::io::ErrorKind::TimedOut
            | std::io::ErrorKind::WouldBlock
            | std::io::ErrorKind::Interrupted => ErrorCode::TemporaryResolverFailure,
            std::io::ErrorKind::OutOfMemory => ErrorCode::OutOfMemory,
            _ => ErrorCode::NameUnresolvable,
        })
}

pub(crate) fn link(linker: &mut Linker<State>) -> wasmtime::Result<()> {
    linker.allow_shadowing(true);
    let mut interface = linker.instance("wasi:sockets/ip-name-lookup@0.2.12")?;
    interface.resource(
        "resolve-address-stream",
        ResourceType::host::<DnsLookup>(),
        |mut store, rep| {
            // Unlike getaddrinfo, dropping the guest handle must never block.
            store
                .data_mut()
                .table
                .delete(Resource::<DnsLookup>::new_own(rep))?;
            Ok(())
        },
    )?;
    interface.func_wrap(
        "resolve-addresses",
        |mut store: StoreContextMut<'_, State>, (network, value): (Resource<Network>, String)| {
            let state = store.data_mut();
            state.table.get(&network)?;
            if !state.guard.as_ref().is_some_and(|guard| guard.live()) {
                return Err(Trap::Interrupt.into());
            }
            let name = match name(&value) {
                Ok(name) => name,
                Err(error) => return Ok((Err(error),)),
            };
            let Some(permit) = state.sockets.budget.acquire() else {
                return Ok((Err(ErrorCode::NewSocketLimit),));
            };
            let lookup = match name {
                Name::Literal(address) => DnsLookup::literal(address, permit),
                Name::Domain(domain) => match DnsLookup::start(permit, move || resolve(domain)) {
                    Ok(lookup) => lookup,
                    Err(error) => return Ok((Err(error),)),
                },
            };
            Ok((Ok(state.table.push(lookup)?),))
        },
    )?;
    interface.func_wrap(
        "[method]resolve-address-stream.resolve-next-address",
        |mut store: StoreContextMut<'_, State>, (lookup,): (Resource<DnsLookup>,)| {
            Ok((store.data_mut().table.get_mut(&lookup)?.next(),))
        },
    )?;
    interface.func_wrap(
        "[method]resolve-address-stream.subscribe",
        |mut store: StoreContextMut<'_, State>,
         (lookup,): (Resource<DnsLookup>,)|
         -> wasmtime::Result<(Resource<DynPollable>,)> {
            Ok((subscribe(&mut store.data_mut().table, lookup)?,))
        },
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::sockets::SocketBudget;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    #[test]
    fn resolver_work_stays_bounded_after_all_guest_handles_are_dropped() {
        let budget = SocketBudget::default();
        let mut controls = Vec::new();
        for _ in 0..MAX_RESOLVERS {
            let (release, wait) = mpsc::channel();
            let lookup = DnsLookup::start(budget.acquire().unwrap(), move || {
                wait.recv().unwrap();
                Ok(VecDeque::from([IpAddr::from([127, 0, 0, 1])]))
            })
            .unwrap();
            controls.push((release, lookup.shared.clone()));
            drop(lookup);
        }
        assert!(matches!(
            DnsLookup::start(budget.acquire().unwrap(), || unreachable!()),
            Err(ErrorCode::NewSocketLimit)
        ));
        // Eight abandoned native lookups still consume eight per-app permits.
        let mut held = Vec::new();
        while let Some(permit) = budget.acquire() {
            held.push(permit);
        }
        assert_eq!(held.len(), 32 - MAX_RESOLVERS);
        for (release, _) in &controls {
            release.send(()).unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while controls.iter().any(|(_, result)| !result.is_ready())
            || RESOLVERS.load(Ordering::Acquire) != 0
        {
            assert!(
                Instant::now() < deadline,
                "native resolver workers did not finish"
            );
            std::thread::yield_now();
        }
        let mut freed = Vec::new();
        while let Some(permit) = budget.acquire() {
            freed.push(permit);
        }
        assert_eq!(freed.len(), MAX_RESOLVERS);
        drop(held);
        drop(freed);
    }

    #[test]
    fn literal_lookup_is_ready_and_retains_the_app_permit_until_drop() {
        let budget = SocketBudget::default();
        let mut lookup =
            DnsLookup::literal(IpAddr::from([127, 0, 0, 1]), budget.acquire().unwrap());
        assert!(lookup.shared.is_ready());
        assert!(matches!(
            lookup.next(),
            Ok(Some(IpAddress::Ipv4((127, 0, 0, 1))))
        ));
        assert!(matches!(lookup.next(), Ok(None)));
        let mut permits = Vec::new();
        while let Some(permit) = budget.acquire() {
            permits.push(permit);
        }
        assert_eq!(permits.len(), 31);
        drop(lookup);
        assert!(budget.acquire().is_some());
    }

    #[test]
    fn host_names_reject_ports_and_invalid_input_and_preserve_idna_and_literals() {
        for value in [
            "",
            "bad\0host",
            "example.test:443",
            "https://example.test",
            "host name",
            "example.test/path",
        ] {
            assert!(name(value).is_err(), "{value:?}");
        }
        for value in ["127.0.0.1", "::1", "[::1]"] {
            assert!(matches!(name(value), Ok(Name::Literal(_))), "{value}");
        }
        assert!(
            matches!(name("bücher.example"), Ok(Name::Domain(domain)) if domain == "xn--bcher-kva.example")
        );
    }
}
