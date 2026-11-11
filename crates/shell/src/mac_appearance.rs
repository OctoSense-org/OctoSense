//! A Mac's appearance and its desktop picture. Auto follows the Mac's own
//! setting; Light and Dark are the person's, and stay. Each choice is one
//! small file in the shell's home.

use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Choice {
    #[default]
    Auto,
    Light,
    Dark,
}

impl Choice {
    fn parse(text: &str) -> Choice {
        match text.trim() {
            "light" => Choice::Light,
            "dark" => Choice::Dark,
            _ => Choice::Auto,
        }
    }

    pub(crate) fn word(self) -> &'static str {
        match self {
            Choice::Auto => "auto",
            Choice::Light => "light",
            Choice::Dark => "dark",
        }
    }

    fn path() -> std::path::PathBuf {
        octosense::paths::home().join("appearance")
    }

    pub(crate) fn load() -> Choice {
        std::fs::read_to_string(Self::path()).map(|text| Choice::parse(&text)).unwrap_or_default()
    }

    fn save(self) {
        if let Err(error) = std::fs::create_dir_all(octosense::paths::home()).and_then(|_| std::fs::write(Self::path(), self.word())) {
            log!("wm: could not keep the appearance: {error}");
        }
    }

    /// Dark or not, with the Mac's own setting as far as it is known.
    pub(crate) fn dark(self, mac_dark: Option<bool>) -> bool {
        match self {
            Choice::Auto => mac_dark.unwrap_or(false),
            Choice::Light => false,
            Choice::Dark => true,
        }
    }
}

/// A Mac's desktop picture. Abyssal Currents changes with the appearance, as a
/// Mac's dynamic pictures do; Tokyo Night stays as it is; a colour fills the desk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Picture {
    #[default]
    AbyssalCurrents,
    TokyoNight,
    Blue,
    Teal,
    Purple,
    Graphite,
}

#[cfg_attr(mobile_only, allow(dead_code))]
impl Picture {
    pub(crate) const ALL: [Picture; 6] =
        [Picture::AbyssalCurrents, Picture::TokyoNight, Picture::Blue, Picture::Teal, Picture::Purple, Picture::Graphite];

    pub(crate) fn id(self) -> &'static str {
        match self {
            Picture::AbyssalCurrents => "abyssal-currents",
            Picture::TokyoNight => "tokyo-night",
            Picture::Blue => "blue",
            Picture::Teal => "teal",
            Picture::Purple => "purple",
            Picture::Graphite => "graphite",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Picture::AbyssalCurrents => "Abyssal Currents",
            Picture::TokyoNight => "Tokyo Night",
            Picture::Blue => "Blue",
            Picture::Teal => "Teal",
            Picture::Purple => "Purple",
            Picture::Graphite => "Graphite",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Picture> {
        Picture::ALL.into_iter().find(|picture| picture.id() == text.trim())
    }

    /// The colour that fills the desk, for a colour.
    pub(crate) fn colour(self) -> Option<(u8, u8, u8)> {
        match self {
            Picture::Blue => Some((28, 84, 150)),
            Picture::Teal => Some((22, 104, 108)),
            Picture::Purple => Some((88, 62, 136)),
            Picture::Graphite => Some((72, 72, 78)),
            Picture::AbyssalCurrents | Picture::TokyoNight => None,
        }
    }

    fn path() -> std::path::PathBuf {
        octosense::paths::home().join("desktop-picture")
    }

    pub(crate) fn load() -> Picture {
        std::fs::read_to_string(Self::path()).ok().and_then(|text| Picture::parse(&text)).unwrap_or_default()
    }

    fn save(self) {
        if let Err(error) = std::fs::create_dir_all(octosense::paths::home()).and_then(|_| std::fs::write(Self::path(), self.id())) {
            log!("wm: could not keep the desktop picture: {error}");
        }
    }
}

impl App {
    /// Keep what the person chose, and answer whether it is dark.
    pub(super) fn keep_mac_appearance(&mut self, choice: Choice) -> bool {
        self.mac_appearance = choice;
        choice.save();
        choice.dark(self.status_sample.mac_dark.or_else(shell::bar::sample_mac_dark))
    }

    /// The person chose in a menu.
    pub(super) fn choose_mac_appearance(&mut self, cx: &mut Cx, choice: Choice) {
        let dark = self.keep_mac_appearance(choice);
        if self.state_mut().style.dark != dark {
            self.toggle_desktop_appearance(cx);
        }
    }

    /// The person chose a desktop picture in System Settings.
    #[cfg_attr(mobile_only, allow(dead_code))]
    pub(super) fn choose_mac_picture(&mut self, cx: &mut Cx, picture: Picture) {
        self.mac_picture = picture;
        picture.save();
        let (style, dark) = (self.state_mut().style.target, self.state_mut().style.dark);
        if style == desktop::DesktopStyle::Macos {
            self.apply_desk_ground(cx, style, dark);
            self.redraw_all(cx);
        }
    }

    /// The Mac's own setting, as the sampler last read it.
    pub(super) fn follow_mac_appearance(&mut self, cx: &mut Cx, mac_dark: bool) {
        if self.mac_appearance == Choice::Auto
            && self.state_mut().style.target == desktop::DesktopStyle::Macos
            && self.state_mut().style.dark != mac_dark
        {
            self.toggle_desktop_appearance(cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_choice_reads_back_and_anything_else_is_auto() {
        for choice in [Choice::Auto, Choice::Light, Choice::Dark] {
            assert_eq!(Choice::parse(choice.word()), choice);
        }
        assert_eq!(Choice::parse("dark\n"), Choice::Dark);
        assert_eq!(Choice::parse("sepia"), Choice::Auto);
        assert_eq!(Choice::parse(""), Choice::Auto);
    }

    #[test]
    fn a_picture_reads_back_and_anything_else_is_abyssal_currents() {
        for picture in Picture::ALL {
            assert_eq!(Picture::parse(picture.id()), Some(picture));
        }
        assert_eq!(Picture::parse("teal\n"), Some(Picture::Teal));
        assert_eq!(Picture::parse("sunset"), None);
        assert_eq!(Picture::default(), Picture::AbyssalCurrents);
        // Pictures draw; colours fill.
        assert_eq!(Picture::ALL.iter().filter(|picture| picture.colour().is_none()).count(), 2);
    }

    #[test]
    fn auto_follows_the_mac_and_the_others_do_not() {
        assert!(Choice::Auto.dark(Some(true)));
        assert!(!Choice::Auto.dark(Some(false)));
        assert!(!Choice::Auto.dark(None));
        assert!(Choice::Dark.dark(Some(false)));
        assert!(!Choice::Light.dark(Some(true)));
    }
}
