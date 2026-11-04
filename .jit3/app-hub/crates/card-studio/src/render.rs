//! Render a card in a hidden `card-host --remote`, one process per size.
//!
//! The makepad remote instrument (makepad `docs/agents/app-remote.md`) has no
//! resize route, so each size is its own launch with card-host's
//! `--size <w>x<h>`, which also drops the caption bar so the card gets the
//! whole window. Every launch is `MAKEPAD_HIDE_WINDOWS=1`, captured only
//! through `/snap`, `/d`, `/log` and `/gq` (grab, then quit), and the process
//! is waited for; only a process that ignores `/gq` is killed, and only the
//! one this module started.
use crate::capture::{Capture, REALIZE_PREFIX};
use crate::checks;
use crate::remote;
use crate::report::{CardInput, Report, SizeReport};
use crate::sizes::TargetSize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct RenderOptions {
    /// An L0 card file, or a bundle directory (`page.card` + `kit/`, or a
    /// script app's `main.splash`).
    pub card: PathBuf,
    /// The host-resolved sources, keyed by declaration name (what card-host
    /// reads as `page.data.json`), e.g. a `sys.digest` payload.
    pub data: Option<PathBuf>,
    pub sizes: Vec<TargetSize>,
    pub card_host: PathBuf,
    /// The L0 kit directory (`_kit.octoscript`, `_palette_*.octoscript`, …)
    /// a bare card file is lowered with. A bundle brings its own `kit/`.
    pub kit: Option<PathBuf>,
    /// Where the report and its files go.
    pub out_dir: PathBuf,
    /// Per launch: to the listening line, and again to a settled frame.
    pub timeout: Duration,
    /// Re-render at the same width and [`RenderOptions::probe_height`] when
    /// content was cut, to measure how tall it wants to be.
    pub probe: bool,
    pub probe_height: f64,
}

impl RenderOptions {
    pub fn new(card: PathBuf, card_host: PathBuf, out_dir: PathBuf) -> RenderOptions {
        RenderOptions {
            card,
            data: None,
            sizes: TargetSize::defaults(),
            card_host,
            kit: None,
            out_dir,
            timeout: Duration::from_secs(30),
            probe: true,
            probe_height: 2000.0,
        }
    }
}

/// Where card-host is when nobody said: `$CARD_HOST`, then next to this
/// binary (both build into the same `target/<profile>/`), then `PATH`.
pub fn default_card_host() -> PathBuf {
    if let Some(p) = std::env::var_os("CARD_HOST") {
        return PathBuf::from(p);
    }
    if let Some(sibling) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join("card-host")))
    {
        if sibling.is_file() {
            return sibling;
        }
    }
    PathBuf::from("card-host")
}

/// Render every size, run the checks, write the report and its files into
/// `out_dir`, and return the report. `report.json` is written last.
pub fn render(opts: &RenderOptions) -> Result<Report, String> {
    std::fs::create_dir_all(&opts.out_dir)
        .map_err(|e| format!("{}: {e}", opts.out_dir.display()))?;
    let work = std::env::temp_dir().join(format!("card-studio-{}-{}", std::process::id(), nanos()));
    let result = render_in(opts, &work);
    let _ = std::fs::remove_dir_all(&work);
    let report = result?;
    let json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    std::fs::write(opts.out_dir.join("report.json"), json + "\n").map_err(|e| e.to_string())?;
    Ok(report)
}

fn render_in(opts: &RenderOptions, work: &Path) -> Result<Report, String> {
    let (bundle, input) = prepare_bundle(opts, &work.join("bundle"))?;
    let app_data = work.join("app-data");
    let mut sizes = BTreeMap::new();
    for size in &opts.sizes {
        let files = SizeFiles::new(&opts.out_dir, &size.name);
        let (capture, png) = render_one(
            opts,
            &bundle,
            &app_data,
            size.width,
            size.height,
            &work.join(format!("{}.stdout", size.name)),
        )?;
        let mut checked = checks::check(&capture, size, None);
        let cut = checked.metrics.hidden_text > 0
            || checked.findings.iter().any(|f| f.check == "text_clipped");
        if cut && opts.probe && opts.probe_height > size.height {
            let probe_log = work.join(format!("{}.probe.stdout", size.name));
            if let Ok((probe, _)) = render_one(
                opts,
                &bundle,
                &app_data,
                size.width,
                opts.probe_height,
                &probe_log,
            ) {
                checked = checks::check(&capture, size, Some(&probe));
            }
        }
        // Keep what was captured, next to the report.
        write(
            &files.snapshot,
            &serde_json::to_string_pretty(&capture.snap).unwrap_or_default(),
        )?;
        write(&files.tree, &capture.tree)?;
        write(
            &files.log,
            &serde_json::to_string_pretty(&capture.log).unwrap_or_default(),
        )?;
        let png_rel = match png {
            Some(src) => {
                std::fs::copy(&src, &files.png)
                    .map_err(|e| format!("copy {}: {e}", src.display()))?;
                Some(files.rel(&files.png))
            }
            None => None,
        };
        let pass = checked.pass();
        sizes.insert(
            size.name.clone(),
            SizeReport {
                kind: size.kind,
                target: [size.width, size.height],
                viewport: checked.viewport.map(|v| [v.w, v.h]),
                png: png_rel,
                snapshot: Some(files.rel(&files.snapshot)),
                tree: Some(files.rel(&files.tree)),
                log: Some(files.rel(&files.log)),
                realize: capture.realize_report(),
                findings: checked.findings,
                metrics: checked.metrics,
                pass,
            },
        );
    }
    Ok(Report::new(input, sizes))
}

struct SizeFiles {
    dir: PathBuf,
    png: PathBuf,
    snapshot: PathBuf,
    tree: PathBuf,
    log: PathBuf,
}

impl SizeFiles {
    fn new(dir: &Path, name: &str) -> SizeFiles {
        let name: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        SizeFiles {
            dir: dir.to_path_buf(),
            png: dir.join(format!("{name}.png")),
            snapshot: dir.join(format!("{name}.snap.json")),
            tree: dir.join(format!("{name}.tree.txt")),
            log: dir.join(format!("{name}.log.json")),
        }
    }
    fn rel(&self, p: &Path) -> String {
        p.strip_prefix(&self.dir)
            .unwrap_or(p)
            .to_string_lossy()
            .into_owned()
    }
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

fn nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// Make the bundle card-host runs: a copy, so `--stamp` never touches the
/// input. A card file gets a minimal unsigned manifest and the kit.
pub fn prepare_bundle(opts: &RenderOptions, dir: &Path) -> Result<(PathBuf, CardInput), String> {
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let kind;
    if opts.card.is_dir() {
        copy_dir(&opts.card, dir)?;
        kind = if dir.join("main.splash").is_file() {
            "script"
        } else {
            "bundle"
        };
        if !dir.join("manifest.json").is_file() {
            write(&dir.join("manifest.json"), PREVIEW_MANIFEST)?;
        }
    } else {
        kind = "l0";
        let card = std::fs::read_to_string(&opts.card)
            .map_err(|e| format!("{}: {e}", opts.card.display()))?;
        write(&dir.join("page.card"), &card)?;
        write(&dir.join("manifest.json"), PREVIEW_MANIFEST)?;
        let kit = opts
            .kit
            .clone()
            .or_else(|| std::env::var_os("CARD_STUDIO_KIT").map(PathBuf::from))
            .ok_or("a card file needs the L0 kit: pass --kit <dir> or set CARD_STUDIO_KIT (e.g. Octoscript-Makepad's components/l0)")?;
        if !kit.join("_kit.octoscript").is_file() {
            return Err(format!(
                "{}: not an L0 kit (no _kit.octoscript)",
                kit.display()
            ));
        }
        copy_dir(&kit, &dir.join("kit"))?;
    }
    match &opts.data {
        Some(data) => {
            let text =
                std::fs::read_to_string(data).map_err(|e| format!("{}: {e}", data.display()))?;
            serde_json::from_str::<serde_json::Value>(&text)
                .map_err(|e| format!("{}: {e}", data.display()))?;
            write(&dir.join("page.data.json"), &text)?;
        }
        None if kind == "l0" => write(&dir.join("page.data.json"), "{}")?,
        None => {}
    }
    Ok((
        dir.to_path_buf(),
        CardInput {
            input: opts.card.to_string_lossy().into_owned(),
            kind: kind.into(),
            data: opts.data.as_ref().map(|d| d.to_string_lossy().into_owned()),
        },
    ))
}

/// The manifest a bare card is previewed under: no capabilities at all.
const PREVIEW_MANIFEST: &str = r#"{
  "schema": 1,
  "id": "card-studio-preview",
  "version": "0.0.0",
  "name": "card-studio preview",
  "integrity": { "bundle_blake3": "" },
  "capabilities": []
}
"#;

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    for entry in std::fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        if ty.is_dir() {
            copy_dir(&src, &dst)?;
        } else if ty.is_file() {
            std::fs::copy(&src, &dst).map_err(|e| format!("{}: {e}", src.display()))?;
        }
    }
    Ok(())
}

/// One owned card-host process.
struct Instance {
    child: Child,
    port: u16,
}

impl Instance {
    fn launch(
        card_host: &Path,
        bundle: &Path,
        app_data: &Path,
        w: f64,
        h: f64,
        stdout: &Path,
        timeout: Duration,
    ) -> Result<Instance, String> {
        let out =
            std::fs::File::create(stdout).map_err(|e| format!("{}: {e}", stdout.display()))?;
        let err = out.try_clone().map_err(|e| e.to_string())?;
        let child = Command::new(card_host)
            .arg("--bundle")
            .arg(bundle)
            .arg("--app-data")
            .arg(app_data)
            .arg("--allow-unsigned")
            .arg("--stamp")
            .arg(format!("--size={w}x{h}"))
            .arg("--remote")
            .env("MAKEPAD_HIDE_WINDOWS", "1")
            .env_remove("MAKEPAD_REMOTE")
            .env_remove("MAKEPAD_FOCUS")
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(err)
            .spawn()
            .map_err(|e| format!("{}: {e}", card_host.display()))?;
        let mut instance = Instance { child, port: 0 };
        let deadline = Instant::now() + timeout;
        loop {
            let text = std::fs::read_to_string(stdout).unwrap_or_default();
            if let Some(port) = listening_port(&text) {
                instance.port = port;
                return Ok(instance);
            }
            if let Ok(Some(status)) = instance.child.try_wait() {
                return Err(format!(
                    "card-host exited ({status}) before listening:\n{}",
                    tail(&text, 20)
                ));
            }
            if Instant::now() > deadline {
                instance.stop();
                return Err(format!(
                    "card-host did not start listening within {timeout:?}:\n{}",
                    tail(&text, 20)
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn get_json(&self, path: &str) -> Result<serde_json::Value, String> {
        remote::get_json(self.port, path, Duration::from_secs(10))
    }

    fn get_text(&self, path: &str) -> Result<String, String> {
        remote::get(self.port, path, Duration::from_secs(10))
            .map(|b| String::from_utf8_lossy(&b).into_owned())
    }

    /// Wait for the card to be mounted (card-host logged its realize report,
    /// a lowering error or a refusal) and the drawn tree to stop changing.
    fn settle(&self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        let mounted = |log: &serde_json::Value| {
            log["l"].as_array().is_some_and(|l| {
                l.iter().filter_map(|s| s.as_str()).any(|s| {
                    s.contains(REALIZE_PREFIX)
                        || s.contains("card-host: refused")
                        || s.contains("card-host: the card did not lower")
                        || s.contains("[SPLASH] eval")
                })
            })
        };
        while Instant::now() < deadline {
            if self.get_json("/log?n=400").is_ok_and(|l| mounted(&l)) {
                break;
            }
            std::thread::sleep(Duration::from_millis(150));
        }
        let mut last = String::new();
        let mut stable = 0;
        while Instant::now() < deadline && stable < 3 {
            std::thread::sleep(Duration::from_millis(250));
            let tree = self.get_text("/d").unwrap_or_default();
            if tree == last && tree.lines().count() > 1 {
                stable += 1;
            } else {
                stable = 0;
                last = tree;
            }
        }
    }

    /// `/gq`: grab every window, then quit. Waits for the exit.
    fn grab_and_quit(&mut self) -> Option<PathBuf> {
        let png = self.get_json("/gq").ok().and_then(|v| {
            v["png"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|p| p.as_str())
                .map(PathBuf::from)
        });
        self.stop();
        png
    }

    /// Wait for the process to leave; `/quit` it if it has not, and kill it
    /// — it is ours — only if that fails too.
    fn stop(&mut self) {
        let wait = |child: &mut Child, ms: u64| {
            let deadline = Instant::now() + Duration::from_millis(ms);
            while Instant::now() < deadline {
                if let Ok(Some(_)) = child.try_wait() {
                    return true;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            false
        };
        if wait(&mut self.child, 5000) {
            return;
        }
        let _ = remote::get(self.port, "/quit", Duration::from_secs(2));
        if !wait(&mut self.child, 5000) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        if let Ok(None) = self.child.try_wait() {
            self.stop();
        }
    }
}

fn render_one(
    opts: &RenderOptions,
    bundle: &Path,
    app_data: &Path,
    w: f64,
    h: f64,
    stdout: &Path,
) -> Result<(Capture, Option<PathBuf>), String> {
    let mut instance = Instance::launch(
        &opts.card_host,
        bundle,
        app_data,
        w,
        h,
        stdout,
        opts.timeout,
    )?;
    instance.settle(opts.timeout);
    let snap = instance.get_json("/snap?all=1");
    let tree = instance.get_text("/d");
    let log = instance.get_json("/log?n=500");
    let png = instance.grab_and_quit();
    let capture = Capture::from_parts(snap?, tree?, &log?);
    Ok((capture, png))
}

/// The port in `[makepad-remote] listening on 127.0.0.1:<port> pid=…`.
pub fn listening_port(text: &str) -> Option<u16> {
    let at = text.find("listening on 127.0.0.1:")? + "listening on 127.0.0.1:".len();
    text[at..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()
}

fn tail(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_the_listening_line() {
        let line =
            "[makepad-remote] listening on 127.0.0.1:53412 pid=9931 app=card-host grabs=/tmp/x";
        assert_eq!(super::listening_port(line), Some(53412));
        assert_eq!(super::listening_port("nothing"), None);
    }
}
