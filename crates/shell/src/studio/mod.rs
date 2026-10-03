//! Foreground-only, unpublished L0 previews. No app policy or live app storage
//! is inherited: each render receives a disposable, zero-quota isolate.
use crate::dev_mode::DevTag;
use makepad_widgets::*;
use serde_json::Value;
use std::{
    collections::{HashMap, VecDeque},
    fs::File,
    io::Write,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

const DEADLINE: Duration = Duration::from_secs(20);
const MAX_JOBS: usize = 4;
const MAX_PNG: usize = 5 * 1024 * 1024;

pub struct RenderJob {
    pub id: String,
    pub app: String,
    pub source: String,
    pub data: Value,
    pub dark: bool,
    /// None uses the current surface's glance width. Reserved for host tests.
    pub width: Option<f64>,
    pub output: File,
    pub reply: Sender<Result<RenderResult, String>>,
    pub dev_tag: DevTag,
}
#[derive(Debug)]
pub struct RenderResult {
    pub width: u32,
    pub height: u32,
    pub settled: bool,
}
struct Pending {
    job: RenderJob,
    cancel: Arc<AtomicBool>,
    started: Instant,
}
#[derive(Default)]
struct Queue {
    jobs: VecDeque<Pending>,
    tokens: HashMap<String, Arc<AtomicBool>>,
}
fn queue() -> &'static Mutex<Queue> {
    static Q: OnceLock<Mutex<Queue>> = OnceLock::new();
    Q.get_or_init(Default::default)
}
fn authorized(job: &RenderJob) -> bool {
    crate::dev_mode::tag_valid(&job.dev_tag) && crate::dev_mode::grants_all(&job.app)
}
pub fn submit(job: RenderJob) -> Result<(), String> {
    if !authorized(&job) {
        return Err("studio requires a current developer-mode grant".into());
    }
    let mut q = queue().lock().unwrap_or_else(|e| e.into_inner());
    if q.tokens.len() >= MAX_JOBS || q.tokens.contains_key(&job.id) {
        return Err("studio_busy".into());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    q.tokens.insert(job.id.clone(), cancel.clone());
    q.jobs.push_back(Pending {
        job,
        cancel,
        started: Instant::now(),
    });
    SignalToUI::set_ui_signal();
    Ok(())
}
pub fn cancel(id: &str) {
    if let Some(cancel) = queue()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .tokens
        .get(id)
    {
        cancel.store(true, Ordering::Release);
    }
    SignalToUI::set_ui_signal();
}
fn retire(id: &str) {
    queue()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .tokens
        .remove(id);
    SignalToUI::set_ui_signal();
}
fn validity(p: &Pending) -> Result<(), String> {
    if p.cancel.load(Ordering::Acquire) {
        return Err("studio_cancelled".into());
    }
    if !authorized(&p.job) {
        return Err("studio_grant_expired".into());
    }
    if p.started.elapsed() >= DEADLINE {
        return Err("studio_timeout".into());
    }
    Ok(())
}

/// Geometry from the platform's created root window, in layout points. The
/// widget lookup and shell's cached DPI can still be empty at a test action.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SurfaceGeometry {
    width: f64,
    height: f64,
    dpi: f64,
}
impl SurfaceGeometry {
    fn from_window(created: bool, geom: &WindowGeom) -> Option<Self> {
        let width = geom.inner_size.x;
        let height = geom.inner_size.y;
        let dpi = geom.dpi_factor;
        (created
            && width.is_finite()
            && height.is_finite()
            && dpi.is_finite()
            && width >= 112.0
            && height > 0.0
            && (0.5..=4.0).contains(&dpi))
        .then_some(Self { width, height, dpi })
    }
    fn card_width(self) -> f64 {
        self.width - 40.0
    }
}
pub(crate) fn surface_geometry(cx: &Cx) -> Option<SurfaceGeometry> {
    // The shell owns the first window; hosted module windows must never supply
    // its geometry. Use the current generation rather than an assumed id_zero.
    let id = cx.windows.id_iter().next()?;
    let window = &cx.windows[id];
    SurfaceGeometry::from_window(window.is_created, &window.window_geom)
}

struct Active {
    pending: Pending,
    frame: WidgetRef,
    splash: SplashRef,
    pass: DrawPass,
    list: DrawList2d,
    texture: Texture,
    jail: PathBuf,
    width: f64,
    height: f64,
    dpi: f64,
    surface: SurfaceGeometry,
    ticket: Option<ReadbackTicket>,
    previous: Option<Vec<u8>>,
    stable: usize,
    draws: usize,
    repaint: bool,
}
pub struct Renderer {
    active: Option<Active>,
    foreground: bool,
    timer: Option<Timer>,
}
impl Default for Renderer {
    fn default() -> Self {
        Self {
            active: None,
            foreground: true,
            timer: None,
        }
    }
}
impl Renderer {
    /// Called before the shell may consume an event. Background/cancellation
    /// revokes publication immediately, even if a GPU lease is still draining.
    pub(crate) fn event(&mut self, cx: &mut Cx, event: &Event, surface: Option<SurfaceGeometry>) {
        match event {
            Event::Pause | Event::Background => self.foreground = false,
            Event::Resume | Event::Foreground => self.foreground = true,
            _ => {}
        }
        if let Some(a) = self.active.as_ref() {
            let status = if !self.foreground {
                Err("not_foreground".into())
            } else if surface != Some(a.surface) {
                Err("studio_surface_changed".into())
            } else {
                validity(&a.pending)
            };
            if let Err(e) = status {
                self.fail(cx, e);
            }
        }
        if !self.foreground {
            // Includes jobs whose PNG worker already owns the output file.
            for token in queue()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .tokens
                .values()
            {
                token.store(true, Ordering::Release);
            }
        }
        if self.active.is_none() {
            let pending = queue()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .jobs
                .pop_front();
            if let Some(pending) = pending {
                let error = if !self.foreground {
                    Some("not_foreground".into())
                } else {
                    validity(&pending).err()
                };
                if let Some(e) = error {
                    let _ = pending.job.reply.send(Err(e));
                    retire(&pending.job.id);
                } else if let Some(surface) = surface {
                    match Active::new(cx, pending, surface) {
                        Ok(a) => self.active = Some(a),
                        Err((p, e)) => {
                            let _ = p.job.reply.send(Err(e));
                            retire(&p.job.id);
                        }
                    }
                } else {
                    // Wait for the native surface; do not turn zero geometry
                    // into a minimum-width successful image. The original
                    // enqueue deadline still applies while waiting.
                    queue()
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .jobs
                        .push_front(pending);
                }
            }
        }
        let waiting = !queue()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .jobs
            .is_empty();
        if (self.active.is_some() || waiting) && self.timer.is_none() {
            self.timer = Some(cx.start_interval(0.08));
        }
        if self.active.is_none() && !waiting {
            if let Some(timer) = self.timer.take() {
                cx.stop_timer(timer);
            }
        }
        if let Some(a) = self.active.as_mut() {
            // No input reaches the preview, and no host-service pump runs.
            if matches!(event, Event::Signal | Event::Timer(_) | Event::NextFrame(_)) {
                a.frame.handle_event(cx, event, &mut Scope::empty());
            }
            if let Event::Timer(te) = event {
                if self
                    .timer
                    .as_ref()
                    .is_some_and(|t| t.is_timer(te).is_some())
                    && a.ticket.is_none()
                {
                    a.repaint = true;
                    cx.redraw_all();
                }
            }
        }
    }
    /// A separate root pass never becomes a live tile or enters its store.
    pub fn draw(&mut self, cx: &mut Cx, event: &DrawEvent) {
        let Some(a) = self
            .active
            .as_mut()
            .filter(|a| a.repaint && a.ticket.is_none())
        else {
            return;
        };
        a.repaint = false;
        let size = dvec2(a.width, a.height);
        a.pass.set_size(cx, size);
        {
            let mut draw = CxDraw::new(cx, event);
            let mut cx = Cx2d::new(&mut draw);
            cx.begin_pass(&a.pass, Some(a.dpi));
            a.list.begin_always(&mut cx);
            cx.begin_root_turtle(size, Layout::flow_overlay());
            let walk = Walk::abs_rect(Rect {
                pos: dvec2(0.0, 0.0),
                size,
            });
            let vm_id = a
                .splash
                .borrow()
                .and_then(|s| cx.script_ref_vm_id(&s.view.source));
            match vm_id {
                Some(id) => widget_async::with_isolate(&mut cx, id, |cx| {
                    a.frame.draw_walk_all(cx, &mut Scope::empty(), walk)
                }),
                None => a.frame.draw_walk_all(&mut cx, &mut Scope::empty(), walk),
            }
            cx.end_pass_sized_turtle();
            a.list.end(&mut cx);
            cx.end_pass(&a.pass);
        }
        a.draws += 1;
        let measured = a.splash.area().rect(cx).size.y.clamp(
            crate::glance_card::TILE_MIN_HEIGHT,
            crate::glance_card::TILE_MAX_HEIGHT,
        );
        if (a.height - measured).abs() > 0.5 {
            a.height = measured;
            a.previous = None;
            a.stable = 0;
            return;
        }
        // Shader readiness is checked after at least one platform paint. A
        // not-yet-compiled shader makes us request another frame, not a PNG.
        if a.draws < 2 || !shaders_ready(cx, a.list.id(), 0) {
            return;
        }
        match a
            .texture
            .read_back(cx, ReadbackRequest { next_render: true })
        {
            Ok(ticket) => a.ticket = Some(ticket),
            Err(ReadbackError::Backpressure) => {}
            Err(e) => self.fail(cx, format!("studio_readback: {e}")),
        }
    }
    /// Return unowned tickets to the shell's shared readback router.
    pub fn readback(&mut self, cx: &mut Cx, result: TextureReadback) -> Option<TextureReadback> {
        if !self
            .active
            .as_ref()
            .is_some_and(|a| a.ticket == Some(result.ticket))
        {
            return Some(result);
        }
        let a = self.active.as_mut().unwrap();
        a.ticket = None;
        let bytes = match rgba(&result, a.pending.job.dark) {
            Ok(b) => b,
            Err(e) => {
                self.fail(cx, e);
                return None;
            }
        };
        if a.previous.as_ref() == Some(&bytes) {
            a.stable += 1;
        } else {
            a.stable = 0;
        }
        a.previous = Some(bytes);
        if a.stable < 2 {
            return None;
        }
        let a = self.active.take().unwrap();
        let bytes = a.previous.clone().unwrap();
        a.splash.set_text(cx, "");
        let _ = std::fs::remove_dir_all(&a.jail);
        let reply = a.pending.job.reply.clone();
        let id = a.pending.job.id.clone();
        // Compression never stalls the UI. Grants, cancellation and the same
        // original deadline are checked again before publishing the file.
        let task = cx.task_pool().submit(Lane::Heavy, move || {
            let mut p = a.pending;
            let outcome = (|| {
                validity(&p)?;
                let png = Cx::encode_rgba_as_png(result.width as u32, result.height as u32, &bytes)
                    .map_err(|e| format!("studio_png: {e:?}"))?;
                if png.len() > MAX_PNG {
                    return Err("studio_png_too_large".into());
                }
                validity(&p)?;
                p.job.output.write_all(&png).map_err(|e| e.to_string())?;
                p.job.output.flush().map_err(|e| e.to_string())?;
                validity(&p)?;
                Ok(RenderResult {
                    width: result.width as u32,
                    height: result.height as u32,
                    settled: true,
                })
            })();
            let _ = p.job.reply.send(outcome);
            retire(&p.job.id);
        });
        match task {
            Ok(task) => task.detach(),
            Err(e) => {
                let _ = reply.send(Err(format!("studio_worker: {e}")));
                retire(&id);
            }
        }
        None
    }
    fn fail(&mut self, cx: &mut Cx, reason: String) {
        if let Some(a) = self.active.take() {
            if let Some(ticket) = a.ticket {
                cx.cancel_texture_readback(ticket);
            }
            a.splash.set_text(cx, "");
            let _ = std::fs::remove_dir_all(&a.jail);
            let _ = a.pending.job.reply.send(Err(reason));
            retire(&a.pending.job.id);
        }
    }
}
impl Active {
    fn new(
        cx: &mut Cx,
        pending: Pending,
        surface: SurfaceGeometry,
    ) -> Result<Self, (Pending, String)> {
        let body = match crate::glance::prepare_studio(
            &pending.job.app,
            &pending.job.source,
            &pending.job.data,
            pending.job.dark,
        ) {
            Ok(b) => b,
            Err(e) => return Err((pending, e)),
        };
        if !crate::glance_card::CAN_RENDER {
            return Err((
                pending,
                "studio renderer needs the App Hub vocabulary".into(),
            ));
        }
        let width = pending.job.width.unwrap_or(surface.card_width());
        let dpi = surface.dpi;
        if !width.is_finite()
            || !(72.0..=2048.0).contains(&width)
            || !dpi.is_finite()
            || !(0.5..=4.0).contains(&dpi)
        {
            return Err((pending, "invalid studio surface geometry".into()));
        }
        log!("studio-test-geometry: surface_width={} surface_height={} dpi={} card_width={} pixel_width={}",
            surface.width, surface.height, dpi, width, (width * dpi).round());
        let base = cx
            .get_data_dir()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let jail = base.join(format!(".studio-isolate-{}", uuid::Uuid::new_v4()));
        if let Err(e) = std::fs::create_dir(&jail) {
            return Err((pending, e.to_string()));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Err(e) = std::fs::set_permissions(&jail, std::fs::Permissions::from_mode(0o700))
            {
                let _ = std::fs::remove_dir(&jail);
                return Err((pending, e.to_string()));
            }
        }
        crate::glance_card::ensure_vocabulary(cx);
        let frame = cx.with_vm(|vm| {
            let value = script_eval!(vm,{use mod.widgets.* GlanceTileFrame {}});
            WidgetRef::script_from_value(vm, value)
        });
        let splash = frame.splash(cx, ids!(card));
        if let Some(mut s) = splash.borrow_mut() {
            s.set_allow_net(false);
        }
        splash.set_sandbox_dir(cx, Some(jail.clone()));
        splash.set_storage_quota(cx, Some(0));
        splash.set_host_caps(cx, Vec::new());
        splash.set_host_prompts(cx, false);
        splash.set_policy(cx, Some(Vec::new()), Some(5_000_000));
        splash.set_memory_bytes(cx, Some(16 * 1024 * 1024));
        splash.set_text(cx, &body);
        let mut children = 0;
        if let Some(s) = splash.borrow() {
            s.children(&mut |_, _| children += 1);
        }
        let running = splash
            .borrow_mut()
            .and_then(|mut s| s.isolate_heap_key(cx))
            .is_some_and(makepad_widgets::splash_policy::may_run);
        if children == 0 || !running {
            splash.set_text(cx, "");
            let _ = std::fs::remove_dir_all(&jail);
            return Err((pending, "studio_eval_failed: preview has no root content or exhausted its instruction budget".into()));
        }
        let pass = DrawPass::new_with_name(cx, "studio_l0_preview");
        let texture = Texture::new_with_format(
            cx,
            TextureFormat::RenderBGRAu8 {
                size: TextureSize::Auto,
                initial: true,
            },
        );
        pass.set_color_texture(
            cx,
            &texture,
            DrawPassClearColor::ClearWith(vec4(0.0, 0.0, 0.0, 0.0)),
        );
        let list = DrawList2d::new(cx);
        cx.redraw_all();
        Ok(Self {
            pending,
            frame,
            splash,
            pass,
            list,
            texture,
            jail,
            width,
            height: crate::glance_card::TILE_DEFAULT_HEIGHT,
            dpi,
            surface,
            ticket: None,
            previous: None,
            stable: 0,
            draws: 0,
            repaint: true,
        })
    }
}
fn shaders_ready(cx: &Cx, id: DrawListId, depth: usize) -> bool {
    if depth > 64 || cx.draw_shaders_pending() {
        return false;
    }
    let items = &cx.draw_lists[id].draw_items;
    (0..items.len()).all(|i| {
        let kind = &items[i].kind;
        if let Some(id) = kind.sub_list() {
            return shaders_ready(cx, id, depth + 1);
        }
        if let Some(call) = kind.draw_call() {
            #[cfg(any(target_os = "android", target_os = "linux", target_os = "windows"))]
            {
                return cx.is_draw_shader_window_ready(call.draw_shader_id);
            }
            #[cfg(not(any(target_os = "android", target_os = "linux", target_os = "windows")))]
            {
                let _ = call;
            }
        }
        true
    })
}
/// Platform readbacks are premultiplied. Flatten before PNG encoding rather
/// than interpreting premultiplied bytes as straight-alpha PNG samples.
fn rgba(result: &TextureReadback, dark: bool) -> Result<Vec<u8>, String> {
    let bytes = result
        .data
        .as_ref()
        .map_err(|e| format!("studio_readback: {e}"))?;
    let row = result
        .width
        .checked_mul(4)
        .ok_or("invalid readback dimensions")?;
    let total = result
        .stride
        .checked_mul(result.height)
        .ok_or("invalid readback dimensions")?;
    if result.stride < row || bytes.len() < total {
        return Err("invalid readback stride".into());
    }
    let mut out = Vec::with_capacity(row * result.height);
    let backdrop = if dark { 24u16 } else { 255u16 };
    for y in 0..result.height {
        let y = match result.origin {
            ReadbackOrigin::TopLeft => y,
            ReadbackOrigin::BottomLeft => result.height - 1 - y,
        };
        for p in bytes[y * result.stride..y * result.stride + row].chunks_exact(4) {
            let rgb = match result.channel_order {
                ReadbackChannelOrder::Rgba => [p[0], p[1], p[2]],
                ReadbackChannelOrder::Bgra => [p[2], p[1], p[0]],
            };
            for c in rgb {
                out.push(
                    (u16::from(c) + (backdrop * (255 - u16::from(p[3])) + 127) / 255).min(255)
                        as u8,
                );
            }
            out.push(255);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pixels(order: ReadbackChannelOrder, origin: ReadbackOrigin, data: &[u8]) -> TextureReadback {
        TextureReadback {
            ticket: ReadbackTicket(1),
            allocation_generation: 1,
            producer_serial: 1,
            width: 1,
            height: 2,
            stride: 8,
            channel_order: order,
            origin,
            data: Ok(Arc::from(data)),
        }
    }
    #[test]
    fn png_input_flattens_premultiplied_rows_and_respects_padding_and_origin() {
        let result = pixels(
            ReadbackChannelOrder::Bgra,
            ReadbackOrigin::BottomLeft,
            &[0, 0, 128, 128, 9, 9, 9, 9, 30, 20, 10, 255, 9, 9, 9, 9],
        );
        assert_eq!(
            rgba(&result, false).unwrap(),
            [10, 20, 30, 255, 255, 127, 127, 255]
        );
        assert_eq!(
            rgba(&result, true).unwrap(),
            [10, 20, 30, 255, 140, 12, 12, 255]
        );
    }
    #[test]
    fn bad_readback_payloads_fail_before_encoding() {
        assert!(rgba(
            &pixels(ReadbackChannelOrder::Rgba, ReadbackOrigin::TopLeft, &[0; 8]),
            false
        )
        .is_err());
        let mut result = pixels(
            ReadbackChannelOrder::Rgba,
            ReadbackOrigin::TopLeft,
            &[0; 16],
        );
        result.data = Err(ReadbackError::Cancelled);
        assert!(rgba(&result, false).unwrap_err().contains("Cancelled"));
    }
    #[test]
    fn preview_is_l0_only_and_explicit_modes_do_not_depend_on_global_dark() {
        let (source, data) = crate::glance::demo_digest();
        let light = crate::glance::prepare_studio("os.news", &source, &data, false).unwrap();
        let dark = crate::glance::prepare_studio("os.news", &source, &data, true).unwrap();
        assert_ne!(light, dark);
        assert_eq!(
            light,
            crate::glance::prepare_studio("os.news", &source, &data, false).unwrap()
        );
        assert!(crate::glance::prepare_studio("os.news", "View{}", &data, false).is_err());
        assert!(crate::glance::prepare_studio("os.news", &source, &Value::Null, false).is_err());
    }
    #[test]
    fn digest_values_are_host_owned_and_chat_snapshots_fail_closed() {
        let source = include_str!("../../resources/glance/news-brief.card");
        let data = serde_json::json!({"brief":{"summary":"FORGED","points":[],"sources":[]},"$status":{"brief":"ready"}});
        let body = crate::glance::prepare_studio("os.news", source, &data, false).unwrap();
        assert!(!body.contains("FORGED"));
        assert!(
            crate::glance::prepare_studio("os.mail", source, &data, false)
                .unwrap_err()
                .contains("own app")
        );
        let (_, _, source, data) = crate::glance::demo_mail().remove(0);
        assert!(
            crate::glance::prepare_studio("os.mail", &source, &data, false)
                .unwrap_err()
                .contains("studio_chat_unsupported")
        );
    }
    #[test]
    fn asynchronous_images_are_refused_instead_of_reported_as_settled() {
        let (source, mut data) = crate::glance::demo_digest();
        let source = source.replace(
            "TextTitle(text: status.message, width: .fill)",
            "Photo(src: status.message)",
        );
        data["status"]["message"] = serde_json::json!("https://example.invalid/image.png");
        let error = crate::glance::prepare_studio("os.news", &source, &data, false).unwrap_err();
        assert!(error.contains("studio_images_unsupported"), "{error}");
    }

    #[test]
    fn unowned_readback_is_preserved_for_other_shell_consumers() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let mut renderer = Renderer::default();
        let original = pixels(
            ReadbackChannelOrder::Rgba,
            ReadbackOrigin::TopLeft,
            &[0; 16],
        );
        assert_eq!(
            renderer.readback(&mut cx, original).unwrap().ticket,
            ReadbackTicket(1)
        );
    }
    #[test]
    fn geometry_waits_for_created_surface_and_uses_its_actual_scale() {
        let mut geom = WindowGeom {
            inner_size: dvec2(0.0, 0.0),
            dpi_factor: 1.0,
            ..Default::default()
        };
        assert!(SurfaceGeometry::from_window(true, &geom).is_none());
        geom.inner_size = dvec2(432.0, 912.0);
        geom.dpi_factor = 2.5;
        assert!(SurfaceGeometry::from_window(false, &geom).is_none());
        let surface = SurfaceGeometry::from_window(true, &geom).unwrap();
        assert_eq!(surface.card_width(), 392.0);
        assert_eq!(surface.card_width() * surface.dpi, 980.0);
        for invalid in [0.0, f64::NAN, f64::INFINITY] {
            geom.dpi_factor = invalid;
            assert!(SurfaceGeometry::from_window(true, &geom).is_none());
        }
    }
    #[test]
    fn background_events_close_admission_and_resume_reopens_it() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let mut renderer = Renderer::default();
        renderer.event(&mut cx, &Event::Background, None);
        assert!(!renderer.foreground);
        renderer.event(&mut cx, &Event::Foreground, None);
        assert!(renderer.foreground);
    }
}
