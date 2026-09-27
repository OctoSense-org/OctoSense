//! File-meta cache. Wire bytes are downloaded by `octos-app-transport` from
//! `/api/files/{handle}`; this slice holds only the metadata the UI needs to
//! pick a viewer (`FileKind`) and render filename/size.
//!
//! `FileHandle` is intentionally a local newtype — taking it from
//! `octos-app-transport` would form a dep cycle (`store → transport → store`).
//! Both crates wrap the same opaque pre-signed handle string.

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FileHandle(pub String);

impl FileHandle { pub fn as_str(&self) -> &str { &self.0 } }
impl From<String> for FileHandle { fn from(s: String) -> Self { Self(s) } }
impl From<&str> for FileHandle { fn from(s: &str) -> Self { Self(s.to_owned()) } }
impl fmt::Display for FileHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0) }
}

/// Viewer hint. Mirrors W04 § 6 — image album / audio / video / markdown /
/// pdf / generic. Kept narrow so the UI's match is exhaustive; unknown types
/// land in `Other` and download as bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    Image,
    Audio,
    Video,
    Markdown,
    Pdf,
    Other,
}

impl FileKind {
    /// Best-effort kind from a MIME type and (optionally) a filename.
    pub fn from_mime(content_type: &str, name: &str) -> Self {
        let ct = content_type.to_ascii_lowercase();
        if ct.starts_with("image/") { return Self::Image; }
        if ct.starts_with("audio/") { return Self::Audio; }
        if ct.starts_with("video/") { return Self::Video; }
        if ct == "application/pdf" { return Self::Pdf; }
        if ct == "text/markdown" || name.to_ascii_lowercase().ends_with(".md") {
            return Self::Markdown;
        }
        Self::Other
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileMeta {
    pub handle: FileHandle,
    pub content_type: String,
    pub size_bytes: u64,
    pub name: String,
    pub kind: FileKind,
}

impl FileMeta {
    pub fn new(
        handle: FileHandle, content_type: impl Into<String>,
        size_bytes: u64, name: impl Into<String>,
    ) -> Self {
        let content_type = content_type.into();
        let name = name.into();
        let kind = FileKind::from_mime(&content_type, &name);
        Self { handle, content_type, size_bytes, name, kind }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_from_mime_image_audio_video_pdf() {
        assert_eq!(FileKind::from_mime("image/png", "x.png"), FileKind::Image);
        assert_eq!(FileKind::from_mime("audio/mpeg", "song.mp3"), FileKind::Audio);
        assert_eq!(FileKind::from_mime("video/mp4", "v.mp4"), FileKind::Video);
        assert_eq!(FileKind::from_mime("application/pdf", "doc.pdf"), FileKind::Pdf);
    }

    #[test]
    fn kind_falls_back_to_filename_for_markdown() {
        // Some servers return text/plain for .md.
        assert_eq!(FileKind::from_mime("text/plain", "NOTES.md"), FileKind::Markdown);
        assert_eq!(FileKind::from_mime("text/markdown", "x.md"), FileKind::Markdown);
        assert_eq!(FileKind::from_mime("text/plain", "x.txt"), FileKind::Other);
    }

    #[test]
    fn file_meta_resolves_kind_on_construction() {
        let m = FileMeta::new(FileHandle::from("h-1"), "image/jpeg", 12_345, "a.jpg");
        assert_eq!(m.kind, FileKind::Image);
        assert_eq!(m.size_bytes, 12_345);
        assert_eq!(m.handle.as_str(), "h-1");
    }
}
