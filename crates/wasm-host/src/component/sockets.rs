//! WASI sockets use the device's network. Declarations are disclosures, not
//! network grants. The host bounds resources and every blocking wait instead.
use super::{super::InvocationGuard, State};
use std::collections::HashMap;
use std::future::Future;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use wasmtime::component::{Linker, Resource, ResourceType};
use wasmtime::{StoreContextMut, Trap};
use wasmtime_wasi::p2::bindings::{
    io::{poll, streams},
    sockets::{network, tcp, tcp_create_socket, udp, udp_create_socket},
};
use wasmtime_wasi::p2::{
    DynInputStream, DynOutputStream, DynPollable, SocketError, StreamError, TcpSocket, UdpSocket,
};
use wasmtime_wasi::runtime::in_tokio;
use wasmtime_wasi::sockets::WasiSocketsView;

/// Bounds pollables, streams and all other host resources in one component.
pub(crate) const MAX_RESOURCES: usize = 512;
/// Live TCP/UDP sockets and DNS lookup streams across one app's components.
const MAX_SOCKETS: usize = 32;
const TICK: Duration = Duration::from_millis(10);

/// Share one budget between every component belonging to an app. A resource's
/// permit follows its real host handle; closing it, trapping or dropping the
/// instance releases the permit. Different apps have independent budgets.
#[derive(Clone, Debug, Default)]
pub struct SocketBudget(Arc<AtomicUsize>);
impl SocketBudget {
    pub(super) fn acquire(&self) -> Option<Permit> {
        self.0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < MAX_SOCKETS).then_some(n + 1)
            })
            .ok()
            .map(|_| Permit(self.clone()))
    }
}
pub(super) struct Permit(SocketBudget);
impl Drop for Permit {
    fn drop(&mut self) {
        self.0 .0.fetch_sub(1, Ordering::AcqRel);
    }
}

pub(crate) struct Resources {
    pub(super) budget: SocketBudget,
    live: HashMap<u32, Permit>,
}
impl Resources {
    pub(crate) fn new(budget: SocketBudget) -> Self {
        Self {
            budget,
            live: HashMap::new(),
        }
    }
    pub(crate) fn clear(&mut self) {
        self.live.clear();
    }
}

/// Epoch interrupts only run while guest code executes. Polling a socket or
/// DNS task waits in the host instead, so observe cancellation here too.
fn wait<F: Future>(guard: Option<Arc<InvocationGuard>>, future: F) -> wasmtime::Result<F::Output> {
    let guard = guard.ok_or_else(|| wasmtime::Error::from(Trap::Interrupt))?;
    if !guard.live() {
        return Err(Trap::Interrupt.into());
    }
    in_tokio(async {
        tokio::pin!(future);
        loop {
            let remaining = guard.deadline.saturating_duration_since(Instant::now());
            tokio::select! {
                biased;
                _ = tokio::time::sleep(TICK.min(remaining)) => {
                    if !guard.live() { return Err(Trap::Interrupt.into()); }
                }
                result = &mut future => {
                    return if guard.live() { Ok(result) } else { Err(Trap::Interrupt.into()) };
                }
            }
        }
    })
}

fn socket_result<T>(
    state: &mut State,
    result: Result<T, SocketError>,
) -> wasmtime::Result<(Result<T, network::ErrorCode>,)> {
    use network::Host;
    Ok((match result {
        Ok(v) => Ok(v),
        Err(e) => Err(state.sockets().convert_error_code(e)?),
    },))
}
fn stream_result<T>(
    state: &mut State,
    result: Result<T, StreamError>,
) -> wasmtime::Result<(Result<T, streams::StreamError>,)> {
    use streams::Host;
    Ok((match result {
        Ok(v) => Ok(v),
        Err(e) => Err(state.table.convert_stream_error(e)?),
    },))
}
fn full<T>() -> wasmtime::Result<(Result<T, network::ErrorCode>,)> {
    Ok((Err(network::ErrorCode::NewSocketLimit),))
}

pub(crate) fn link(linker: &mut Linker<State>) -> wasmtime::Result<()> {
    linker.allow_shadowing(true);
    linker
        .instance("wasi:sockets/tcp-create-socket@0.2.12")?
        .func_wrap(
            "create-tcp-socket",
            |mut store: StoreContextMut<'_, State>, (family,): (network::IpAddressFamily,)| {
                use tcp_create_socket::Host;
                let state = store.data_mut();
                let Some(permit) = state.sockets.budget.acquire() else {
                    return full();
                };
                let result = state.sockets().create_tcp_socket(family);
                if let Ok(socket) = &result {
                    state.sockets.live.insert(socket.rep(), permit);
                }
                socket_result(state, result)
            },
        )?;
    linker
        .instance("wasi:sockets/udp-create-socket@0.2.12")?
        .func_wrap(
            "create-udp-socket",
            |mut store: StoreContextMut<'_, State>, (family,): (network::IpAddressFamily,)| {
                use udp_create_socket::Host;
                let state = store.data_mut();
                let Some(permit) = state.sockets.budget.acquire() else {
                    return full();
                };
                let guard = state.guard.clone();
                let result = wait(guard, state.sockets().create_udp_socket(family))?;
                if let Ok(socket) = &result {
                    state.sockets.live.insert(socket.rep(), permit);
                }
                socket_result(state, result)
            },
        )?;
    let mut types = linker.instance("wasi:sockets/tcp@0.2.12")?;
    types.resource(
        "tcp-socket",
        ResourceType::host::<TcpSocket>(),
        |mut store, rep| {
            use tcp::HostTcpSocket;
            let state = store.data_mut();
            state.sockets().drop(Resource::new_own(rep))?;
            state.sockets.live.remove(&rep);
            Ok(())
        },
    )?;
    types.func_wrap(
        "[method]tcp-socket.accept",
        |mut store: StoreContextMut<'_, State>, (socket,): (Resource<TcpSocket>,)| {
            use tcp::HostTcpSocket;
            let state = store.data_mut();
            let Some(permit) = state.sockets.budget.acquire() else {
                return full();
            };
            let result = state.sockets().accept(socket);
            if let Ok((socket, _, _)) = &result {
                state.sockets.live.insert(socket.rep(), permit);
            }
            socket_result(state, result)
        },
    )?;
    types.func_wrap(
        "[method]tcp-socket.start-bind",
        |mut store: StoreContextMut<'_, State>,
         (socket, net, addr): (
            Resource<TcpSocket>,
            Resource<network::Network>,
            network::IpSocketAddress,
        )| {
            use tcp::HostTcpSocket;
            let state = store.data_mut();
            let result = wait(
                state.guard.clone(),
                state.sockets().start_bind(socket, net, addr),
            )?;
            socket_result(state, result)
        },
    )?;
    types.func_wrap(
        "[method]tcp-socket.start-listen",
        |mut store: StoreContextMut<'_, State>, (socket,): (Resource<TcpSocket>,)| {
            use tcp::HostTcpSocket;
            let state = store.data_mut();
            let result = wait(state.guard.clone(), state.sockets().start_listen(socket))?;
            socket_result(state, result)
        },
    )?;
    let mut types = linker.instance("wasi:sockets/udp@0.2.12")?;
    types.resource(
        "incoming-datagram-stream",
        ResourceType::host::<udp::IncomingDatagramStream>(),
        |mut store, rep| {
            use udp::HostIncomingDatagramStream;
            let state = store.data_mut();
            wait(
                state.guard.clone(),
                state.sockets().drop(Resource::new_own(rep)),
            )??;
            Ok(())
        },
    )?;
    types.resource(
        "outgoing-datagram-stream",
        ResourceType::host::<udp::OutgoingDatagramStream>(),
        |mut store, rep| {
            use udp::HostOutgoingDatagramStream;
            let state = store.data_mut();
            wait(
                state.guard.clone(),
                state.sockets().drop(Resource::new_own(rep)),
            )??;
            Ok(())
        },
    )?;
    types.resource(
        "udp-socket",
        ResourceType::host::<UdpSocket>(),
        |mut store, rep| {
            use udp::HostUdpSocket;
            let state = store.data_mut();
            state.sockets().drop(Resource::new_own(rep))?;
            state.sockets.live.remove(&rep);
            Ok(())
        },
    )?;
    types.func_wrap(
        "[method]udp-socket.start-bind",
        |mut store: StoreContextMut<'_, State>,
         (socket, net, addr): (
            Resource<UdpSocket>,
            Resource<network::Network>,
            network::IpSocketAddress,
        )| {
            use udp::HostUdpSocket;
            let state = store.data_mut();
            let result = wait(
                state.guard.clone(),
                state.sockets().start_bind(socket, net, addr),
            )?;
            socket_result(state, result)
        },
    )?;
    types.func_wrap(
        "[method]udp-socket.stream",
        |mut store: StoreContextMut<'_, State>,
         (socket, addr): (Resource<UdpSocket>, Option<network::IpSocketAddress>)| {
            use udp::HostUdpSocket;
            let state = store.data_mut();
            let result = wait(state.guard.clone(), state.sockets().stream(socket, addr))?;
            socket_result(state, result)
        },
    )?;
    link_waits(linker)
}

/// Cover all blocking stream operations, including streams produced by HTTP
/// and TCP and pollables produced by DNS, UDP, clocks and TCP connect/listen.
fn link_waits(linker: &mut Linker<State>) -> wasmtime::Result<()> {
    let mut types = linker.instance("wasi:io/poll@0.2.12")?;
    types.func_wrap(
        "poll",
        |mut store: StoreContextMut<'_, State>, (items,): (Vec<Resource<DynPollable>>,)| {
            use poll::Host;
            let state = store.data_mut();
            Ok((wait(state.guard.clone(), state.table.poll(items))??,))
        },
    )?;
    types.func_wrap(
        "[method]pollable.block",
        |mut store: StoreContextMut<'_, State>, (item,): (Resource<DynPollable>,)| {
            use poll::HostPollable;
            let state = store.data_mut();
            wait(state.guard.clone(), state.table.block(item))??;
            Ok(())
        },
    )?;
    let mut types = linker.instance("wasi:io/streams@0.2.12")?;
    types.resource(
        "input-stream",
        ResourceType::host::<DynInputStream>(),
        |mut store, rep| {
            use streams::HostInputStream;
            let state = store.data_mut();
            wait(
                state.guard.clone(),
                state.table.drop(Resource::new_own(rep)),
            )??;
            Ok(())
        },
    )?;
    types.resource(
        "output-stream",
        ResourceType::host::<DynOutputStream>(),
        |mut store, rep| {
            use streams::HostOutputStream;
            let state = store.data_mut();
            wait(
                state.guard.clone(),
                state.table.drop(Resource::new_own(rep)),
            )??;
            Ok(())
        },
    )?;
    macro_rules! input {
        ($name:literal, $method:ident) => {
            types.func_wrap(
                $name,
                |mut store: StoreContextMut<'_, State>,
                 (stream, len): (Resource<DynInputStream>, u64)| {
                    use streams::HostInputStream;
                    let state = store.data_mut();
                    let result = wait(state.guard.clone(), state.table.$method(stream, len))?;
                    stream_result(state, result)
                },
            )?;
        };
    }
    input!("[method]input-stream.blocking-read", blocking_read);
    input!("[method]input-stream.blocking-skip", blocking_skip);
    types.func_wrap(
        "[method]output-stream.blocking-write-and-flush",
        |mut store: StoreContextMut<'_, State>,
         (stream, bytes): (Resource<DynOutputStream>, Vec<u8>)| {
            use streams::HostOutputStream;
            let state = store.data_mut();
            let result = wait(
                state.guard.clone(),
                state.table.blocking_write_and_flush(stream, bytes),
            )?;
            stream_result(state, result)
        },
    )?;
    types.func_wrap(
        "[method]output-stream.blocking-write-zeroes-and-flush",
        |mut store: StoreContextMut<'_, State>, (stream, len): (Resource<DynOutputStream>, u64)| {
            use streams::HostOutputStream;
            let state = store.data_mut();
            let result = wait(
                state.guard.clone(),
                state.table.blocking_write_zeroes_and_flush(stream, len),
            )?;
            stream_result(state, result)
        },
    )?;
    types.func_wrap(
        "[method]output-stream.blocking-flush",
        |mut store: StoreContextMut<'_, State>, (stream,): (Resource<DynOutputStream>,)| {
            use streams::HostOutputStream;
            let state = store.data_mut();
            let result = wait(state.guard.clone(), state.table.blocking_flush(stream))?;
            stream_result(state, result)
        },
    )?;
    types.func_wrap(
        "[method]output-stream.blocking-splice",
        |mut store: StoreContextMut<'_, State>,
         (out, input, len): (Resource<DynOutputStream>, Resource<DynInputStream>, u64)| {
            use streams::HostOutputStream;
            let state = store.data_mut();
            let result = wait(
                state.guard.clone(),
                state.table.blocking_splice(out, input, len),
            )?;
            stream_result(state, result)
        },
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn cancellation_ends_a_host_future_that_never_wakes() {
        // DNS, poll and streams all use this wait. No external DNS server or
        // platform resolver timing is needed to prove the cancellation path.
        let pending = Arc::new(AtomicBool::new(true));
        let flag = pending.clone();
        let cancel = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            flag.store(false, Ordering::Release);
        });
        let start = Instant::now();
        let guard = Arc::new(InvocationGuard {
            deadline: start + Duration::from_secs(5),
            pending: Box::new(move || pending.load(Ordering::Acquire)),
        });
        let error = wait(Some(guard), std::future::pending::<()>()).unwrap_err();
        assert!(matches!(
            error.downcast_ref::<Trap>(),
            Some(Trap::Interrupt)
        ));
        assert!(start.elapsed() < Duration::from_secs(1));
        cancel.join().unwrap();
    }

    #[test]
    fn deadline_ends_a_host_future_that_never_wakes() {
        let start = Instant::now();
        let guard = Arc::new(InvocationGuard {
            deadline: start + Duration::from_millis(30),
            pending: Box::new(|| true),
        });
        let error = wait(Some(guard), std::future::pending::<()>()).unwrap_err();
        assert!(matches!(
            error.downcast_ref::<Trap>(),
            Some(Trap::Interrupt)
        ));
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    struct StalledClose(Arc<AtomicBool>);
    impl Drop for StalledClose {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }
    #[wasmtime_wasi::async_trait]
    impl wasmtime_wasi::p2::Pollable for StalledClose {
        async fn ready(&mut self) {}
    }
    #[wasmtime_wasi::async_trait]
    impl wasmtime_wasi::p2::InputStream for StalledClose {
        fn read(&mut self, _: usize) -> Result<bytes::Bytes, StreamError> {
            Err(StreamError::Closed)
        }
        async fn cancel(&mut self) {
            std::future::pending::<()>().await;
        }
    }

    #[test]
    fn a_stream_destructor_cannot_outlive_the_call() {
        use streams::HostInputStream;
        let closed = Arc::new(AtomicBool::new(false));
        let mut table = wasmtime::component::ResourceTable::new();
        let stream = table
            .push(Box::new(StalledClose(closed.clone())) as DynInputStream)
            .unwrap();
        let guard = Arc::new(InvocationGuard {
            deadline: Instant::now() + Duration::from_millis(30),
            pending: Box::new(|| true),
        });
        let error = wait(Some(guard), table.drop(stream)).unwrap_err();
        assert!(matches!(
            error.downcast_ref::<Trap>(),
            Some(Trap::Interrupt)
        ));
        assert!(closed.load(Ordering::Acquire));
    }
}
