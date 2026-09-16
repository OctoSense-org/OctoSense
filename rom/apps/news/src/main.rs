//! news — headlines from several sources, as a plain full-window Makepad
//! app. Everything that makes it a news app lives in `NewsView`
//! (`src/view.rs`) and none of it depends on this binary, so
//! `cargo test -p octosense-news` covers it without a window.
//!
//! news runs standalone, and unmodified inside OctoSense tiles via the
//! shared --stdin-loop client runtime every Makepad app has. Hosted, it
//! exposes its one read tool to the assistant over the WM's bus
//! (src/ai.rs); standalone there is no assistant in the window, so the
//! port stays closed rather than parking context frames nobody reads.

pub use makepad_widgets;
use makepad_ai_services::port::{AiServicePort, PortEvent};
use makepad_widgets::*;
use octosense_news::{ai, view::NewsView};

app_main!(App);

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    startup() do #(App::script_component(vm)) {
        ui: Root {
            main_window := Window {
                window.title: "News"
                window.inner_size: vec2(480, 800)
                pass +: { clear_color: theme.color_bg_app }
                body +: {
                    padding: 0 margin: 0 spacing: 0
                    news := NewsView {}
                }
            }
        }
    }
}

#[derive(Script, ScriptHook)]
pub struct App {
    #[live]
    ui: WidgetRef,
    /// The app's service toward the assistant: the WM's bus when hosted.
    #[rust]
    ai_port: Option<AiServicePort>,
    /// Last summary sent as volatile context, so unchanged events stay quiet.
    #[rust]
    ai_context: String,
}

impl App {
    fn ai_summary(&self, cx: &mut Cx) -> String {
        self.ui.widget(cx, ids!(news)).borrow::<NewsView>().map(|view| view.ai_summary()).unwrap_or_default()
    }

    fn ai_answer(&self, cx: &mut Cx, call: &makepad_ai_services::wire::ServiceCall) -> makepad_ai_services::wire::ToolResult {
        ai::answer_root(&self.ui.widget(cx, ids!(news)), call)
    }

    fn refresh_ai_context(&mut self, cx: &mut Cx) {
        if self.ai_port.is_none() {
            return;
        }
        let text = self.ai_summary(cx);
        if text == self.ai_context {
            return;
        }
        self.ai_context = text.clone();
        if let Some(port) = self.ai_port.as_ref() {
            port.set_context(&text);
        }
    }

    fn drain_ai_port(&mut self, cx: &mut Cx, event: &Event) {
        let events = match self.ai_port.as_mut() {
            Some(port) => port.handle_event(cx, event),
            None => return,
        };
        for ev in events {
            match ev {
                PortEvent::Registered(endpoint) => {
                    log!("news: AI service registered as {}", endpoint.as_str());
                    self.ai_context.clear();
                    self.refresh_ai_context(cx);
                }
                PortEvent::Call(call) => {
                    let result = self.ai_answer(cx, &call);
                    if let Some(port) = self.ai_port.as_ref() {
                        port.reply(result);
                    }
                }
                // The one tool answers at once, so there is nothing to
                // cancel, and the headlines have no chat of their own to
                // step aside.
                PortEvent::Cancel { .. } | PortEvent::ChatOpen { .. } => {}
                PortEvent::Subscribe { .. } | PortEvent::Unsubscribe { .. } => {}
            }
        }
    }
}

impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        // Dev flags for looking at the faces in a plain window: `--phone`
        // sizes the window like the phone viewport, `--tile` like the wide
        // home tile and asks the view for its compact face, as a host does.
        let args: Vec<String> = std::env::args().collect();
        let window = self.ui.window(cx, ids!(main_window));
        if args.iter().any(|a| a == "--phone" || a == "--tile") {
            // The host gives a face the whole rect; the window's own caption
            // bar would take the top of it.
            let mut win = self.ui.widget(cx, ids!(main_window));
            script_apply_eval!(cx, win, { show_caption_bar: false });
        }
        if args.iter().any(|a| a == "--phone") {
            window.resize(cx, dvec2(402.0, 780.0));
        }
        // Only a host has an assistant to adopt the link; a standalone
        // window would park its context frames forever.
        if cx.in_makepad_studio() {
            self.ai_port = AiServicePort::open(cx, ai::manifest());
        }
        makepad_wm_api::set_title(cx, "News");
        // Storage before the first event reaches the view: the `--tile`
        // dispatch below starts it, and a start with the jail in hand reads
        // the feeds file and the cache in the same pass as the fetches.
        let storage = cx.storage("news");
        if let Some(mut view) = self.ui.widget(cx, ids!(news)).borrow_mut::<NewsView>() {
            view.set_storage(cx, storage);
        }
        if args.iter().any(|a| a == "--tile") {
            window.resize(cx, dvec2(370.0, 98.0));
            self.ui.widget(cx, ids!(news)).handle_event(cx, &Event::Custom(HostedViewMode::Tile.to_json()), &mut Scope::empty());
        }
    }

    // The view handles its own actions (tabs, rows, refresh); nothing reaches the app.
    fn handle_actions(&mut self, _cx: &mut Cx, _actions: &Actions) {}
}

impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        makepad_widgets::script_mod(vm);
        makepad_wm_theme::apply(vm);
        octosense_news::view::script_mod(vm);
        self::script_mod(vm)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        // The window manager asked politely (SUPER+W): go now.
        if let Event::Custom(json) = event {
            if let Some(makepad_wm_api::WmEvent::CloseRequested) = makepad_wm_api::WmEvent::parse(json) {
                cx.quit();
                return;
            }
        }
        self.drain_ai_port(cx, event);
        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
        self.refresh_ai_context(cx);
    }
}
