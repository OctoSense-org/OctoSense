//! Run one card bundle under its own policy.
//!
//! ```sh
//! card-host --bundle <dir> [--app-data <dir>] [--allow-unsigned] [--stamp] [--system]
//!           [--static <prefix>=<dir>]...
//! card-host --help
//! ```
//!
//! The bundle is a directory holding `manifest.json`, `page.card`,
//! `page.data.json` and a `kit/` directory, or a script app's `main.splash`.
//! `--stamp` rewrites the manifest's digest to match the directory, which is
//! what a build step does before signing; without it a bundle whose bytes
//! changed is refused. `--system` admits the bundle as a system app is
//! admitted (by digest, under `HostLimits::system`), for developing one; an
//! empty digest in its manifest is filled in memory, as the build fills it in
//! the packed copy. `--static photos=<dir>` serves `<dir>`'s files at
//! `photos/...` from memory, the way a shell serves a system app's
//! compiled-in artwork.
//!
//! The command line is checked before the app starts: `--help` prints the
//! usage and exits 0, an unknown option prints it and exits 2, and neither
//! opens a window.
mod args;
// `app_main!` also emits a `main` of its own in there; this one is the entry.
#[allow(dead_code)]
mod host;

fn main() {
    match args::parse(std::env::args().skip(1)) {
        Ok(args) => host::run(args),
        Err(args::ArgsError::Help) => print!("{}", args::USAGE),
        Err(args::ArgsError::Invalid(problem)) => {
            eprint!("card-host: {problem}\n\n{}", args::USAGE);
            std::process::exit(2);
        }
    }
}
