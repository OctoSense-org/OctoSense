//! OctoSense on the desktop: the shell (crates/shell) and nothing else. The
//! window manager, hosting and phone layer are `octosense_shell`; this
//! package is the entry point and the desktop packaging (config/, upstream/,
//! scripts/, resources/android).
use octosense_shell::makepad_widgets::*;
use octosense_shell::App;

octosense_shell::octosense_main!();
