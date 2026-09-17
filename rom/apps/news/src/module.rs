//! news as a module (aicontrol.md §3): the app the window manager seats in
//! a tile in-process, in an isolate of its own — a home-screen resident,
//! exactly like `weather` and `photos`.
//!
//! `register` puts this crate's widget family and root type into the
//! isolate the host prepared; `create` mints one `NewsView{}` root there,
//! gives it the host's storage jail for the feeds file and the cache, and
//! hands the host its one tool: the same tool the standalone binary answers
//! over its port, answered on the root at call time. The module never opens
//! a socket or spawns a thread itself — every fetch rides the platform's own
//! HTTP request API, so it works identically hosted.

use crate::model::Hosting;
use crate::view::NewsView;
use makepad_ai_services::wire::{ServiceCall, ServiceManifest};
use makepad_app_module::*;
use makepad_widgets::*;

pub struct NewsModule;

/// The one linked instance of the module description: immutable, no state.
pub static NEWS_MODULE: NewsModule = NewsModule;

impl AppModule for NewsModule {
    fn id(&self) -> &'static str {
        "news"
    }

    fn label(&self) -> &'static str {
        "News"
    }

    fn register(&self, vm: &mut ScriptVm) {
        // The reader first: the view's DSL mounts an `ArticleReader`.
        crate::reader::script_mod(vm);
        crate::view::script_mod(vm);
    }

    fn open_schema(&self) -> OpenSchema {
        OpenSchema::new(1)
    }

    fn create(&self, vm: &mut ScriptVm, _open: ValidatedOpen, handles: InstanceHandles) -> InstanceParts {
        let value = script_eval!(vm, {
            use mod.widgets.*
            NewsView {}
        });
        let root = WidgetRef::script_from_value(vm, value);
        if let Some(mut view) = root.borrow_mut::<NewsView>() {
            // The instance's disk is its storage jail: the feeds file and
            // the per-source cache live there, on every host the same way.
            view.set_storage(vm.cx_mut(), handles.storage);
            // In the host's own window: the Browser through the host first,
            // then the reader on this window's web view.
            view.set_hosting(Hosting::Module);
        }
        let shutdown_root = root.clone();
        InstanceParts {
            root: root.clone(),
            executor: Box::new(NewsExecutor { root }),
            shutdown: Box::new(move |vm| {
                if let Some(mut view) = shutdown_root.borrow_mut::<NewsView>() {
                    view.shutdown(vm.cx_mut());
                }
            }),
        }
    }

    /// The cache and the feeds file live in the storage jail; every source
    /// is fetched over HTTP.
    fn capabilities(&self) -> &'static [&'static str] {
        &["storage", "net"]
    }
}

/// The instance's one tool, read from the root at call time.
struct NewsExecutor {
    root: WidgetRef,
}

impl ServiceExecutor for NewsExecutor {
    fn manifest(&self) -> ServiceManifest {
        crate::ai::manifest()
    }

    fn execute(&mut self, _cx: &mut Cx, call: &ServiceCall) -> ExecOutcome {
        ExecOutcome::Done(crate::ai::answer_root(&self.root, call))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{isolate_root, Isolate};

    #[test]
    fn the_module_describes_itself_and_opens_empty() {
        let m = &NEWS_MODULE;
        assert_eq!(m.id(), "news");
        assert_eq!(m.label(), "News");
        assert_eq!(m.capabilities(), &["storage", "net"]);
        let schema = m.open_schema();
        assert_eq!(schema.version, 1);
        assert!(schema.empty_open().is_ok(), "no argument is required");
        assert!(schema.validate(r#"{"source":"hn"}"#, &[]).is_err(), "nothing is passed in");
    }

    /// The whole contract without a window manager: an instance in an
    /// isolate of its own, its root switching faces the same way a
    /// standalone window does, teardown in the host's order.
    #[test]
    fn the_module_mints_its_root_in_a_fresh_isolate_and_switches_faces() {
        let mut iso = Isolate::new();
        let storage = iso.cx.storage("news.test");
        let (replies, _upstream) = ReplySink::pair();
        let handles = InstanceHandles {
            scope: InstanceScope::new(1, 1),
            storage,
            viewport: Viewport { size: dvec2(400.0, 700.0) },
            replies,
        };
        let open = NEWS_MODULE.open_schema().empty_open().unwrap();
        let InstanceParts { root, mut executor, shutdown } = iso.with_vm(|vm| {
            let parts = NEWS_MODULE.create(vm, open, handles);
            assert!(vm.take_errors().is_empty(), "the isolate evaluated the view without errors");
            parts
        });
        assert!(root.borrow::<NewsView>().is_some(), "the root is a NewsView");
        assert!(executor.manifest().tool("headlines").is_some());

        // Full is the default face; the host asks for the tile over
        // `Event::Custom`. The isolate has no network, so no fetch lands;
        // the face switch and the empty answer are what is asserted.
        iso.entered(|cx| root.handle_event(cx, &Event::Custom(HostedViewMode::Tile.to_json()), &mut Scope::empty()));
        {
            let view = root.borrow::<NewsView>().unwrap();
            assert_eq!(view.face(&iso.cx), HostedViewMode::Tile, "the module root switched faces from the host's message");
            assert!(view.has_pending_loads(), "the jail is read on the start");
            assert_eq!(view.ai_summary(), "News: All tab, 0 rows, Loading", "three fetches in flight, nothing landed");
        }
        let call = ServiceCall { call_id: "c1".into(), tool: "headlines".into(), args: String::new() };
        match executor.execute(&mut iso.cx, &call) {
            ExecOutcome::Done(result) => assert_eq!(result.text, "No headlines: Loading"),
            _ => panic!("the tool answers at once"),
        }

        // The host's order: shutdown in the isolate, the refs, the isolate.
        iso.with_vm(|vm| shutdown(vm));
        drop(executor);
        iso.teardown(root);
    }

    /// A standalone window seats the view before its startup hands over a
    /// storage namespace: the late handle is read at once, not on a start
    /// that already happened.
    #[test]
    fn storage_handed_to_a_started_view_is_read_at_once() {
        let (mut iso, root) = isolate_root();
        iso.entered(|cx| {
            root.handle_event(cx, &Event::Custom(HostedViewMode::Full.to_json()), &mut Scope::empty());
            let mut view = root.borrow_mut::<NewsView>().unwrap();
            assert!(!view.has_pending_loads(), "started without a jail: nothing to read");
            let storage = cx.storage("news.late");
            view.set_storage(cx, storage);
            assert!(view.has_pending_loads(), "the late handle is read without another start");
            view.shutdown(cx);
        });
        iso.teardown(root);
    }
}
