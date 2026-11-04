//! `card-studio`: inspect a card before anyone sees it (ADR 0002 §7).
//!
//! See [`USAGE`]. Besides the command line, the binary speaks the octos
//! skill protocol: `card-studio card_render` and
//! `card-studio card_critique_payload` read the tool's JSON input on stdin and
//! print `{"success": bool, "output": "…"}` (see `skills/card-studio`).
use octosense_card_studio::capture::Capture;
use octosense_card_studio::critique::{self, CritiqueOptions};
use octosense_card_studio::render::{self, RenderOptions};
use octosense_card_studio::report::{Report, Summary};
use octosense_card_studio::{checks, sizes::TargetSize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

const USAGE: &str = "\
card-studio: render a card in a hidden card-host --remote at target sizes,
run the measured checks, and prepare its vision critique (ADR 0002 section 7).

Usage:
  card-studio render --card <file.card|bundle-dir> [--data <json>]
                     [--size glance|phone|desktop|<w>x<h>|<name>=<w>x<h>]...
                     [--out <dir>] [--card-host <bin>] [--kit <dir>]
                     [--timeout <secs>] [--no-probe]
  card-studio check --snap <snap.json> --tree <tree.txt> [--log <log.json>]
                    [--size <size>]
  card-studio critique --report <report.json> --rubric <rubric.md>
                       [--size <name>]... [--inline] [--out <payload.json>]
  card-studio card_render | card_critique_payload     (octos skill protocol)
  card-studio -h | --help

render   Renders at each --size (default: glance, phone and desktop; glance
         is 350x160 for now), one hidden card-host per size, and writes
         <out>/report.json plus <size>.png, .snap.json, .tree.txt, .log.json.
         A card file is lowered with the L0 kit from --kit or $CARD_STUDIO_KIT;
         --data is the host-resolved sources (page.data.json). card-host is
         --card-host, $CARD_HOST, the one next to this binary, or PATH.
         Content that was cut is measured again in a 2000pt-tall probe render
         unless --no-probe. Prints the report; exits 0 on pass, 1 on fail.
check    The measured checks over a saved /snap?all=1 and /d (and /log):
         no card-host, no GPU. Prints findings and metrics; exit 0/1.
critique Prints the vision-model request for a report: the rubric, the
         prompt, the answer's JSON schema, and per size the PNG (path, or
         base64 with --inline), the widget snapshot and the findings. The
         model call is the caller's.

Defaults may also come from card-studio.json next to the binary:
  {\"card_host\": \"…\", \"kit\": \"…\", \"out_dir\": \"…\"}
";

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let code = match argv.first().map(String::as_str) {
        None | Some("-h" | "--help" | "help") => {
            print!("{USAGE}");
            0
        }
        Some("card_render" | "card_critique_payload") => skill(&argv[0]),
        Some(cmd) => match run(cmd, &argv[1..]) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("card-studio: {e}");
                if e.starts_with("usage:") {
                    eprint!("\n{USAGE}");
                }
                2
            }
        },
    };
    std::process::exit(code);
}

/// `card-studio.json` next to the binary: defaults for a skill install,
/// where the environment is filtered.
#[derive(Default)]
struct Config {
    card_host: Option<PathBuf>,
    kit: Option<PathBuf>,
    out_dir: Option<PathBuf>,
}

fn config() -> Config {
    let Some(path) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join("card-studio.json")))
    else {
        return Config::default();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Config::default();
    };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    let p = |k: &str| v[k].as_str().map(PathBuf::from);
    Config {
        card_host: p("card_host"),
        kit: p("kit"),
        out_dir: p("out_dir"),
    }
}

struct Flags {
    values: Vec<(String, String)>,
    switches: Vec<String>,
}

impl Flags {
    fn parse(args: &[String], valued: &[&str], switches: &[&str]) -> Result<Flags, String> {
        let mut flags = Flags {
            values: Vec::new(),
            switches: Vec::new(),
        };
        let mut it = args.iter();
        while let Some(arg) = it.next() {
            let (name, inline) = match arg.split_once('=') {
                Some((n, v)) if n.starts_with("--") => (n.to_string(), Some(v.to_string())),
                _ => (arg.clone(), None),
            };
            if valued.contains(&name.as_str()) {
                let value = inline
                    .or_else(|| it.next().cloned())
                    .filter(|v| !v.is_empty());
                flags.values.push((
                    name.clone(),
                    value.ok_or(format!("usage: {name} needs a value"))?,
                ));
            } else if switches.contains(&name.as_str()) && inline.is_none() {
                flags.switches.push(name);
            } else {
                return Err(format!("usage: unexpected argument {arg:?}"));
            }
        }
        Ok(flags)
    }
    fn one(&self, name: &str) -> Option<&str> {
        self.values
            .iter()
            .rev()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
    fn all(&self, name: &str) -> Vec<&str> {
        self.values
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
            .collect()
    }
    fn on(&self, name: &str) -> bool {
        self.switches.iter().any(|s| s == name)
    }
}

fn run(cmd: &str, args: &[String]) -> Result<i32, String> {
    match cmd {
        "render" => {
            let f = Flags::parse(
                args,
                &[
                    "--card",
                    "--data",
                    "--size",
                    "--out",
                    "--card-host",
                    "--kit",
                    "--timeout",
                ],
                &["--no-probe"],
            )?;
            let card = f.one("--card").ok_or("usage: render needs --card")?;
            let sizes: Vec<String> = f.all("--size").into_iter().map(str::to_string).collect();
            let opts = render_options(
                card,
                f.one("--data"),
                &sizes,
                f.one("--out"),
                f.one("--card-host"),
                f.one("--kit"),
                f.one("--timeout")
                    .map(|t| {
                        t.parse::<u64>()
                            .map_err(|_| format!("usage: --timeout {t:?}"))
                    })
                    .transpose()?,
                !f.on("--no-probe"),
            )?;
            let report = render::render(&opts)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
            );
            eprintln!(
                "card-studio: wrote {}",
                opts.out_dir.join("report.json").display()
            );
            Ok(if report.summary.pass { 0 } else { 1 })
        }
        "check" => {
            let f = Flags::parse(args, &["--snap", "--tree", "--log", "--size"], &[])?;
            let read = |p: &str| std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"));
            let snap: serde_json::Value =
                serde_json::from_str(&read(f.one("--snap").ok_or("usage: check needs --snap")?)?)
                    .map_err(|e| e.to_string())?;
            let tree = read(f.one("--tree").ok_or("usage: check needs --tree")?)?;
            let log: serde_json::Value = match f.one("--log") {
                Some(p) => serde_json::from_str(&read(p)?).map_err(|e| e.to_string())?,
                None => serde_json::Value::Null,
            };
            let capture = match log {
                // A saved `<size>.log.json` is a bare array of lines.
                serde_json::Value::Array(lines) => {
                    Capture::from_parts(snap, tree, &serde_json::json!({"l": lines}))
                }
                other => Capture::from_parts(snap, tree, &other),
            };
            let size = TargetSize::parse(f.one("--size").unwrap_or("phone"))?;
            let checked = checks::check(&capture, &size, None);
            let summary = Summary::of(&checked.findings);
            let out = serde_json::json!({
                "size": size.name, "viewport": checked.viewport.map(|v| [v.w, v.h]),
                "findings": checked.findings, "metrics": checked.metrics, "summary": summary,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?
            );
            Ok(if summary.pass { 0 } else { 1 })
        }
        "critique" => {
            let f = Flags::parse(
                args,
                &["--report", "--rubric", "--size", "--out"],
                &["--inline"],
            )?;
            let report_path = f.one("--report").ok_or("usage: critique needs --report")?;
            let rubric_path = f.one("--rubric").ok_or("usage: critique needs --rubric")?;
            let payload = critique_payload(
                Path::new(report_path),
                Path::new(rubric_path),
                f.all("--size").into_iter().map(str::to_string).collect(),
                f.on("--inline"),
            )?;
            let text = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
            match f.one("--out") {
                Some(out) => std::fs::write(out, text + "\n").map_err(|e| format!("{out}: {e}"))?,
                None => println!("{text}"),
            }
            Ok(0)
        }
        other => Err(format!("usage: unknown command {other:?}")),
    }
}

#[allow(clippy::too_many_arguments)]
fn render_options(
    card: &str,
    data: Option<&str>,
    sizes: &[String],
    out: Option<&str>,
    card_host: Option<&str>,
    kit: Option<&str>,
    timeout: Option<u64>,
    probe: bool,
) -> Result<RenderOptions, String> {
    let cfg = config();
    let out_dir = out
        .map(PathBuf::from)
        .or(cfg.out_dir)
        .unwrap_or_else(|| PathBuf::from("card-studio-out"));
    let card_host = card_host
        .map(PathBuf::from)
        .or(cfg.card_host)
        .unwrap_or_else(render::default_card_host);
    let mut opts = RenderOptions::new(PathBuf::from(card), card_host, out_dir);
    opts.data = data.map(PathBuf::from);
    opts.kit = kit.map(PathBuf::from).or(cfg.kit);
    if !sizes.is_empty() {
        opts.sizes = sizes
            .iter()
            .map(|s| TargetSize::parse(s))
            .collect::<Result<_, _>>()?;
    }
    if let Some(t) = timeout {
        opts.timeout = Duration::from_secs(t.max(1));
    }
    opts.probe = probe;
    Ok(opts)
}

fn critique_payload(
    report_path: &Path,
    rubric_path: &Path,
    sizes: Vec<String>,
    inline: bool,
) -> Result<serde_json::Value, String> {
    let report: Report = serde_json::from_str(
        &std::fs::read_to_string(report_path)
            .map_err(|e| format!("{}: {e}", report_path.display()))?,
    )
    .map_err(|e| format!("{}: {e}", report_path.display()))?;
    let rubric = std::fs::read_to_string(rubric_path)
        .map_err(|e| format!("{}: {e}", rubric_path.display()))?;
    let dir = report_path.parent().unwrap_or(Path::new("."));
    critique::payload(
        &report,
        dir,
        &rubric,
        &CritiqueOptions {
            sizes,
            inline_images: inline,
        },
    )
}

/// The octos skill protocol: JSON in on stdin, `{success, output}` out.
fn skill(tool: &str) -> i32 {
    let mut raw = String::new();
    let result = std::io::stdin()
        .read_to_string(&mut raw)
        .map_err(|e| format!("stdin: {e}"))
        .and_then(|_| {
            serde_json::from_str::<serde_json::Value>(if raw.trim().is_empty() {
                "{}"
            } else {
                &raw
            })
            .map_err(|e| format!("input: {e}"))
        })
        .and_then(|input| skill_call(tool, &input));
    let (success, output) = match result {
        Ok(output) => (true, output),
        Err(e) => (false, e),
    };
    println!(
        "{}",
        serde_json::json!({"success": success, "output": output})
    );
    0
}

fn skill_call(tool: &str, input: &serde_json::Value) -> Result<String, String> {
    let s = |k: &str| input[k].as_str();
    match tool {
        "card_render" => {
            let card = s("card").ok_or("card_render needs \"card\"")?;
            let sizes: Vec<String> = input["sizes"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            // Data may come inline; it is written next to the report.
            let out = s("out_dir")
                .map(PathBuf::from)
                .or(config().out_dir)
                .unwrap_or_else(|| {
                    std::env::temp_dir().join(format!("card-studio-{}", std::process::id()))
                });
            std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
            let data = match (&input["data"], s("data_path")) {
                (serde_json::Value::Object(_), _) => {
                    let p = out.join("data.json");
                    std::fs::write(&p, input["data"].to_string()).map_err(|e| e.to_string())?;
                    Some(p.to_string_lossy().into_owned())
                }
                (_, Some(p)) => Some(p.to_string()),
                _ => None,
            };
            let opts = render_options(
                card,
                data.as_deref(),
                &sizes,
                out.to_str(),
                s("card_host"),
                s("kit"),
                None,
                true,
            )?;
            let report = render::render(&opts)?;
            let mut v = serde_json::to_value(&report).map_err(|e| e.to_string())?;
            v["report_path"] = serde_json::Value::String(
                opts.out_dir
                    .join("report.json")
                    .to_string_lossy()
                    .into_owned(),
            );
            Ok(v.to_string())
        }
        "card_critique_payload" => {
            let report = s("report").ok_or("card_critique_payload needs \"report\"")?;
            let rubric = s("rubric").ok_or("card_critique_payload needs \"rubric\"")?;
            let sizes: Vec<String> = input["sizes"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let payload = critique_payload(
                Path::new(report),
                Path::new(rubric),
                sizes,
                input["inline"].as_bool().unwrap_or(false),
            )?;
            Ok(payload.to_string())
        }
        other => Err(format!("unknown tool {other}")),
    }
}
