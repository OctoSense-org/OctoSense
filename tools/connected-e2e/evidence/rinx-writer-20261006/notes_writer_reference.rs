//! Diagnostic reference only: original ArticlePanel writer, fictional content.
pub use makepad_widgets;
use makepad_widgets::*;
use rinx::article_app::ArticlePanelWidgetRefExt;
app_main!(App);
script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    let app = startup() do #(App::script_component(vm)) {
        ui: Root {main_window := Window {window.inner_size: #(if std::env::args().any(|a| a == "--phone") {vec2(430.0,850.0)} else {vec2(1200.0,820.0)}) body +: {article := ArticlePanel {}}}}
    }
    app
}
#[derive(Script, ScriptHook)] struct App {#[live] ui: WidgetRef}
impl MatchEvent for App {
 fn handle_startup(&mut self, cx: &mut Cx) {self.ui.article_panel(cx,ids!(article)).diagnostic_writer(cx);}
}
impl AppMain for App {
 fn script_mod(vm:&mut ScriptVm)->ScriptValue {
  makepad_widgets::theme_mod(vm);script_eval!(vm,{mod.theme=mod.themes.light});
  rinx::theme::init_standalone(vm);
  article_makepad::apple_fonts::install(vm);makepad_widgets::widgets_mod(vm);
  makepad_widgets::desktop_style::apply_widgets(vm);rinx::app::register_widgets(vm);self::script_mod(vm)
 }
 fn handle_event(&mut self,cx:&mut Cx,event:&Event){self.match_event(cx,event);self.ui.handle_event(cx,event,&mut Scope::empty());}
}
