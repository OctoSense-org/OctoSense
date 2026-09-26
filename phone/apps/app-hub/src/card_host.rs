//! Mount the Hub's policy-enforcing runner inside the mobile content area.
//! The shared module retains verification, storage, assets and containment.
use makepad_app_module::*;
use makepad_widgets::{widget_async::with_isolate, *};
use std::collections::HashMap;

script_mod! {
    use mod.prelude.widgets.*
    mod.widgets.HostedHubCard = set_type_default() do #(HostedHubCard::register_widget(vm)) {
        width: Fill height: Fill flow: Overlay
    }
}

pub struct CardModule;
pub static CARD_MODULE: CardModule = CardModule;
impl AppModule for CardModule {
    fn id(&self) -> &'static str {
        "card"
    }
    fn label(&self) -> &'static str {
        "Card app"
    }
    fn capabilities(&self) -> &'static [&'static str] {
        octosense_appstore::cardapp::CARD_MODULE.capabilities()
    }
    fn open_schema(&self) -> OpenSchema {
        octosense_appstore::cardapp::CARD_MODULE.open_schema()
    }
    fn register(&self, vm: &mut ScriptVm) {
        // The runner opens system apps by id: they must be registered first.
        crate::system_apps();
        octosense_appstore::cardapp::CARD_MODULE.register(vm);
        script_mod(vm);
    }
    fn create(
        &self,
        vm: &mut ScriptVm,
        open: ValidatedOpen,
        handles: InstanceHandles,
    ) -> InstanceParts {
        let mut parts = octosense_appstore::cardapp::CARD_MODULE.create(vm, open, handles);
        let value = script_eval!(vm, {use mod.widgets.* HostedHubCard{}});
        let root = WidgetRef::script_from_value(vm, value);
        {
            let mut hosted = root.borrow_mut::<HostedHubCard>().unwrap();
            hosted.view.children.push((id!(runner), parts.root.clone()));
            hosted.inner = parts.root;
        }
        parts.root = root;
        parts
    }
}

#[derive(Clone, Copy)]
struct MountedPosition {
    source: DVec2,
    applied: DVec2,
}
fn mounted_position(
    previous: Option<MountedPosition>,
    current: DVec2,
    origin: DVec2,
) -> MountedPosition {
    // A script edit replaces the artboard coordinate; undo only our own offset.
    let source = previous
        .filter(|old| old.applied == current)
        .map_or(current, |old| old.source);
    MountedPosition {
        source,
        applied: source + origin,
    }
}
fn notice_is_visible(text: &str) -> bool {
    !text.trim().is_empty()
}

#[derive(Script, Widget)]
struct HostedHubCard {
    #[deref]
    view: View,
    #[rust]
    inner: WidgetRef,
    #[rust]
    positions: HashMap<WidgetUid, MountedPosition>,
    #[rust]
    pending_style: bool,
}
impl ScriptHook for HostedHubCard {
    fn on_after_apply(
        &mut self,
        vm: &mut ScriptVm,
        apply: &Apply,
        scope: &mut Scope,
        _: ScriptValue,
    ) {
        if matches!(apply, Apply::ScriptReapply) {
            // The runner is attached by Rust, so View's declaration traversal
            // cannot reach it. Reapply its live source without recreating it.
            let source = self
                .inner
                .widget_type_id()
                .and_then(|ty| vm.bx.heap.type_default_for_id(ty))
                .unwrap_or_else(|| self.inner.script_source());
            if source != ScriptObject::ZERO {
                let card = self.inner.splash(vm.cx_mut(), ids!(card));
                let guest_source = card.borrow().map(|splash| splash.view.source.clone());
                self.inner.script_apply(vm, apply, scope, source.into());
                // The outer Splash declaration may restyle its shell, but
                // the existing body and its template references remain guest-owned.
                if let (Some(source), Some(mut splash)) = (guest_source, card.borrow_mut()) {
                    splash.view.source = source;
                }
                self.pending_style = true;
            }
        }
    }
}
impl HostedHubCard {
    fn position_children(
        &mut self,
        cx: &mut Cx,
        widget: &WidgetRef,
        origin: DVec2,
        current_positions: &mut HashMap<WidgetUid, MountedPosition>,
    ) {
        if let Some(current) = widget.walk(cx).abs_pos {
            let position = mounted_position(
                self.positions.get(&widget.widget_uid()).copied(),
                current,
                origin,
            );
            if position.applied != current {
                let mut target = widget.clone();
                script_apply_eval!(cx, target, {abs_pos: #(position.applied)});
            }
            current_positions.insert(widget.widget_uid(), position);
        }
        let mut children = Vec::new();
        widget.children(&mut |_, child| children.push(child));
        for child in children {
            self.position_children(cx, &child, origin, current_positions);
        }
    }
}
impl Widget for HostedHubCard {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        let card = self.inner.splash(cx, ids!(card));
        let owner = card
            .borrow()
            .and_then(|splash| cx.script_ref_vm_id(&splash.view.source));
        if let Some(owner) = owner {
            // Interactive children may instantiate script templates. Their
            // unqualified Cx VM must be the same guest VM used for drawing.
            with_isolate(cx, owner, |cx| {
                if let Event::NetworkResponses(responses) = event {
                    // Splash's usual pump looks up an uninstalled VM. Here
                    // the guest is already installed, so deliver only its
                    // registered callbacks through the current-VM entry.
                    cx.handle_script_network_events_for_current_vm(responses);
                }
                self.inner.handle_event(cx, event, scope);
            });
        } else {
            // The shared runner still owns the first start and all refusals.
            self.inner.handle_event(cx, event, scope);
        }
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        cx.begin_turtle(walk, self.view.layout);
        let rect = cx.turtle().rect();
        let notice = self.inner.label(cx, ids!(notice));
        if notice_is_visible(&notice.text()) {
            self.inner.draw_walk_all(cx, scope, Walk::fill());
        } else {
            let card = self.inner.splash(cx, ids!(card));
            if let Some(mut splash) = card.borrow_mut() {
                if let Some(vm_id) = cx.script_ref_vm_id(&splash.view.source) {
                    with_isolate(cx, vm_id, |cx| {
                        if self.pending_style {
                            // Splash flushes its deferred style rebuild only
                            // on draw. Suppress that preparation pass so the
                            // refreshed tree is mounted before any pixels or
                            // hit areas are emitted. Reapply preserves visibility.
                            let visible = splash.view.visible;
                            splash.view.visible = false;
                            splash.draw_walk_all(cx, scope, Walk::fill());
                            splash.view.visible = visible;
                            self.pending_style = false;
                        }
                        let children = splash
                            .view
                            .children
                            .iter()
                            .map(|(_, child)| child.clone())
                            .collect::<Vec<_>>();
                        let mut current_positions = HashMap::new();
                        for child in children {
                            self.position_children(cx, &child, rect.pos, &mut current_positions);
                        }
                        // A live card can replace its guest tree; forget dead
                        // widget IDs while retaining offsets for surviving ones.
                        self.positions = current_positions;
                        splash.draw_walk_all(cx, scope, Walk::fill());
                    });
                }
            };
            // A host service's sheet over the app (a sign-in), drawn on top
            // in its own isolate.
            let sheet_ref = self.inner.splash(cx, ids!(sheet));
            let up = sheet_ref.borrow().filter(|s| s.view.visible).and_then(|s| cx.script_ref_vm_id(&s.view.source));
            if let Some(vm_id) = up {
                if let Some(mut sheet) = sheet_ref.borrow_mut() {
                    with_isolate(cx, vm_id, |cx| {
                        sheet.draw_walk_all(cx, scope, Walk::fill());
                    });
                }
            }
        }
        cx.end_turtle();
        DrawStep::done()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    script_mod! {
        use mod.prelude.widgets.*
        mod.widgets.CardEventProbe = set_type_default() do #(CardEventProbe::register_widget(vm)) {}
        mod.prelude.widgets.CardEventProbe = mod.widgets.CardEventProbe
    }
    #[derive(Script, Widget)]
    struct CardEventProbe {
        #[deref]
        view: View,
        #[rust]
        created: WidgetRef,
        #[rust]
        applied_style: Option<desktop_style::StyleSheet>,
        #[rust]
        edits: usize,
    }
    impl ScriptHook for CardEventProbe {
        fn on_after_apply(&mut self, vm: &mut ScriptVm, _: &Apply, _: &mut Scope, _: ScriptValue) {
            self.applied_style = desktop_style::current(vm);
        }
    }
    impl Widget for CardEventProbe {
        fn handle_event(&mut self, cx: &mut Cx, event: &Event, _: &mut Scope) {
            self.view.handle_event(cx, event, &mut Scope::empty());
            if matches!(event, Event::Custom(_)) {
                self.created = cx.with_vm(|vm| {
                    let value = script_eval!(vm, {use mod.widgets.* Label{text: mod.probe_name}});
                    WidgetRef::script_from_value(vm, value)
                });
            }
        }
        fn draw_walk(&mut self, _: &mut Cx2d, _: &mut Scope, _: Walk) -> DrawStep {
            DrawStep::done()
        }
    }
    fn hosted_fixture(cx: &mut Cx) -> (widget_async::SplashVmId, WidgetRef) {
        cx.with_vm(makepad_widgets::script_mod);
        widget_async::register_splash_isolate_mod(|vm| {
            script_mod(vm);
        });
        let outer = cx.alloc_splash_vm_with_network(false);
        let root = cx.with_script_vm_id_trusted(outer, |vm| {
            super::script_mod(vm);
            let value = script_eval!(vm, {use mod.widgets.* HostedHubCard{}});
            let root = WidgetRef::script_from_value(vm, value);
            let value = script_eval!(vm, {
                use mod.widgets.*
                mod.probe_name = "outer"
                CardEventProbe{notice := Label{text: ""} card := Splash{}}
            });
            let inner = WidgetRef::script_from_value(vm, value);
            let mut hosted = root.borrow_mut::<HostedHubCard>().unwrap();
            hosted.view.children.push((id!(runner), inner.clone()));
            hosted.inner = inner;
            drop(hosted);
            root
        });
        (outer, root)
    }
    #[test]
    fn initialized_card_events_create_widgets_in_the_guest_heap() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (outer, root) = hosted_fixture(&mut cx);
        with_isolate(&mut cx, outer, |cx| {
            let outer_heap = cx.with_vm(|vm| vm.bx.heap.heap_key());
            let card = root.splash(cx, ids!(card));
            card.set_text(cx, "mod.probe_name = \"guest\"\nprobe := CardEventProbe{}");
            root.handle_event(cx, &Event::Custom("create".into()), &mut Scope::empty());
            let probe = card
                .borrow()
                .unwrap()
                .view
                .children
                .iter()
                .find(|(id, _)| *id == id!(probe))
                .unwrap()
                .1
                .clone();
            let probe = probe.borrow::<CardEventProbe>().unwrap();
            assert_eq!(
                probe.created.text(),
                "guest",
                "event templates must resolve against the card's heap"
            );
            assert_eq!(cx.with_vm(|vm| vm.bx.heap.heap_key()), outer_heap);
        });
    }
    #[test]
    fn initialized_card_dispatches_registered_network_callbacks() {
        use makepad_platform::makepad_script_std::net::{HttpEvents, ScriptHttp};
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (outer, root) = hosted_fixture(&mut cx);
        let owner = with_isolate(&mut cx, outer, |cx| {
            let card = root.splash(cx, ids!(card));
            card.borrow_mut().unwrap().set_allow_net(true);
            card.set_text(cx, "mod.response_seen = false\nprobe := CardEventProbe{}");
            let source = card.borrow().unwrap().view.source.clone();
            cx.script_ref_vm_id(&source).unwrap()
        });
        let request_id = LiveId::unique();
        cx.with_script_vm_id_trusted(owner, |vm| {
            let value = script_eval!(vm, {mod.net.HttpEvents{on_response: |response| {mod.response_seen = true}}});
            let events = HttpEvents::script_from_value(vm, value);
            vm.cx_mut().script_std_mut().data.http_requests.push(ScriptHttp {id: request_id, events});
        });
        let event = Event::NetworkResponses(vec![NetworkResponse::HttpResponse {
            request_id,
            response: HttpResponse::new(LiveId(0), 200, Default::default(), None),
        }]);
        with_isolate(&mut cx, outer, |cx| {
            root.handle_event(cx, &event, &mut Scope::empty())
        });
        cx.with_script_vm_id_trusted(owner, |vm| {
            assert_eq!(script_eval!(vm, {mod.response_seen}).as_bool(), Some(true));
            assert!(vm.cx_mut().script_std_mut().data.http_requests.is_empty());
        });
    }
    #[test]
    fn restyle_reaches_the_live_runner_without_replacing_edits() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (outer, mut root) = hosted_fixture(&mut cx);
        let probe = root.borrow::<HostedHubCard>().unwrap().inner.clone();
        let uid = probe.widget_uid();
        probe.borrow_mut::<CardEventProbe>().unwrap().edits = 7;
        let style = desktop_style::StyleSheet::load_with_appearance(
            desktop_style::DesktopStyle::Macos,
            true,
        );
        cx.with_script_vm_id_trusted(outer, |vm| {
            desktop_style::install(vm, style.clone());
            let source = root.script_source();
            root.script_apply(
                vm,
                &Apply::ScriptReapply,
                &mut Scope::empty(),
                source.into(),
            );
        });
        let current = root.borrow::<HostedHubCard>().unwrap().inner.clone();
        assert_eq!(current.widget_uid(), uid);
        let current = current.borrow::<CardEventProbe>().unwrap();
        assert_eq!(current.edits, 7);
        assert_eq!(
            current.applied_style.as_ref().map(|s| &s.name),
            Some(&style.name)
        );
    }
    #[test]
    fn shared_runner_restyle_keeps_the_guest_source_owner() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (outer, mut root) = hosted_fixture(&mut cx);
        cx.with_script_vm_id_trusted(outer, |vm| {
            octosense_appstore::cardapp::CARD_MODULE.register(vm);
            let value = script_eval!(vm, {use mod.widgets.* CardAppView{}});
            let inner = WidgetRef::script_from_value(vm, value);
            let mut hosted = root.borrow_mut::<HostedHubCard>().unwrap();
            hosted.inner = inner.clone();
            hosted.view.children.clear();
            hosted.view.children.push((id!(runner), inner));
        });
        let card = with_isolate(&mut cx, outer, |cx| {
            let card = root.splash(cx, ids!(card));
            card.set_text(
                cx,
                "mod.probe_name = \"guest\"\nprobe := CardEventProbe{abs_pos: vec2(24,75)}",
            );
            card
        });
        let source = card.borrow().unwrap().view.source.clone();
        let owner = cx.script_ref_vm_id(&source);
        assert_ne!(owner, Some(outer));
        cx.with_script_vm_id_trusted(outer, |vm| {
            desktop_style::install(vm, desktop_style::StyleSheet {
                name: "card-restyle-test".into(),
                theme: "mod.saved_widgets = mod.widgets\nmod.saved_prelude = mod.prelude.widgets\n0\n".into(),
                widgets: "mod.widgets = {..mod.saved_widgets, ..mod.widgets}\nmod.prelude.widgets = {..mod.saved_prelude, ..mod.prelude.widgets}\n0\n".into(),
                icons: Vec::new(),
            });
            let source = root.script_source();
            root.script_apply(vm, &Apply::ScriptReapply, &mut Scope::empty(), source.into());
        });
        let current = card.borrow().unwrap().view.source.clone();
        assert_eq!(
            cx.script_ref_vm_id(&current),
            owner,
            "the body still belongs to its nested VM after restyling"
        );
        // A lazy style rebuild must finish before the first visible frame is
        // translated, otherwise it restores the old window-absolute positions.
        with_isolate(&mut cx, outer, |cx| {
            let pass = DrawPass::new(cx);
            pass.set_size(cx, dvec2(400.0, 700.0));
            let mut draw_list = DrawList2d::new(cx);
            let event = DrawEvent::default();
            let mut draw = makepad_draw::cx_draw::CxDraw::new(cx, &event);
            let mut cx = Cx2d::new(&mut draw);
            cx.begin_pass(&pass, None);
            draw_list.begin_always(&mut cx);
            cx.begin_root_turtle(dvec2(400.0, 700.0), Layout::default());
            root.draw_walk_all(
                &mut cx,
                &mut Scope::empty(),
                Walk {
                    abs_pos: Some(dvec2(10.0, 42.0)),
                    width: Size::Fixed(360.0),
                    height: Size::Fixed(640.0),
                    ..Default::default()
                },
            );
            cx.end_turtle();
            draw_list.end(&mut cx);
            cx.end_pass(&pass);
        });
        let probe = card
            .borrow()
            .unwrap()
            .view
            .children
            .iter()
            .find(|(id, _)| *id == id!(probe))
            .unwrap()
            .1
            .clone();
        assert_eq!(probe.walk(&mut cx).abs_pos, Some(dvec2(34.0, 117.0)));
    }
    #[test]
    fn mounted_positions_do_not_accumulate_offsets_and_preserve_script_edits() {
        let first = mounted_position(None, dvec2(24.0, 75.0), dvec2(10.0, 42.0));
        assert_eq!(first.applied, dvec2(34.0, 117.0));
        let repeated = mounted_position(Some(first), first.applied, dvec2(10.0, 42.0));
        assert_eq!(repeated.applied, first.applied);
        let moved = mounted_position(Some(repeated), repeated.applied, dvec2(0.0, 70.0));
        assert_eq!(moved.applied, dvec2(24.0, 145.0));
        let scripted = mounted_position(Some(moved), dvec2(30.0, 100.0), dvec2(0.0, 70.0));
        assert_eq!(scripted.applied, dvec2(30.0, 170.0));
    }
    #[test]
    fn only_empty_card_notice_is_omitted() {
        assert!(!notice_is_visible(""));
        assert!(!notice_is_visible(" \n"));
        assert!(notice_is_visible("Cannot open: this app was withdrawn"));
    }
}
