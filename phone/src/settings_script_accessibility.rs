//! Geometry and Android node transport for script-owned Settings semantics.
//! This module has no application pages, native settings targets or UI policy.
use crate::settings_accessibility::{self as a11y, Action, ActionKind, Bounds, Identities, Layout, Node, Role, Scroll};
use makepad_strict_json::Value;
use makepad_widgets::*;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug)]
pub(crate) struct Metadata {
    pub(crate) key: String,
    pub(crate) pane: String,
    pub(crate) active_scroll: String,
}
impl Metadata {
    pub(crate) fn decode(state: &Value) -> Result<Self, String> {
        static WIDGETS: std::sync::OnceLock<HashSet<String>> = std::sync::OnceLock::new();
        let widgets = WIDGETS.get_or_init(crate::settings_script::bundled_widgets);
        let invalid = || "Invalid script accessibility metadata".to_owned();
        let data = state.get("accessibility").ok_or_else(invalid)?;
        let Value::Obj(fields) = data else { return Err(invalid()) };
        if fields.len() != 2 { return Err(invalid()) }
        let key = data.get("key").and_then(Value::as_str)
            .filter(|s| !s.is_empty() && s.len() <= 16384).ok_or_else(invalid)?;
        let pane = data.get("pane").and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty() && s.chars().count() <= a11y::MAX_TEXT).ok_or_else(invalid)?;
        let active_scroll = state.get("active_scroll").and_then(Value::as_str)
            .filter(|s| widgets.contains(*s)).ok_or_else(invalid)?;
        Ok(Self { key: key.into(), pane: a11y::bounded(pane), active_scroll: active_scroll.into() })
    }
}

#[derive(Default)]
pub(crate) struct AccessibilityUi {
    identities: Identities,
    bindings: HashMap<i32, WidgetRef>,
}
impl AccessibilityUi {
    pub(crate) fn tracking(&self) -> bool { !self.identities.token().is_empty() }
    pub(crate) fn sync_page(&mut self, root: WidgetUid, metadata: &Metadata) {
        self.identities.page(format!("{}:{}", root.0, metadata.key));
    }
    pub(crate) fn retire(&mut self) { self.identities.retire(); self.bindings.clear(); }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn layout(
        &mut self, cx: &mut Cx, view: &View, root: WidgetUid, metadata: &Metadata,
        semantic: &HashMap<String, String>, labels: &HashMap<String, String>, disabled: &HashSet<String>, dpi: f64,
    ) -> Layout {
        self.sync_page(root, metadata);
        self.bindings.clear();
        let Some(bounds) = Bounds::from_rect(view.area().clipped_rect(cx), dpi, None) else { return Layout::default() };
        let scroll_widget = view.widget(cx, &[LiveId::from_str(&metadata.active_scroll)]);
        let scroll_area = scroll_widget.area().clipped_rect(cx);
        let scroll_bounds = Bounds::from_rect(scroll_area, dpi, Some(bounds));
        let scroll_id = scroll_bounds.and_then(|_| self.identities.id(format!("scroll:{}", metadata.active_scroll)));
        let rows: HashMap<LiveId, &str> = semantic.iter().map(|(id, value)| (LiveId::from_str(id), value.as_str())).collect();
        let labels: HashMap<LiveId, &str> = labels.iter().map(|(id, value)| (LiveId::from_str(id), value.as_str())).collect();
        let disabled: HashSet<LiveId> = disabled.iter().map(|id| LiveId::from_str(id)).collect();
        struct Seen { id: LiveId, widget: WidgetRef, parent: bool, context: String, disabled: bool }
        fn walk(widget: &WidgetRef, id: LiveId, parent: bool, context: &str, blocked: bool,
            scroll: WidgetUid, rows: &HashMap<LiveId, &str>, disabled: &HashSet<LiveId>, seen: &mut Vec<Seen>) {
            if !widget.visible() { return }
            let parent = parent || widget.widget_uid() == scroll;
            let context = rows.get(&id).copied().unwrap_or(context);
            let blocked = blocked || disabled.contains(&id);
            seen.push(Seen { id, widget: widget.clone(), parent, context: context.into(), disabled: blocked });
            widget.children(&mut |id, child| walk(&child, id, parent, context, blocked, scroll, rows, disabled, seen));
        }
        let mut seen = Vec::new();
        view.children(&mut |id, child| walk(&child, id, false, "", false, scroll_widget.widget_uid(), &rows, &disabled, &mut seen));
        let mut top = scroll_area.pos.y;
        let mut bottom = scroll_area.pos.y + scroll_area.size.y;
        let mut nodes = Vec::new();
        for item in seen {
            let widget = &item.widget;
            let area = widget.area();
            if item.parent && widget.widget_uid() != scroll_widget.widget_uid() && !area.is_empty() {
                let rect = area.rect_union(cx, false);
                if rect.size.x > 0. && rect.size.y > 0. { top = top.min(rect.pos.y); bottom = bottom.max(rect.pos.y + rect.size.y); }
            }
            let editor = widget.borrow::<TextInput>().map(|e| (e.is_password(), e.is_read_only()));
            let role = if editor.is_some() { Role::Edit } else if widget.borrow::<Button>().is_some() { Role::Button }
                else if widget.borrow::<Label>().is_some() { Role::Text } else { continue };
            // Secure editors stay native; never export a password accidentally.
            if editor.is_some_and(|e| e.0) { continue }
            let parent_bounds = if item.parent { scroll_bounds } else { Some(bounds) };
            let Some(node_bounds) = Bounds::from_rect(area.clipped_rect_union(cx), dpi, parent_bounds) else { continue };
            if nodes.len() >= a11y::MAX_NODES { break }
            let text = widget.text();
            let label = a11y::bounded(labels.get(&item.id).copied().unwrap_or(&text));
            if label.trim().is_empty() { continue }
            let enabled = !item.disabled && !widget.disabled(cx) && !editor.is_some_and(|e| e.1);
            // Static buttons use their compiled ID and widget identity. Recycled
            // rows additionally inherit the script's complete semantic target.
            let Some(id) = self.identities.id(format!("{}:{:?}:{}", widget.widget_uid().0, item.id, item.context)) else { continue };
            let actions = if !enabled { vec![] } else { match role {
                Role::Button => vec![ActionKind::Click], Role::Edit => vec![ActionKind::Focus, ActionKind::SetText], Role::Text => vec![],
            }};
            nodes.push(Node { id, parent: if item.parent { scroll_id } else { None }, role, label,
                value: if role == Role::Edit { a11y::bounded(&text) } else { String::new() },
                enabled, focused: role == Role::Edit && cx.has_key_focus(area), actions, bounds: node_bounds });
            self.bindings.insert(id, widget.clone());
        }
        Layout { token: self.identities.token().into(), pane: metadata.pane.clone(), bounds,
            scroll: scroll_id.zip(scroll_bounds).map(|(id, bounds)| Scroll { id, bounds,
                forward: bottom > scroll_area.pos.y + scroll_area.size.y + 1., backward: top < scroll_area.pos.y - 1. }), nodes }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare_action(
        &mut self, cx: &mut Cx, view: &View, root: WidgetUid, metadata: &Metadata,
        semantic: &HashMap<String, String>, labels: &HashMap<String, String>, disabled: &HashSet<String>, dpi: f64, action: &Action,
    ) -> Option<PreparedAction> {
        let layout = self.layout(cx, view, root, metadata, semantic, labels, disabled, dpi);
        if !layout.permits(action) { return None }
        let widget = if matches!(action.kind, ActionKind::ScrollForward | ActionKind::ScrollBackward) {
            view.widget(cx, &[LiveId::from_str(&metadata.active_scroll)])
        } else { self.bindings.get(&action.id)?.clone() };
        if action.kind == ActionKind::SetText && action.text.is_none() { return None }
        Some(PreparedAction { widget, kind: action.kind, text: action.text.clone() })
    }
}

/// The caller releases its accessibility borrow before delivering events to the
/// controller. No domain request is executed or retried by this native layer.
pub(crate) struct PreparedAction { widget: WidgetRef, kind: ActionKind, text: Option<String> }
impl PreparedAction {
    pub(crate) fn dispatch(self, cx: &mut Cx, mut dispatch: impl FnMut(&mut Cx, Event)) -> bool {
        let widget = self.widget;
        match self.kind {
            ActionKind::ScrollForward | ActionKind::ScrollBackward => {
                let rect = widget.area().clipped_rect(cx);
                let event = Event::Scroll(makepad_platform::event::ScrollEvent {
                    window_id: CxWindowPool::id_zero(), scroll: dvec2(0., rect.size.y * 0.8 * if self.kind == ActionKind::ScrollForward { 1. } else { -1. }),
                    abs: rect.pos + rect.size * 0.5, modifiers: Default::default(), handled_x: Default::default(), handled_y: Default::default(),
                    is_mouse: true, time: crate::host::now(), phase: Default::default(),
                });
                widget.handle_event(cx, &event, &mut Scope::empty());
            }
            ActionKind::Click => {
                for phase in [ButtonAction::Pressed(Default::default()), ButtonAction::Clicked(Default::default())] {
                    let actions = cx.capture_actions(|cx| cx.widget_action(widget.widget_uid(), phase));
                    dispatch(cx, Event::Actions(actions));
                }
            }
            ActionKind::Focus => {
                if let Some(mut input) = widget.borrow_mut::<TextInput>() { input.take_key_focus(cx) } else { return false }
            }
            ActionKind::SetText => {
                let Some(text) = self.text else { return false };
                widget.set_text(cx, &text);
                if let Some(mut input) = widget.borrow_mut::<TextInput>() { input.move_cursor_text_end(cx, false); }
                let value = widget.text();
                let actions = cx.capture_actions(|cx| cx.widget_action(widget.widget_uid(), makepad_widgets::text_input::TextInputAction::Changed(value)));
                dispatch(cx, Event::Actions(actions));
            }
        }
        widget.redraw(cx);
        true
    }
}
