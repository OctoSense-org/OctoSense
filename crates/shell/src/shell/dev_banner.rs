//! The developer-mode banner (ADR 0004 §13): while developer mode is on, a
//! strip along the bottom of the screen says so ("Developer mode: all apps
//! have full access"), how long it lasts, and offers a one-tap **Turn off**.
//! It is drawn over everything else in the shell chrome and cannot be
//! dismissed while the mode is on. The state is `dev_mode.rs`'s, read at
//! every draw; the shell turns the mode off when [`ShellDevBanner::off_hit`]
//! says the press landed on the button.

use makepad_widgets::*;

use super::ui::{contains, rect, DrawShellFill, HAlign, ShellDraw};
use super::{rgb, ShellTokens};
use crate::dev_mode;

pub const HEIGHT: f64 = 30.0;
pub const OFF_LABEL: &str = "Turn off";

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    mod.widgets.ShellDevBannerBase = #(ShellDevBanner::register_widget(vm))
    mod.widgets.ShellDevBanner = set_type_default() do mod.widgets.ShellDevBannerBase {
        width: Fill
        height: Fill
        draw_bg +: {}
        d +: {}
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct ShellDevBanner {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[walk]
    walk: Walk,
    #[layout]
    layout: Layout,
    #[redraw]
    #[live]
    draw_bg: DrawShellFill,
    #[live]
    d: ShellDraw,
    #[live]
    tokens: ShellTokens,
    #[rust]
    area: Area,
    /// Where the strip and its button were last drawn (none while off).
    #[rust]
    strip: Option<Rect>,
    #[rust]
    off: Option<Rect>,
    /// The line last drawn, for the control surface and tests.
    #[rust]
    pub shown: Option<String>,
}

impl ShellDevBanner {
    /// Whether a press at `p` is on the banner's Turn off button.
    pub fn off_hit(&self, p: Vec2d) -> bool {
        self.off.is_some_and(|r| contains(r, p))
    }
    /// Whether a press at `p` is anywhere on the banner (it is the shell's).
    pub fn claims(&self, p: Vec2d) -> bool {
        self.strip.is_some_and(|r| contains(r, p))
    }

    fn draw_banner(&mut self, cx: &mut Cx2d, screen: Rect) {
        self.strip = None;
        self.off = None;
        let Some((active, profile)) = dev_mode::status() else {
            self.shown = None;
            return;
        };
        let tok = self.d.tokens(self.tokens);
        let line = dev_mode::banner_text(&active);
        let lasts = dev_mode::lasts_text(&active, profile, dev_mode::now());
        let strip = rect(screen.pos.x, screen.pos.y + screen.size.y - HEIGHT, screen.size.x, HEIGHT);
        // A warning colour no theme uses for chrome, so it never blends in.
        let ground = rgb(0xE0, 0x8A, 0x00);
        let ink = rgb(0x1A, 0x12, 0x00);
        self.d.solid(cx, strip, ground);
        let px = tok.font.body;
        let off_w = self.d.measure(cx, true, px, OFF_LABEL) + 24.0;
        let off = rect(strip.pos.x + strip.size.x - off_w - 8.0, strip.pos.y + 4.0, off_w, HEIGHT - 8.0);
        self.d.solid(cx, off, ink);
        self.d.label(cx, off, true, px, ground, HAlign::Center, OFF_LABEL);
        let text = rect(strip.pos.x + 12.0, strip.pos.y, (off.pos.x - strip.pos.x - 24.0).max(0.0), HEIGHT);
        let full = format!("{line} · {lasts}");
        self.d.label_elided(cx, text, true, px, ink, HAlign::Left, &full);
        self.strip = Some(strip);
        self.off = Some(off);
        self.shown = Some(full);
    }
}

impl Widget for ShellDevBanner {
    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        cx.begin_turtle(walk, self.layout);
        let screen = cx.turtle().rect();
        self.d.begin_surface(cx);
        self.draw_banner(cx, screen);
        self.d.end_surface(cx);
        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }

    // The shell routes presses (lib.rs `dev_banner_pointer`); nothing here.
    fn handle_event(&mut self, _cx: &mut Cx, _event: &Event, _scope: &mut Scope) {}
}
