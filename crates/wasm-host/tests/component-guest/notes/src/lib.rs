//! A test component: ordinary Rust and an unmodified crate, exported
//! through WIT. Built for wasm32-wasip2 with no OctoSense-specific code.
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};

wit_bindgen::generate!({ world: "notes", path: "wit" });

struct Notes;

static COUNT: AtomicU32 = AtomicU32::new(0);

impl Guest for Notes {
    fn to_html(markdown: String) -> String {
        let mut out = String::new();
        pulldown_cmark::html::push_html(&mut out, Parser::new(&markdown));
        out
    }

    fn analyze(markdown: String) -> Stats {
        let mut headings = Vec::new();
        let mut current: Option<String> = None;
        for event in Parser::new(&markdown) {
            match event {
                Event::Start(Tag::Heading { level, .. }) if level <= HeadingLevel::H3 => {
                    current = Some(String::new())
                }
                Event::Text(text) => {
                    if let Some(h) = current.as_mut() {
                        h.push_str(&text)
                    }
                }
                Event::End(TagEnd::Heading(_)) => headings.extend(current.take()),
                _ => {}
            }
        }
        Stats {
            words: markdown.split_whitespace().count() as u32,
            lines: markdown.lines().count() as u32,
            headings,
        }
    }

    fn save_html(markdown: String, path: String) -> Result<u64, String> {
        let html = Self::to_html(markdown);
        std::fs::write(&path, &html).map_err(|e| e.to_string())?;
        Ok(html.len() as u64)
    }

    fn read_file(path: String) -> Result<String, String> {
        std::fs::read_to_string(&path).map_err(|e| e.to_string())
    }

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    fn random_u64() -> u64 {
        let mut bytes = [0u8; 8];
        getrandom::fill(&mut bytes).expect("randomness");
        u64::from_le_bytes(bytes)
    }

    fn count() -> u32 {
        COUNT.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn spin() {
        loop {
            std::hint::black_box(());
        }
    }

    fn grow(megabytes: u32) -> u32 {
        let v = vec![1u8; (megabytes as usize) << 20];
        std::hint::black_box(&v);
        (v.len() >> 20) as u32
    }

    fn echo_bytes(data: Vec<u8>) -> Vec<u8> {
        data
    }

    fn append_text(path: String, text: String) -> Result<u64, String> {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        Ok(file.metadata().map_err(|e| e.to_string())?.len())
    }

    fn set_len(path: String, len: u64) -> Result<(), String> {
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .and_then(|file| file.set_len(len))
            .map_err(|e| e.to_string())
    }

    fn delete_file(path: String) -> Result<(), String> {
        std::fs::remove_file(&path).map_err(|e| e.to_string())
    }

    fn file_info(path: String) -> Result<FileInfo, String> {
        let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
        Ok(FileInfo {
            byte_count: meta.len(),
            kind: if meta.is_file() {
                EntryKind::RegularFile
            } else if meta.is_dir() {
                EntryKind::Directory
            } else {
                EntryKind::Other
            },
        })
    }
}

export!(Notes);
