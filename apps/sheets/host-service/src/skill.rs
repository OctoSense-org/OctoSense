//! The system agent's skill for the sheet engine (gridcraft, ADR 0013): the
//! files of `skill/`, embedded so a build ships the skill that matches its
//! engine. The shell installs it into the kernel's skills dir
//! (`crates/shell/src/system_chat/skills.rs`). Its generated reference,
//! `functions.md`, is kept current by `tests/skill.rs`.

/// The skill's name: its folder in the kernel's skills dir and its
/// `SKILL.md` frontmatter `name`.
pub const NAME: &str = "sheet-engine";

/// Its files, as (path in the skill's folder, content).
pub const FILES: &[(&str, &str)] = &[
    ("SKILL.md", include_str!("../skill/SKILL.md")),
    ("functions.md", include_str!("../skill/functions.md")),
];
