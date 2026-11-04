//! The command line, parsed before anything else starts.
//!
//! `card-host --help` used to open a window, refuse `./manifest.json` and
//! keep running (App Hub#9): the flags were only looked up, never checked, so
//! an agent asking for usage got a stuck process instead. Everything is now
//! checked up front: `-h`/`--help` prints [`USAGE`] and exits 0, anything
//! unknown prints it and exits 2, and only a clean parse starts the app.
use std::path::PathBuf;

pub const USAGE: &str = "\
card-host: run one OctoSense bundle (a card or a script app) under the policy
its manifest resolves to: admit, resolve, apply, then evaluate.

Usage:
  card-host [--bundle <dir>] [--app-data <dir>] [--allow-unsigned] [--stamp]
            [--system] [--static <prefix>=<dir>]... [--size <w>x<h>]
  card-host -h | --help

Options:
  --bundle <dir>           The bundle to run. Default: the current directory.
  --app-data <dir>         Where the app's storage jail is made, at
                           <dir>/<app id>/; host services keep their state in
                           <dir>/.host/. Default: $TMPDIR/octosense-card-apps.
  --allow-unsigned         Admit a manifest with no signature. A signed
                           manifest is still refused: card-host verifies no
                           publisher keys.
  --stamp                  Rewrite the manifest's integrity.bundle_blake3 to
                           match the directory before admitting. Refuses any
                           publisher signature or GitHub signing metadata.
  --system                 Admit as a system app: by digest only, under the
                           system ceilings. An empty digest is filled in memory.
  --static <prefix>=<dir>  Serve <dir>'s files at <prefix>/... from memory.
                           Repeatable.
  --size <w>x<h>           The window's inner size in layout points, e.g.
                           390x844. Default: the manifest's window size,
                           else 412x892. card-studio renders a card at
                           several sizes with it.
  -h, --help               Print this and exit.

Every L0 card logs one `card-host: realize {json}` line: the lint result
(valid, level, diagnostics), the realize report (nodes, truncated,
diagnostics), each declared source's $state and which lowering drew it.

Remote control: set MAKEPAD_REMOTE=<port> (or pass --remote [<port>]) for a
localhost HTTP control surface; GET / on it lists the routes. See
docs/DEVELOPMENT.md in OctoSense-App-Hub.
";

#[derive(Debug, PartialEq)]
pub struct Args {
    pub bundle: PathBuf,
    pub app_data: PathBuf,
    pub allow_unsigned: bool,
    pub stamp: bool,
    pub system: bool,
    pub statics: Vec<(String, PathBuf)>,
    /// `--size`: the window's inner size in layout points.
    pub size: Option<(f64, f64)>,
}

/// Parse `<w>x<h>` (layout points, both positive and finite).
pub fn parse_size(text: &str) -> Option<(f64, f64)> {
    let (w, h) = text.split_once(['x', 'X'])?;
    let (w, h) = (w.trim().parse::<f64>().ok()?, h.trim().parse::<f64>().ok()?);
    (w.is_finite() && h.is_finite() && w >= 1.0 && h >= 1.0).then_some((w, h))
}

#[derive(Debug, PartialEq)]
pub enum ArgsError {
    /// `-h` or `--help`: print the usage and exit 0.
    Help,
    /// Anything else wrong: say what, print the usage, exit 2.
    Invalid(String),
}

/// Flags the Makepad runtime reads from argv itself. They are passed through
/// untouched, so `card-host --remote 8151` keeps working.
fn is_runtime_flag(arg: &str) -> bool {
    const EXACT: &[&str] = &["--remote", "--stdin-loop", "--hot", "--devtools", "--no-draw", "--draws"];
    const PREFIXES: &[&str] = &[
        "--remote=",
        "--remote-title-tag=",
        "--draws=",
        "--linux-backend=",
        "--mode=",
        "-mode=",
        "-scale=",
        // macOS adds a process serial number when launched from Finder.
        "-psn_",
    ];
    EXACT.contains(&arg) || PREFIXES.iter().any(|p| arg.starts_with(p))
}

/// Parse the arguments after the program name.
pub fn parse<I: IntoIterator<Item = String>>(argv: I) -> Result<Args, ArgsError> {
    let mut args = Args {
        bundle: PathBuf::from("."),
        app_data: std::env::temp_dir().join("octosense-card-apps"),
        allow_unsigned: false,
        stamp: false,
        system: false,
        statics: Vec::new(),
        size: None,
    };
    let mut argv = argv.into_iter().peekable();
    while let Some(arg) = argv.next() {
        // `--flag=value` and `--flag value` both work for the valued flags.
        let (flag, inline) = match arg.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") => (flag.to_string(), Some(value.to_string())),
            _ => (arg.clone(), None),
        };
        let mut value = |name: &str| -> Result<String, ArgsError> {
            match inline.clone().or_else(|| argv.next()) {
                Some(v) if !v.is_empty() => Ok(v),
                _ => Err(ArgsError::Invalid(format!("{name} needs a value"))),
            }
        };
        match flag.as_str() {
            "-h" | "--help" => return Err(ArgsError::Help),
            "--bundle" => args.bundle = PathBuf::from(value("--bundle")?),
            "--app-data" => args.app_data = PathBuf::from(value("--app-data")?),
            "--static" => {
                let mount = value("--static")?;
                let Some((prefix, dir)) = mount.split_once('=') else {
                    return Err(ArgsError::Invalid(format!("--static wants <prefix>=<dir>, got {mount:?}")));
                };
                args.statics.push((prefix.trim_matches('/').to_string(), PathBuf::from(dir)));
            }
            "--size" => {
                let text = value("--size")?;
                args.size = Some(
                    parse_size(&text).ok_or_else(|| ArgsError::Invalid(format!("--size wants <w>x<h>, got {text:?}")))?,
                );
            }
            "--allow-unsigned" | "--stamp" | "--system" if inline.is_some() => {
                return Err(ArgsError::Invalid(format!("{flag} takes no value")));
            }
            "--allow-unsigned" => args.allow_unsigned = true,
            "--stamp" => args.stamp = true,
            "--system" => args.system = true,
            _ if is_runtime_flag(&arg) => {
                // `--remote` and `--draws` may be followed by their value.
                if arg == "--remote" {
                    if argv.peek().is_some_and(|n| n.parse::<u16>().is_ok() || n.contains(':')) {
                        argv.next();
                    }
                } else if arg == "--draws" && argv.peek().is_some_and(|n| n.parse::<usize>().is_ok()) {
                    argv.next();
                }
            }
            _ if arg.starts_with('-') => return Err(ArgsError::Invalid(format!("unknown option {arg}"))),
            _ => return Err(ArgsError::Invalid(format!("unexpected argument {arg:?}"))),
        }
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_strs(argv: &[&str]) -> Result<Args, ArgsError> {
        parse(argv.iter().map(|s| s.to_string()))
    }

    #[test]
    fn help_in_any_position_asks_for_usage() {
        for argv in [&["--help"][..], &["-h"], &["--bundle", "x", "--help"], &["--stamp", "-h", "--system"]] {
            assert_eq!(parse_strs(argv), Err(ArgsError::Help), "{argv:?}");
        }
    }

    #[test]
    fn no_arguments_is_the_documented_default() {
        let args = parse_strs(&[]).unwrap();
        assert_eq!(args.bundle, PathBuf::from("."));
        assert_eq!(args.app_data, std::env::temp_dir().join("octosense-card-apps"));
        assert!(!args.allow_unsigned && !args.stamp && !args.system);
        assert!(args.statics.is_empty());
        assert_eq!(args.size, None);
    }

    #[test]
    fn every_documented_flag_parses() {
        let args = parse_strs(&[
            "--bundle", "app/bundle", "--app-data=/tmp/state", "--allow-unsigned", "--stamp", "--system",
            "--static", "/photos/=art", "--static=icons=icons", "--size", "390x844",
        ])
        .unwrap();
        assert_eq!(
            args,
            Args {
                bundle: PathBuf::from("app/bundle"),
                app_data: PathBuf::from("/tmp/state"),
                allow_unsigned: true,
                stamp: true,
                system: true,
                statics: vec![("photos".into(), PathBuf::from("art")), ("icons".into(), PathBuf::from("icons"))],
                size: Some((390.0, 844.0)),
            }
        );
    }

    #[test]
    fn unknown_or_malformed_arguments_are_refused() {
        for argv in [
            &["--bogus"][..],
            &["-x"],
            &["stray"],
            &["--bundle"],
            &["--bundle="],
            &["--app-data"],
            &["--static", "no-equals-sign"],
            &["--stamp=yes"],
            &["--size", "390"],
            &["--size=0x10"],
            &["--size", "wide"],
        ] {
            assert!(matches!(parse_strs(argv), Err(ArgsError::Invalid(_))), "{argv:?}");
        }
    }

    #[test]
    fn the_runtime_flags_pass_through() {
        let args = parse_strs(&[
            "--remote", "8151", "--bundle", "b", "--remote=127.0.0.1:9000", "--remote-title-tag=agent",
            "--stdin-loop", "--hot", "--devtools", "--draws", "3", "-psn_0_12345",
        ])
        .unwrap();
        assert_eq!(args.bundle, PathBuf::from("b"));
        // A bare --remote picks a free port; what follows is ours again.
        assert_eq!(parse_strs(&["--remote", "--stamp"]).unwrap().stamp, true);
    }

    #[test]
    fn usage_names_every_flag() {
        for flag in ["--bundle", "--app-data", "--allow-unsigned", "--stamp", "--system", "--static", "--size", "--help"] {
            assert!(USAGE.contains(flag), "{flag}");
        }
    }
}
