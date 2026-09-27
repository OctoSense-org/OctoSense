//! Bounded toast queue (depth 3). Used by the connection state machine and
//! ad-hoc UI surfaces. See `01-ARCHITECTURE.md` § 7 (Failure model) — toasts
//! ride alongside the reducer, not through the protocol.

use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Error,
    Reconnecting,
    ReconnectSuccess,
    Info,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toast {
    pub kind: ToastKind,
    pub message: String,
}

impl Toast {
    pub fn new(kind: ToastKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into() }
    }
}

#[derive(Clone, Debug)]
pub struct ToastQueue {
    pub items: VecDeque<Toast>,
    pub capacity: usize,
}

impl Default for ToastQueue { fn default() -> Self { Self::new(3) } }

impl ToastQueue {
    pub fn new(capacity: usize) -> Self {
        Self { items: VecDeque::with_capacity(capacity), capacity }
    }

    /// Push a new toast; evict the oldest when at capacity.
    pub fn push(&mut self, toast: Toast) {
        if self.items.len() == self.capacity { self.items.pop_front(); }
        self.items.push_back(toast);
    }

    pub fn dismiss_oldest(&mut self) -> Option<Toast> { self.items.pop_front() }
    pub fn len(&self) -> usize { self.items.len() }
    pub fn is_empty(&self) -> bool { self.items.is_empty() }
    pub fn iter(&self) -> impl Iterator<Item = &Toast> { self.items.iter() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(msg: &str) -> Toast { Toast::new(ToastKind::Info, msg) }

    #[test]
    fn toast_queue_evicts_oldest_at_capacity() {
        let mut q = ToastQueue::default(); // capacity 3
        q.push(t("a"));
        q.push(t("b"));
        q.push(t("c"));
        assert_eq!(q.len(), 3);
        // 4th push evicts "a".
        q.push(t("d"));
        let msgs: Vec<_> = q.iter().map(|x| x.message.clone()).collect();
        assert_eq!(msgs, vec!["b", "c", "d"]);
    }

    #[test]
    fn dismiss_oldest_pops_front() {
        let mut q = ToastQueue::default();
        q.push(t("a"));
        q.push(t("b"));
        let popped = q.dismiss_oldest();
        assert_eq!(popped.unwrap().message, "a");
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn distinct_kinds_round_trip() {
        let mut q = ToastQueue::default();
        q.push(Toast::new(ToastKind::Error, "boom"));
        q.push(Toast::new(ToastKind::Reconnecting, "ws"));
        q.push(Toast::new(ToastKind::ReconnectSuccess, "ok"));
        let kinds: Vec<_> = q.iter().map(|x| x.kind).collect();
        assert_eq!(
            kinds,
            vec![
                ToastKind::Error,
                ToastKind::Reconnecting,
                ToastKind::ReconnectSuccess
            ]
        );
    }
}
