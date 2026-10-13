//! The import guard (SERVICE.md "Import guard", decision 4): the service's
//! own check of a file, before the engine sees a byte of it, on every path
//! that opens a file (`deck.open`, `deck.cover`, the v1 file methods, and
//! through them the agent's `deck.info` and `deck.run`).
//!
//! deckcraft sizes what it allocates for a zip from the zip's own headers
//! (#448): its pptx reader takes up to 50,000 entries, 512 MiB a part and
//! 2 GiB in all, its native reader 2 GiB an entry, and neither checks that
//! an entry inflates to no more than its directory says. A hostile file
//! could make it allocate gigabytes on the shell's UI thread and abort the
//! process. So, before the engine reads anything:
//!
//! - **A zip** (`.pptx`, `.deckcraft`): its central directory is read with
//!   bounds, by this module and not by the zip library: at most
//!   [`MAX_ENTRIES`] entries, names of at most [`MAX_NAME_BYTES`], no entry
//!   declaring over [`MAX_ENTRY_BYTES`] uncompressed, at most
//!   [`MAX_TOTAL_BYTES`] declared in all, and no entry past [`RATIO_GRACE`]
//!   compressing more than [`MAX_RATIO`] to 1 (Apache POI's zip-bomb rule).
//!   Over any of those is `held:`.
//! - **Read alike.** The guard only checks what the engine will read if it
//!   reads the zip exactly as the engine's zip reader does (the `zip` crate
//!   behind deckcraft's package and native readers), so it takes the zip
//!   only in a shape no two readers can take differently ([`directory`]):
//!   the last end record, its comment ending the file, with no second end
//!   record after it; its entry counts agreeing (on this disk and in all,
//!   in the ZIP64 record too); the directory filling exactly the space it
//!   declares, up to the end record or the ZIP64 record, every header in
//!   it and every entry before it, the first at the very start of the file
//!   (nothing prepended); every extra field read as that reader reads it.
//!   Then that reader itself reads the directory ([`read_alike`]), and
//!   anything it sees differently (where the zip starts, where its
//!   directory is, how many entries, an entry's name, sizes, offsets,
//!   method) refuses the file. Any of this, a directory that does not
//!   parse, an entry outside the file, an encrypted entry or one
//!   compressed in a way the engine does not read is `damaged:`.
//! - Then the guard says what it is, and its verdict picks the engine's
//!   reader (`crate::engine_read`): a native deck by deckcraft's own marker
//!   (`deckcraft_format::sniff`, which the engine tries first), else an
//!   Office package by its `[Content_Types].xml` entry in the directory the
//!   guard verified (the engine's `deckcraft_pptx::sniff` looks in a
//!   package's first 4 KiB only, which no LibreOffice package meets). Any
//!   other zip (`.key`, `.odp`) is `unsupported:`.
//! - Then every entry is inflated, bounded, into a scratch buffer (nothing
//!   is kept but its first bytes): one that inflates past its declared size
//!   is `damaged:`.
//! - Audio and video parts pass like any other part. The engine's readers
//!   keep them as bytes (a package's media with its own poster picture),
//!   and nothing the service runs decodes them: deckcraft's decoders
//!   (`deckcraft-media`) are a dependency of its engine only, reached only
//!   through `media.info`, `media.posterFrame`, `insert.audio` and
//!   `insert.video`, which the door holds back (#448). The renderer draws a
//!   clip's poster, or a film or speaker glyph, and the writers copy its
//!   bytes. Before the engine reads, the declared total of a zip's binary
//!   parts (everything but its XML and relationship parts, and a native
//!   deck's manifest) is held to the session's 128 MiB of media
//!   (`too_large:`, in the session ceiling's words, [`binary_total`]), so
//!   the engine never copies out more; after it reads, every path checks
//!   the deck's media against the same ceiling.
//! - A zip inside the zip is `held:`, except an Office package's embedded object
//!   ([`EMBEDDINGS`]): a chart's workbook or an embedded document is a zip
//!   that neither the service nor the engine ever unzips (deckcraft's only
//!   zip readers are its package and native readers; it reads a chart from
//!   the chart's own cached values, and keeps only an embedded object's
//!   XML and preview). Such a part passes, held like every entry to the
//!   bounds above (entries, name, declared and actual size, ratio,
//!   totals); its own entries are never read, by anyone.
//! - **An OLE compound file** (`D0 CF 11 E0`): one whose directory holds an
//!   `EncryptedPackage` stream is a password-protected Office file
//!   (`protected:`); any other is a legacy `.ppt` or not a presentation at
//!   all (`unsupported:`). Its directory is read with bounds too.
//! - **A PDF** is `unsupported:`, and so is any other file that is not
//!   outline text (`.txt` or `.md`, by name, as the engine reads it).
//!
//! The guard reads only the bytes the service already read (at most the
//! service's 64 MiB a file). Beyond the directory's entries it allocates a
//! 64 KiB inflate buffer and, for an OLE file, a sector table of at most
//! one entry a sector; the engine's zip reader, run on a directory the
//! guard has bounded, holds one record an entry. Engine-side bounds stay
//! tracked in #448.

use std::io::Read;

use crate::codes::{fail, Code};

/// The most entries a zip may have.
pub(crate) const MAX_ENTRIES: usize = 10_000;
/// The longest entry name, in bytes.
pub(crate) const MAX_NAME_BYTES: usize = 1024;
/// The most one entry may declare, uncompressed.
pub(crate) const MAX_ENTRY_BYTES: u64 = 64 << 20;
/// The most a zip's entries may declare together.
pub(crate) const MAX_TOTAL_BYTES: u64 = 256 << 20;
/// The most an entry may compress to 1, once past [`RATIO_GRACE`].
pub(crate) const MAX_RATIO: u64 = 100;
/// Entries up to this size are not held to [`MAX_RATIO`]: a small blank
/// picture or printer setting compresses far better than that.
pub(crate) const RATIO_GRACE: u64 = 100 << 10;
/// The most extensible data a ZIP64 end record may carry beyond its 44
/// bytes of fields: the engine's zip reader allocates all of it, and an
/// honest writer leaves it empty.
pub(crate) const MAX_ZIP64_EXTRA: u64 = 1024;

/// What the engine will be asked to read, once the guard passed the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Format {
    /// deckcraft's own `.deckcraft` zip, by its marker as
    /// `deckcraft_format::sniff` finds it.
    Native,
    /// An Office Open XML package (`.pptx`): a `[Content_Types].xml` entry
    /// in the directory the guard verified.
    Pptx,
    /// Outline text (`.txt`, `.md`).
    Outline,
}

/// The OLE compound file signature: a legacy Office file, or an encrypted
/// Office Open XML one.
const OLE: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

/// Check `bytes`, the file at `name` (storage-relative, only its extension
/// is used), before the engine reads it. `Ok` says how the engine will read
/// it; `Err` is the coded refusal.
pub(crate) fn check(name: &str, bytes: &[u8]) -> Result<Format, String> {
    if bytes.starts_with(&OLE[..4]) {
        return Err(ole(bytes));
    }
    if bytes.starts_with(b"PK") {
        return zip(bytes);
    }
    if bytes.starts_with(b"%PDF") {
        return Err(fail(Code::Unsupported, "this is a PDF, not a presentation the deck engine reads (.pptx or .deckcraft)"));
    }
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".txt") || lower.ends_with(".md") {
        return Ok(Format::Outline);
    }
    Err(fail(Code::Unsupported, "this is not a presentation the deck engine reads (.pptx, .deckcraft, or outline .txt or .md)"))
}

fn held(words: impl std::fmt::Display) -> String {
    fail(Code::Held, words)
}

fn damaged(words: impl std::fmt::Display) -> String {
    fail(Code::Damaged, words)
}

/// An entry's name as an error may show it: printable, and short.
fn shown(name: &str) -> String {
    let clean: String = name.chars().filter(|c| !c.is_control()).collect();
    if clean.chars().count() > 80 {
        format!("{}…", clean.chars().take(79).collect::<String>())
    } else {
        clean
    }
}

/// Little-endian numbers at an offset the file itself may have named: out
/// of range (or overflowing) is `None`, never a panic.
fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at.checked_add(2)?)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at.checked_add(4)?)?.try_into().ok()?))
}

fn u64_at(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(at..at.checked_add(8)?)?.try_into().ok()?))
}

/// One entry of a zip's central directory, as the guard reads it, which is
/// how the engine's zip reader must read it too ([`read_alike`]).
#[derive(Debug)]
struct Entry {
    /// The name as the checks match it and an error shows it.
    name: String,
    /// The name's bytes as stored.
    raw: Vec<u8>,
    flags: u16,
    method: u16,
    compressed: u64,
    declared: u64,
    offset: u64,
}

/// A zip's central directory: its entries, and where it starts.
#[derive(Debug)]
struct Directory {
    entries: Vec<Entry>,
    start: usize,
}

const EOCD: u32 = 0x0605_4b50;
const ZIP64_LOCATOR: u32 = 0x0706_4b50;
const ZIP64_EOCD: u32 = 0x0606_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const LOCAL: u32 = 0x0403_4b50;

/// The central directory, read with the bounds of the module doc, and only
/// in a shape that the engine's zip reader takes exactly as the guard does.
///
/// Every rule the guard adds beyond its bounds closes a way two readers
/// part (the reviewed reader is the `zip` crate, 3.0.0: `read.rs`
/// `get_metadata`, `spec.rs` `find_central_directory`):
///
/// - It takes the last end record whose comment ends the file, and refuses
///   any second end record after it, in its comment or overlapping it: the
///   reader searches backwards from the end of the file and tries the last
///   one first.
/// - The entries on this disk and the entries in all agree: the reader
///   counts the first (zip32) or the second (ZIP64), the guard the same
///   number either way.
/// - The directory fills exactly the space its end record declares, from
///   its offset up to the end record (or the ZIP64 record), every header
///   inside it: nothing past the last header the count names, and nothing
///   the guard leaves unread.
/// - Every entry starts before the directory, and the first at the very
///   start of the file: data put before the zip would make the reader
///   move every offset (its archive offset), the guard none.
/// - Whatever that reader refuses in this end record, the ZIP64 record or
///   a header, the guard refuses first: a reader that refuses the end
///   record it found searches backwards for another one, reading counts
///   the guard never saw. So the ZIP64 record ends exactly at its locator,
///   carries at most [`MAX_ZIP64_EXTRA`] bytes of extensible data (that
///   reader allocates them) and asks for no newer version than wrote it,
///   every disk number is 0,
///   no entry is encrypted or compressed other than stored or deflated,
///   and every extra field is one that reader takes ([`extras`]).
fn directory(b: &[u8]) -> Result<Directory, String> {
    let unreadable = || damaged("the presentation's zip directory can't be read");
    let parts = |what: &str| damaged(format!("the presentation's zip {what}, which zip readers take differently, so the deck service does not open it (#448)"));
    let disks = || damaged("the presentation's zip spans several disks");
    // The end record: the last one in the final 64 KiB (its comment's most)
    // whose comment ends the file.
    let lowest = b.len().saturating_sub(22 + 0xFFFF);
    let mut at = None;
    let mut i = b.len().checked_sub(22).ok_or_else(unreadable)?;
    loop {
        if u32_at(b, i) == Some(EOCD) && u16_at(b, i + 20).is_some_and(|c| i + 22 + c as usize == b.len()) {
            at = Some(i);
            break;
        }
        if i == lowest {
            break;
        }
        i -= 1;
    }
    let eocd = at.ok_or_else(unreadable)?;
    if b[eocd + 1..].windows(4).any(|w| w == EOCD.to_le_bytes()) {
        return Err(parts("has a second end record in its comment"));
    }
    let field16 = |o: usize| u16_at(b, eocd + o).ok_or_else(unreadable);
    let field32 = |o: usize| u32_at(b, eocd + o).ok_or_else(unreadable);
    if field16(4)? != 0 || field16(6)? != 0 {
        return Err(disks());
    }
    let (on_disk, total) = (field16(8)?, field16(10)?);
    if on_disk != total {
        return Err(parts(&format!("counts {total} entries in all but {on_disk} on this disk")));
    }
    let mut count = total as u64;
    let mut size = field32(12)? as u64;
    let mut start = field32(16)? as u64;
    // Where the directory must end: at the end record, or at the ZIP64 one.
    let mut directory_end = eocd;
    // ZIP64, when the reader looks for it (`may_be_zip64`): the entry count
    // or the directory's offset at its most. Its locator is just before the
    // end record and names the record.
    if total == 0xFFFF || start == 0xFFFF_FFFF {
        let locator = eocd.checked_sub(20).filter(|l| u32_at(b, *l) == Some(ZIP64_LOCATOR)).ok_or_else(unreadable)?;
        if u32_at(b, locator + 4) != Some(0) || u32_at(b, locator + 16).is_none_or(|n| n > 1) {
            return Err(disks());
        }
        let record = usize::try_from(u64_at(b, locator + 8).ok_or_else(unreadable)?).map_err(|_| unreadable())?;
        if u32_at(b, record) != Some(ZIP64_EOCD) {
            return Err(unreadable());
        }
        let record_size = u64_at(b, record + 4).ok_or_else(unreadable)?;
        if record_size > 44 + MAX_ZIP64_EXTRA {
            return Err(held(format!("the presentation's zip has a ZIP64 end record of {record_size} bytes, more than the {} the deck service opens (#448)", 44 + MAX_ZIP64_EXTRA)));
        }
        if record_size < 44 || (record as u64).checked_add(12).and_then(|r| r.checked_add(record_size)) != Some(locator as u64) {
            return Err(unreadable());
        }
        if u16_at(b, record + 14).ok_or_else(unreadable)? > u16_at(b, record + 12).ok_or_else(unreadable)? {
            return Err(parts("asks for a newer zip version than the one that wrote it"));
        }
        if u32_at(b, record + 16) != Some(0) || u32_at(b, record + 20) != Some(0) {
            return Err(disks());
        }
        let (on_disk, total) = (u64_at(b, record + 24).ok_or_else(unreadable)?, u64_at(b, record + 32).ok_or_else(unreadable)?);
        if on_disk != total {
            return Err(parts(&format!("counts {total} entries in all but {on_disk} on this disk")));
        }
        count = total;
        size = u64_at(b, record + 40).ok_or_else(unreadable)?;
        start = u64_at(b, record + 48).ok_or_else(unreadable)?;
        directory_end = record;
    }
    if count > MAX_ENTRIES as u64 {
        return Err(held(format!("the presentation's zip has {count} entries, more than the {MAX_ENTRIES} the deck service opens (#448)")));
    }
    let start = usize::try_from(start).map_err(|_| unreadable())?;
    let end = start.checked_add(usize::try_from(size).map_err(|_| unreadable())?).ok_or_else(unreadable)?;
    let misplaced = || damaged("the presentation's zip directory isn't the size and place its end record declares");
    if end != directory_end {
        return Err(misplaced());
    }
    let mut entries = Vec::with_capacity(count as usize);
    let mut total: u64 = 0;
    let mut p = start;
    for _ in 0..count {
        if p + 46 > end || u32_at(b, p) != Some(CENTRAL) {
            return Err(unreadable());
        }
        let flags = u16_at(b, p + 8).ok_or_else(unreadable)?;
        let method = u16_at(b, p + 10).ok_or_else(unreadable)?;
        let mut compressed = u32_at(b, p + 20).ok_or_else(unreadable)? as u64;
        let mut declared = u32_at(b, p + 24).ok_or_else(unreadable)? as u64;
        let name_len = u16_at(b, p + 28).ok_or_else(unreadable)? as usize;
        let extra_len = u16_at(b, p + 30).ok_or_else(unreadable)? as usize;
        let comment_len = u16_at(b, p + 32).ok_or_else(unreadable)? as usize;
        let mut offset = u32_at(b, p + 42).ok_or_else(unreadable)? as u64;
        if name_len > MAX_NAME_BYTES {
            return Err(held(format!("an entry of the presentation's zip has a name of {name_len} bytes, more than the {MAX_NAME_BYTES} the deck service opens (#448)")));
        }
        let name_at = p + 46;
        let extra_at = name_at + name_len;
        let next = extra_at + extra_len + comment_len;
        if next > end {
            return Err(unreadable());
        }
        let raw = b[name_at..extra_at].to_vec();
        let name = String::from_utf8_lossy(&raw).into_owned();
        if flags & 1 != 0 {
            return Err(damaged(format!("`{}` in the presentation's zip is encrypted, which the deck engine doesn't read", shown(&name))));
        }
        if method != 0 && method != 8 {
            return Err(damaged(format!("`{}` in the presentation's zip is compressed with method {method}, which the deck engine doesn't read", shown(&name))));
        }
        extras(b, extra_at, extra_at + extra_len, [&mut declared, &mut compressed, &mut offset], &name)?;
        if offset >= start as u64 {
            return Err(damaged(format!("`{}` in the presentation's zip starts after the zip's directory", shown(&name))));
        }
        if declared > MAX_ENTRY_BYTES {
            return Err(held(format!(
                "`{}` in the presentation's zip declares {declared} bytes, more than the {MAX_ENTRY_BYTES} the deck service opens in one entry (#448)",
                shown(&name)
            )));
        }
        total = total.saturating_add(declared);
        if total > MAX_TOTAL_BYTES {
            return Err(held(format!("the presentation's zip declares more than the {MAX_TOTAL_BYTES} bytes the deck service opens in all (#448)")));
        }
        if declared > RATIO_GRACE && declared > compressed.saturating_mul(MAX_RATIO) {
            return Err(held(format!(
                "`{}` in the presentation's zip compresses {declared} bytes into {compressed}, more than the {MAX_RATIO} to 1 the deck service opens (#448)",
                shown(&name)
            )));
        }
        entries.push(Entry { name, raw, flags, method, compressed, declared, offset });
        p = next;
    }
    if p != end {
        return Err(misplaced());
    }
    if entries.iter().map(|e| e.offset).min().is_some_and(|first| first != 0) {
        return Err(parts("has data before its first entry"));
    }
    Ok(Directory { entries, start })
}

/// An entry's extra field (`b[from..to]`), read as the engine's zip reader
/// reads it (`zip` 3.0.0, `read::parse_single_extra_field`), so that reader
/// takes the very entries the guard checked, with the same sizes and
/// offset (`slots`: uncompressed, compressed, offset, as the header gave
/// them):
///
/// - ZIP64 (`0x0001`): a field of 24 bytes or more gives all three values,
///   whatever the header's 32-bit fields say; a shorter one gives those
///   whose 32-bit field is at its most, in that order. A field too short
///   for what it must give is `damaged:`.
/// - NTFS times (`0x000a`) and extended times (`0x5455`) pass only in the
///   shapes that reader takes; any other is `damaged:`.
/// - AES (`0x9901`), and Info-ZIP's Unicode path and comment (`0x7075`,
///   `0x6375`), are `damaged:`: the first is encryption, and the second
///   would rename the entry for the engine after the guard checked its
///   name.
/// - A field that runs past the extra field is `damaged:`.
///
/// Other fields that reader skips, and so does the guard.
fn extras(b: &[u8], from: usize, to: usize, slots: [&mut u64; 3], name: &str) -> Result<(), String> {
    let refused = |what: &str| damaged(format!("`{}` in the presentation's zip has {what}, which the deck service does not open (#448)", shown(name)));
    let [declared, compressed, offset] = slots;
    let mut e = from;
    while e < to {
        let broken = || refused("a broken extra field");
        if e + 4 > to {
            return Err(broken());
        }
        let kind = u16_at(b, e).ok_or_else(broken)?;
        let len = u16_at(b, e + 2).ok_or_else(broken)? as usize;
        let body = e + 4;
        let next = body + len;
        if next > to {
            return Err(broken());
        }
        match kind {
            0x0001 => {
                let all = len >= 24;
                let mut q = body;
                for slot in [&mut *declared, &mut *compressed, &mut *offset] {
                    if all || *slot == 0xFFFF_FFFF {
                        if q + 8 > next {
                            return Err(refused("a ZIP64 field of the wrong length"));
                        }
                        *slot = u64_at(b, q).ok_or_else(broken)?;
                        q += 8;
                    }
                }
            }
            0x000a => {
                if len != 32 || u16_at(b, body + 4) != Some(1) || u16_at(b, body + 6) != Some(24) {
                    return Err(refused("an NTFS time field the deck engine's zip reader refuses"));
                }
            }
            0x5455 => {
                let flags = if len >= 1 { b[body] } else { 0xFF };
                if !(len == 5 || len == 1 + 4 * flags.count_ones() as usize) || flags & 0xF8 != 0 {
                    return Err(refused("an extended time field the deck engine's zip reader refuses"));
                }
            }
            0x9901 => return Err(refused("AES encryption")),
            0x6375 | 0x7075 => return Err(refused("an Info-ZIP Unicode name or comment")),
            _ => {}
        }
        e = next;
    }
    Ok(())
}

/// Where `e`'s data begins in `b`: past its local header, by that header's
/// own name and extra lengths, as the engine's zip reader finds it.
fn data_start(b: &[u8], e: &Entry) -> Result<usize, String> {
    let outside = || damaged(format!("`{}` lies outside the presentation's zip", shown(&e.name)));
    let at = usize::try_from(e.offset).map_err(|_| outside())?;
    if u32_at(b, at) != Some(LOCAL) {
        return Err(outside());
    }
    let name_len = u16_at(b, at + 26).ok_or_else(outside)? as usize;
    let extra_len = u16_at(b, at + 28).ok_or_else(outside)? as usize;
    Ok(at + 30 + name_len + extra_len)
}

/// Where `e`'s data lies in `b`.
fn data_of<'a>(b: &'a [u8], e: &Entry) -> Result<&'a [u8], String> {
    let outside = || damaged(format!("`{}` lies outside the presentation's zip", shown(&e.name)));
    let start = data_start(b, e)?;
    let len = usize::try_from(e.compressed).map_err(|_| outside())?;
    b.get(start..start.checked_add(len).ok_or_else(outside)?).ok_or_else(outside)
}

/// The engine's own zip reader must read the directory exactly as the
/// guard did: the `zip` crate that deckcraft's package and native readers
/// open every deck with (`ZipArchive::new`, its default configuration; the
/// same crate and version, which a test checks in `Cargo.lock`). It runs
/// only after [`directory`] bounded the directory and refused everything
/// that reader would refuse in it, so the reader takes that very end
/// record and never looks for another, and it reads the directory and each
/// entry's local header, never an entry's data. Anything the reader sees
/// differently (data before the zip, another directory, another number of
/// entries, an entry's name, sizes, offset, where its data begins, its
/// method or encryption) is `damaged:`.
fn read_alike(b: &[u8], dir: &Directory) -> Result<(), String> {
    let differs = |what: String| damaged(format!("the deck engine's zip reader reads the presentation differently ({what}), so the deck service does not open it (#448)"));
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(b)).map_err(|e| damaged(format!("the deck engine's zip reader can't read the presentation: {e}")))?;
    if z.offset() != 0 {
        return Err(differs(format!("{} bytes before the zip", z.offset())));
    }
    if z.central_directory_start() != dir.start as u64 {
        return Err(differs("another directory".into()));
    }
    if z.len() != dir.entries.len() {
        return Err(differs(format!("{} entries, not {}", z.len(), dir.entries.len())));
    }
    for (i, e) in dir.entries.iter().enumerate() {
        let start = data_start(b, e)? as u64;
        let f = z.by_index_raw(i).map_err(|err| damaged(format!("the deck engine's zip reader can't read `{}`: {err}", shown(&e.name))))?;
        let method = match f.compression() {
            zip::CompressionMethod::Stored => Some(0),
            zip::CompressionMethod::Deflated => Some(8),
            _ => None,
        };
        let alike = f.name_raw() == e.raw.as_slice()
            && f.compressed_size() == e.compressed
            && f.size() == e.declared
            && f.header_start() == e.offset
            && f.data_start() == start
            && method == Some(e.method)
            && !f.encrypted();
        if !alike {
            return Err(differs(format!("`{}`", shown(&e.name))));
        }
    }
    Ok(())
}

/// What an entry turned out to hold: its first bytes, and the whole of it
/// when the caller asked to keep it.
struct Inflated {
    head: Vec<u8>,
    kept: Option<Vec<u8>>,
}

/// How many first bytes of each entry the signatures look at.
const HEAD: usize = 16;

/// Inflate (or read) `e` with its declared size as the bound, keeping its
/// first bytes, the whole of it when `keep`, and feeding every chunk to
/// `watch`.
fn inflate(b: &[u8], e: &Entry, keep: bool, watch: &mut dyn FnMut(&[u8])) -> Result<Inflated, String> {
    if e.flags & 1 != 0 {
        return Err(damaged(format!("`{}` in the presentation's zip is encrypted, which the deck engine doesn't read", shown(&e.name))));
    }
    let data = data_of(b, e)?;
    let mut out = Inflated { head: Vec::with_capacity(HEAD), kept: keep.then(Vec::new) };
    let mut take = |chunk: &[u8], out: &mut Inflated| {
        let room = HEAD.saturating_sub(out.head.len());
        out.head.extend_from_slice(&chunk[..chunk.len().min(room)]);
        if let Some(kept) = out.kept.as_mut() {
            kept.extend_from_slice(chunk);
        }
        watch(chunk);
    };
    match e.method {
        0 => {
            if e.compressed != e.declared {
                return Err(damaged(format!("`{}` in the presentation's zip is not the size its directory says", shown(&e.name))));
            }
            take(data, &mut out);
        }
        8 => {
            let mut decoder = flate2::read::DeflateDecoder::new(data);
            let mut buf = vec![0u8; 64 << 10];
            let mut total: u64 = 0;
            loop {
                let n = decoder
                    .read(&mut buf)
                    .map_err(|err| damaged(format!("`{}` in the presentation's zip can't be inflated: {err}", shown(&e.name))))?;
                if n == 0 {
                    break;
                }
                total += n as u64;
                if total > e.declared {
                    return Err(damaged(format!(
                        "`{}` in the presentation's zip inflates past the {} bytes its directory declares",
                        shown(&e.name),
                        e.declared
                    )));
                }
                take(&buf[..n], &mut out);
            }
        }
        other => {
            return Err(damaged(format!(
                "`{}` in the presentation's zip is compressed with method {other}, which the deck engine doesn't read",
                shown(&e.name)
            )))
        }
    }
    Ok(out)
}

/// What a zip's binary parts declare in all: every entry but a text part
/// of the model (an Office package's XML and relationship parts, a native
/// deck's `presentation.json` and `mimetype`). The media a deck holds once
/// read is some of these, so a total past the session's media ceiling is
/// refused before the engine copies any of it out.
fn binary_total(entries: &[Entry]) -> u64 {
    let text = |name: &str| {
        let lower = name.to_ascii_lowercase();
        lower.ends_with(".xml") || lower.ends_with(".rels") || lower == "presentation.json" || lower == "mimetype"
    };
    entries.iter().filter(|e| !text(&e.name)).map(|e| e.declared).fold(0u64, u64::saturating_add)
}

/// Whether `head` starts a zip archive: a zip inside the zip.
fn zip_signature(head: &[u8]) -> bool {
    head.starts_with(b"PK\x03\x04") || head.starts_with(b"PK\x05\x06") || head.starts_with(b"PK\x07\x08")
}

/// Where an Office package keeps its embedded objects (a chart's workbook,
/// an embedded document or OLE object), whose parts may be zips themselves.
pub(crate) const EMBEDDINGS: &str = "ppt/embeddings/";

/// Whether `name` is an embedded object's part: under [`EMBEDDINGS`], with
/// no empty, `.` or `..` segment and no backslash, so no other part can
/// borrow the exemption by its spelling.
fn embedded_object(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.strip_prefix(EMBEDDINGS).is_some_and(|rest| !rest.contains('\\') && rest.split('/').all(|seg| !seg.is_empty() && seg != "." && seg != ".."))
}

/// The guard for a zip (module doc).
fn zip(b: &[u8]) -> Result<Format, String> {
    let dir = directory(b)?;
    read_alike(b, &dir)?;
    let entries = &dir.entries;
    if let Some(why) = crate::media_over(binary_total(entries)) {
        return Err(crate::too_large_deck(why));
    }
    // What it is (module doc): a native deck by deckcraft's own marker,
    // which the engine tries first, else an Office package by its
    // content-type list in the directory both readers read alike. The
    // verdict picks the engine's reader (`crate::engine_read`).
    let package = entries.iter().find(|e| e.name.eq_ignore_ascii_case("[Content_Types].xml"));
    let format = if deckcraft_format::sniff(b) {
        Format::Native
    } else if package.is_some() {
        Format::Pptx
    } else {
        return Err(fail(Code::Unsupported, "this zip is not a presentation the deck engine reads (.pptx or .deckcraft)"));
    };
    for e in entries {
        let inflated = inflate(b, e, false, &mut |_| {})?;
        // An Office package's embedded object may be a zip: nothing unzips it.
        if zip_signature(&inflated.head) && !(format == Format::Pptx && embedded_object(&e.name)) {
            return Err(held(format!("the presentation holds a zip inside its zip (`{}`), which the deck service does not open (#448)", shown(&e.name))));
        }
    }
    Ok(format)
}

/// The guard for an OLE compound file: `protected:` when its directory holds
/// an `EncryptedPackage` stream, `unsupported:` otherwise (module doc).
fn ole(b: &[u8]) -> String {
    if encrypted_package(b) == Some(true) {
        fail(Code::Protected, "this presentation is protected with a password, and the deck service doesn't open those")
    } else {
        fail(Code::Unsupported, "this is a legacy .ppt or another OLE file, not a presentation the deck engine reads (.pptx or .deckcraft)")
    }
}

/// Whether the compound file's directory holds a stream named
/// `EncryptedPackage` (`None`: the file is no compound file it can read).
/// Every walk is bounded by the file's own sector count, so a looping chain
/// ends.
fn encrypted_package(b: &[u8]) -> Option<bool> {
    const FREE: u32 = 0xFFFF_FFFA;
    if b.len() < 512 || b[..8] != OLE {
        return None;
    }
    let sector = match (u16_at(b, 0x1A)?, u16_at(b, 0x1E)?) {
        (3, 9) => 512usize,
        (4, 12) => 4096usize,
        _ => return None,
    };
    let sectors = b.len() / sector;
    let sector_at = |s: u32| -> Option<&[u8]> {
        let at = (s as usize).checked_add(1)?.checked_mul(sector)?;
        b.get(at..at + sector)
    };
    // The FAT's sectors: the header's 109, then the DIFAT chain; never more
    // than the file has sectors, whatever the header says.
    let fat_count = (u32_at(b, 0x2C)? as usize).min(sectors);
    let mut fat: Vec<u32> = (0..109).filter_map(|i| u32_at(b, 0x4C + 4 * i)).filter(|s| *s < FREE).collect();
    let mut difat = u32_at(b, 0x44)?;
    let mut steps = 0;
    while difat < FREE && fat.len() < fat_count && steps <= sectors {
        let sec = sector_at(difat)?;
        let per = sector / 4 - 1;
        fat.extend((0..per).filter_map(|i| u32_at(sec, 4 * i)).filter(|s| *s < FREE));
        difat = u32_at(sec, 4 * per)?;
        steps += 1;
    }
    fat.truncate(fat_count);
    let next = |s: u32| -> Option<u32> {
        let per = sector / 4;
        let fs = *fat.get(s as usize / per)?;
        u32_at(sector_at(fs)?, (s as usize % per) * 4)
    };
    // The directory chain: 128-byte entries, each a UTF-16 name.
    let mut s = u32_at(b, 0x30)?;
    let mut steps = 0;
    while s < FREE {
        if steps > sectors {
            return None;
        }
        steps += 1;
        let sec = sector_at(s)?;
        for entry in sec.as_chunks::<128>().0 {
            let len = u16_at(entry, 0x40)? as usize;
            if entry[0x42] == 2 && (4..=64).contains(&len) {
                let units: Vec<u16> = entry[..len - 2].as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
                if String::from_utf16_lossy(&units).eq_ignore_ascii_case("EncryptedPackage") {
                    return Some(true);
                }
            }
        }
        s = next(s)?;
    }
    Some(false)
}

/// Hostile and honest files for the guard's tests, built here byte by byte
/// so a test can make a zip lie.
#[cfg(test)]
pub(crate) mod fixtures {
    use std::io::Write;

    /// One entry of a test zip.
    pub(crate) struct Part {
        pub name: String,
        pub data: Vec<u8>,
        pub deflate: bool,
        /// The uncompressed size the directory declares, when it lies.
        pub declared: Option<u64>,
        /// The name's bytes, when they are not `name`'s.
        pub raw_name: Option<Vec<u8>>,
        /// The general-purpose flags (bit 11: the name is UTF-8).
        pub flags: u16,
        /// The central header's extra field.
        pub extra: Vec<u8>,
        /// Only a local header and data, which no central header names.
        pub hidden: bool,
    }

    impl Part {
        pub(crate) fn stored(name: &str, data: &[u8]) -> Part {
            Part { name: name.into(), data: data.to_vec(), deflate: false, declared: None, raw_name: None, flags: 0, extra: Vec::new(), hidden: false }
        }
        pub(crate) fn deflated(name: &str, data: &[u8]) -> Part {
            Part { deflate: true, ..Part::stored(name, data) }
        }
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &byte in data {
            crc ^= byte as u32;
            for _ in 0..8 {
                crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
            }
        }
        !crc
    }

    /// `data` deflated as a part with `deflate` is.
    pub(crate) fn deflate(data: &[u8]) -> Vec<u8> {
        let mut e = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
        e.write_all(data).unwrap();
        e.finish().unwrap()
    }

    /// A zip's two halves: its entries (local headers and data) and its
    /// central directory, which names `count` of them.
    pub(crate) struct Layout {
        pub locals: Vec<u8>,
        pub central: Vec<u8>,
        pub count: u16,
    }

    /// The entries and central directory of `parts`, honest unless a part
    /// says otherwise, as if `base` bytes came before them.
    pub(crate) fn layout_from(parts: &[Part], base: u32) -> Layout {
        let mut out = Vec::new();
        let mut central = Vec::new();
        let mut count = 0u16;
        for p in parts {
            let body = if p.deflate { deflate(&p.data) } else { p.data.clone() };
            let crc = crc32(&p.data);
            let declared = p.declared.unwrap_or(p.data.len() as u64) as u32;
            let method: u16 = if p.deflate { 8 } else { 0 };
            let offset = base + out.len() as u32;
            let name = p.raw_name.clone().unwrap_or_else(|| p.name.as_bytes().to_vec());
            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes());
            out.extend_from_slice(&p.flags.to_le_bytes());
            out.extend_from_slice(&method.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0x21u16.to_le_bytes());
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&(body.len() as u32).to_le_bytes());
            out.extend_from_slice(&declared.to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&name);
            out.extend_from_slice(&body);
            if p.hidden {
                continue;
            }
            count += 1;
            central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&p.flags.to_le_bytes());
            central.extend_from_slice(&method.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0x21u16.to_le_bytes());
            central.extend_from_slice(&crc.to_le_bytes());
            central.extend_from_slice(&(body.len() as u32).to_le_bytes());
            central.extend_from_slice(&declared.to_le_bytes());
            central.extend_from_slice(&(name.len() as u16).to_le_bytes());
            central.extend_from_slice(&(p.extra.len() as u16).to_le_bytes());
            central.extend_from_slice(&[0u8; 10]);
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(&name);
            central.extend_from_slice(&p.extra);
        }
        Layout { locals: out, central, count }
    }

    /// [`layout_from`] at the start of the file.
    pub(crate) fn layout(parts: &[Part]) -> Layout {
        layout_from(parts, 0)
    }

    /// An end record: `on_disk` and `total` entries, a directory of `size`
    /// bytes at `offset`, and `comment`.
    pub(crate) fn end_record(on_disk: u16, total: u16, size: u32, offset: u32, comment: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&[0u8; 4]);
        out.extend_from_slice(&on_disk.to_le_bytes());
        out.extend_from_slice(&total.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        out.extend_from_slice(&(comment.len() as u16).to_le_bytes());
        out.extend_from_slice(comment);
        out
    }

    /// A zip of `parts`, honest unless a part says otherwise.
    pub(crate) fn zip(parts: &[Part]) -> Vec<u8> {
        let l = layout(parts);
        let end = end_record(l.count, l.count, l.central.len() as u32, l.locals.len() as u32, b"");
        [l.locals, l.central, end].concat()
    }

    /// A ZIP64 zip of `parts`: its directory named by a ZIP64 end record
    /// (counting `on_disk` and `total` entries, needing version `needed`)
    /// and its locator, the end record's own fields at their most.
    pub(crate) fn zip64(parts: &[Part], on_disk: u64, total: u64, needed: u16) -> Vec<u8> {
        zip64_with(parts, on_disk, total, needed, b"")
    }

    /// [`zip64`], its end record carrying `extensible` data.
    pub(crate) fn zip64_with(parts: &[Part], on_disk: u64, total: u64, needed: u16, extensible: &[u8]) -> Vec<u8> {
        let l = layout(parts);
        let record_at = (l.locals.len() + l.central.len()) as u64;
        let mut record = Vec::new();
        record.extend_from_slice(&0x0606_4b50u32.to_le_bytes());
        record.extend_from_slice(&(44 + extensible.len() as u64).to_le_bytes());
        record.extend_from_slice(&45u16.to_le_bytes());
        record.extend_from_slice(&needed.to_le_bytes());
        record.extend_from_slice(&[0u8; 8]);
        record.extend_from_slice(&on_disk.to_le_bytes());
        record.extend_from_slice(&total.to_le_bytes());
        record.extend_from_slice(&(l.central.len() as u64).to_le_bytes());
        record.extend_from_slice(&(l.locals.len() as u64).to_le_bytes());
        record.extend_from_slice(extensible);
        let mut locator = Vec::new();
        locator.extend_from_slice(&0x0706_4b50u32.to_le_bytes());
        locator.extend_from_slice(&0u32.to_le_bytes());
        locator.extend_from_slice(&record_at.to_le_bytes());
        locator.extend_from_slice(&1u32.to_le_bytes());
        let end = end_record(0xFFFF, 0xFFFF, 0xFFFF_FFFF, 0xFFFF_FFFF, b"");
        [l.locals, l.central, record, locator, end].concat()
    }

    /// One extra field: `kind`, then `body`.
    pub(crate) fn field(kind: u16, body: &[u8]) -> Vec<u8> {
        [kind.to_le_bytes().as_slice(), (body.len() as u16).to_le_bytes().as_slice(), body].concat()
    }

    /// An Office package's content-type list and its first part.
    pub(crate) fn package(extra: Vec<Part>, types: &str) -> Vec<u8> {
        let mut parts = vec![Part::deflated(
            "[Content_Types].xml",
            format!("<?xml version=\"1.0\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"xml\" ContentType=\"application/xml\"/>{types}</Types>").as_bytes(),
        )];
        parts.push(Part::deflated("ppt/presentation.xml", b"<p:presentation/>"));
        parts.extend(extra);
        zip(&parts)
    }

    /// The zip `bytes` again, every entry read back with the guard's own
    /// reader and deflated, with `extra` after them: an engine-written
    /// package with parts added.
    pub(crate) fn with_parts(bytes: &[u8], extra: Vec<Part>) -> Vec<u8> {
        with_last(bytes, "", extra)
    }

    /// [`with_parts`], the entry named `last` moved after every other
    /// entry and `extra`: a package laid out as LibreOffice writes one,
    /// its content-type list last.
    pub(crate) fn with_last(bytes: &[u8], last: &str, extra: Vec<Part>) -> Vec<u8> {
        let dir = super::directory(bytes).unwrap();
        let read = |e: &super::Entry| Part::deflated(&e.name, &super::inflate(bytes, e, true, &mut |_| {}).unwrap().kept.unwrap());
        let mut parts: Vec<Part> = dir.entries.iter().filter(|e| !e.name.ends_with('/') && e.name != last).map(read).collect();
        parts.extend(extra);
        parts.extend(dir.entries.iter().filter(|e| e.name == last).map(read));
        zip(&parts)
    }

    /// The zip `bytes` again (as [`with_parts`]), the entry named `name`
    /// rewritten by `edit`.
    pub(crate) fn with_rewritten(bytes: &[u8], name: &str, edit: impl Fn(&[u8]) -> Vec<u8>) -> Vec<u8> {
        let dir = super::directory(bytes).unwrap();
        let parts: Vec<Part> = dir
            .entries
            .iter()
            .filter(|e| !e.name.ends_with('/'))
            .map(|e| {
                let data = super::inflate(bytes, e, true, &mut |_| {}).unwrap().kept.unwrap();
                Part::deflated(&e.name, &if e.name == name { edit(&data) } else { data })
            })
            .collect();
        zip(&parts)
    }

    /// A native deck's skeleton: its `mimetype` and `presentation.json`.
    pub(crate) fn native(extra: Vec<Part>, json: &str) -> Vec<u8> {
        let mut parts = vec![Part::stored("mimetype", deckcraft_format::MIMETYPE.as_bytes()), Part::deflated("presentation.json", json.as_bytes())];
        parts.extend(extra);
        zip(&parts)
    }

    /// An OLE compound file (version 3, 512-byte sectors) whose directory
    /// holds `streams` after its root entry.
    pub(crate) fn ole(streams: &[&str]) -> Vec<u8> {
        let mut header = vec![0u8; 512];
        header[..8].copy_from_slice(&super::OLE);
        header[0x18..0x1A].copy_from_slice(&0x3Eu16.to_le_bytes());
        header[0x1A..0x1C].copy_from_slice(&3u16.to_le_bytes());
        header[0x1C..0x1E].copy_from_slice(&0xFFFEu16.to_le_bytes());
        header[0x1E..0x20].copy_from_slice(&9u16.to_le_bytes());
        header[0x20..0x22].copy_from_slice(&6u16.to_le_bytes());
        header[0x2C..0x30].copy_from_slice(&1u32.to_le_bytes());
        header[0x30..0x34].copy_from_slice(&1u32.to_le_bytes());
        header[0x38..0x3C].copy_from_slice(&4096u32.to_le_bytes());
        header[0x3C..0x40].copy_from_slice(&0xFFFF_FFFEu32.to_le_bytes());
        header[0x44..0x48].copy_from_slice(&0xFFFF_FFFEu32.to_le_bytes());
        for i in 0..109 {
            let v: u32 = if i == 0 { 0 } else { 0xFFFF_FFFF };
            header[0x4C + 4 * i..0x50 + 4 * i].copy_from_slice(&v.to_le_bytes());
        }
        // Sector 0: the FAT (itself, then the directory's two sectors).
        let mut fat = vec![0xFFu8; 512];
        fat[..4].copy_from_slice(&0xFFFF_FFFDu32.to_le_bytes());
        fat[4..8].copy_from_slice(&2u32.to_le_bytes());
        fat[8..12].copy_from_slice(&0xFFFF_FFFEu32.to_le_bytes());
        // Sectors 1 and 2: the directory, four entries a sector.
        let mut dir = vec![0u8; 1024];
        let names: Vec<(&str, u8)> = std::iter::once(("Root Entry", 5u8)).chain(streams.iter().map(|s| (*s, 2u8))).collect();
        for (i, (name, kind)) in names.iter().enumerate().take(8) {
            let e = &mut dir[i * 128..(i + 1) * 128];
            let units: Vec<u16> = name.encode_utf16().collect();
            for (k, u) in units.iter().enumerate() {
                e[k * 2..k * 2 + 2].copy_from_slice(&u.to_le_bytes());
            }
            e[0x40..0x42].copy_from_slice(&(((units.len() + 1) * 2) as u16).to_le_bytes());
            e[0x42] = *kind;
            e[0x44..0x50].copy_from_slice(&[0xFF; 12]);
        }
        let mut out = header;
        out.extend_from_slice(&fat);
        out.extend_from_slice(&dir);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::{end_record, field, layout, layout_from, native, ole, package, zip, zip64, zip64_with, Part};
    use super::*;

    #[track_caller]
    fn coded(r: Result<Format, String>, code: &str) -> String {
        let e = r.expect_err("refused");
        assert!(e.starts_with(&format!("{code}: ")), "expected {code}: {e}");
        e
    }

    #[test]
    fn honest_files_pass_and_say_what_they_are() {
        assert_eq!(check("deck.pptx", &package(vec![Part::stored("ppt/media/image1.png", b"\x89PNG\r\n\x1a\nrest")], "")), Ok(Format::Pptx));
        assert_eq!(check("deck.deckcraft", &native(vec![Part::stored("media/7", b"\xFF\xD8\xFF\xE0jpeg")], "{\"format\":\"deckcraft\"}")), Ok(Format::Native));
        assert_eq!(check("outline.txt", b"Title\n\tPoint\n"), Ok(Format::Outline));
        assert_eq!(check("Notes.MD", b"# x"), Ok(Format::Outline));
        // The engine's own writers' output passes.
        let deck = deckcraft_model::defaults::new_presentation(None);
        assert_eq!(check("x.deckcraft", &deckcraft_format::save(&deck).unwrap()), Ok(Format::Native));
        assert_eq!(check("x.pptx", &deckcraft_engine::cmd::file::save_bytes(&deck, "pptx").unwrap()), Ok(Format::Pptx));
    }

    #[test]
    fn what_the_engine_does_not_read_is_unsupported() {
        coded(check("legacy.ppt", &ole(&["PowerPoint Document", "Current User"])), "unsupported");
        coded(check("broken.ppt", &OLE), "unsupported");
        coded(check("doc.pdf", b"%PDF-1.7\n..."), "unsupported");
        coded(check("deck.key", &zip(&[Part::stored("Index/Document.iwa", b"\0\0\0"), Part::stored("Metadata/Properties.plist", b"<plist/>")])), "unsupported");
        coded(check("deck.odp", &zip(&[Part::stored("mimetype", b"application/vnd.oasis.opendocument.presentation")])), "unsupported");
        coded(check("deck.pptx", b"not a zip at all"), "unsupported");
        coded(check("image.png", b"\x89PNG\r\n\x1a\n"), "unsupported");
    }

    #[test]
    fn an_encrypted_package_is_protected() {
        let e = coded(check("locked.pptx", &ole(&["EncryptionInfo", "EncryptedPackage"])), "protected");
        assert!(e.contains("password"), "{e}");
        // Found past the first directory sector too.
        coded(check("locked.pptx", &ole(&["a", "b", "c", "d", "EncryptedPackage"])), "protected");
    }

    #[test]
    fn a_zip_beyond_the_guard_is_held() {
        // Ten thousand entries and one more.
        let many: Vec<Part> = (0..=MAX_ENTRIES).map(|i| Part::stored(&format!("f{i}"), b"")).collect();
        let e = coded(check("many.pptx", &zip(&many)), "held");
        assert!(e.contains("10001 entries"), "{e}");
        // At the cap the directory passes (and the names decide).
        let at_cap: Vec<Part> = (0..MAX_ENTRIES).map(|i| Part::stored(&format!("f{i}"), b"")).collect();
        coded(check("cap.zip", &zip(&at_cap)), "unsupported");
        // A zip bomb: 16 MiB of zeros in a few kilobytes.
        let bomb = package(vec![Part::deflated("ppt/slides/slide1.xml", &vec![0u8; 16 << 20])], "");
        assert!(bomb.len() < 1 << 20, "{} bytes", bomb.len());
        let e = coded(check("bomb.pptx", &bomb), "held");
        assert!(e.contains("to 1"), "{e}");
        // One entry declaring more than 64 MiB, and entries declaring more
        // than 256 MiB in all.
        let big = Part { declared: Some(MAX_ENTRY_BYTES + 1), ..Part::stored("ppt/big.bin", &vec![7u8; 1 << 20]) };
        coded(check("big.pptx", &package(vec![big], "")), "held");
        let parts: Vec<Part> = (0..5).map(|i| Part { declared: Some(60 << 20), ..Part::stored(&format!("ppt/p{i}.bin"), &vec![7u8; 1 << 20]) }).collect();
        let e = coded(check("total.pptx", &package(parts, "")), "held");
        assert!(e.contains("in all"), "{e}");
        // A name past 1,024 bytes.
        coded(check("name.pptx", &package(vec![Part::stored(&"n".repeat(MAX_NAME_BYTES + 1), b"x")], "")), "held");
    }

    #[test]
    fn a_lying_or_broken_zip_is_damaged() {
        // Declares 100 bytes, inflates to a megabyte.
        let liar = Part { declared: Some(100), ..Part::deflated("ppt/slides/slide1.xml", &vec![b' '; 1 << 20]) };
        let e = coded(check("liar.pptx", &package(vec![liar], "")), "damaged");
        assert!(e.contains("inflates past the 100 bytes"), "{e}");
        let stored_liar = Part { declared: Some(3), ..Part::stored("ppt/x.bin", b"0123456789") };
        coded(check("liar.pptx", &package(vec![stored_liar], "")), "damaged");
        // A directory that doesn't parse: cut in half.
        let good = package(vec![], "");
        coded(check("cut.pptx", &good[..good.len() / 2]), "damaged");
        coded(check("cut.pptx", b"PK\x03\x04garbage"), "damaged");
        // An entry whose data lies outside the file.
        let mut bad = package(vec![], "");
        let n = bad.len();
        // The last central entry's local offset (16 bytes before the end
        // record's start + the name) — rewrite every offset to past the end.
        for i in 0..n.saturating_sub(4) {
            if bad[i..i + 4] == 0x0201_4b50u32.to_le_bytes() {
                bad[i + 42..i + 46].copy_from_slice(&(n as u32 + 100).to_le_bytes());
            }
        }
        coded(check("outside.pptx", &bad), "damaged");
    }

    #[test]
    fn embedded_objects_and_media_pass_and_other_nested_zips_are_held() {
        let inner = zip(&[Part::deflated("[Content_Types].xml", b"<Types/>"), Part::deflated("xl/workbook.xml", b"<workbook/>")]);
        // A chart's workbook, an embedded document or object: zips that
        // nothing unzips, under the package's embeddings.
        let xlsx = "<Default Extension=\"xlsx\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet\"/>";
        assert_eq!(check("chart.pptx", &package(vec![Part::stored("ppt/embeddings/Microsoft_Excel_Worksheet1.xlsx", &inner)], xlsx)), Ok(Format::Pptx));
        let objects = vec![
            Part::deflated("ppt/embeddings/Microsoft_Word_Document.docx", &inner),
            Part::deflated("ppt/embeddings/oleObject1.bin", &inner),
            Part::stored("PPT/Embeddings/Sheet2.xlsx", &inner),
        ];
        assert_eq!(check("objects.pptx", &package(objects, "")), Ok(Format::Pptx));
        // A zip anywhere else stays held, and so does one only spelled to
        // look embedded, or one in a native deck, which embeds no objects.
        for name in ["ppt/media/x.zip", "ppt/slides/x.zip", "x.zip", "ppt/embeddings/../media/x.zip", "ppt/embeddings//x.xlsx", "ppt/embeddings/a/./x.xlsx", "ppt\\embeddings\\x.xlsx"] {
            let e = coded(check("nested.pptx", &package(vec![Part::stored(name, &inner)], "")), "held");
            assert!(e.contains("zip inside its zip"), "{name}: {e}");
        }
        coded(check("nested.deckcraft", &native(vec![Part::stored("ppt/embeddings/x.xlsx", &inner)], "{}")), "held");
        // An embedded object is held to every other bound: its declared
        // size, its name, its actual size, and the ratio.
        let big = Part { declared: Some(MAX_ENTRY_BYTES + 1), ..Part::stored("ppt/embeddings/big.xlsx", &vec![7u8; 1 << 20]) };
        coded(check("big.pptx", &package(vec![big], "")), "held");
        let long = format!("ppt/embeddings/{}.xlsx", "n".repeat(MAX_NAME_BYTES));
        coded(check("name.pptx", &package(vec![Part::stored(&long, &inner)], "")), "held");
        let liar = Part { declared: Some(100), ..Part::deflated("ppt/embeddings/liar.xlsx", &[inner.clone(), vec![0u8; 1 << 20]].concat()) };
        coded(check("liar.pptx", &package(vec![liar], "")), "damaged");
        // A workbook that is a zip bomb (16 MiB in a few kilobytes) is
        // refused by its directory entry's ratio, before anything is
        // inflated or unzipped: with its stream broken it is refused the
        // same way, where inflating it would have found it damaged.
        let name = "ppt/embeddings/bomb.xlsx";
        let mut bomb = package(vec![Part::deflated(name, &[inner.clone(), vec![0u8; 16 << 20]].concat())], "");
        assert!(bomb.len() < 1 << 20, "{} bytes", bomb.len());
        let e = coded(check("bomb.pptx", &bomb), "held");
        assert!(e.contains(name) && e.contains("100 to 1"), "{e}");
        let body = bomb.windows(name.len()).position(|w| w == name.as_bytes()).unwrap() + name.len();
        bomb[body + 64..body + 128].fill(0xFF);
        let e = coded(check("bomb.pptx", &bomb), "held");
        assert!(e.contains("100 to 1"), "{e}");
        // Audio and video pass like any other part, however they show: by
        // extension, by declared type, by signature, under the embeddings,
        // in a native deck by signature and by its manifest's content type.
        // Nothing the service runs decodes them (module doc).
        let movie = b"\0\0\0\x18ftypisom".as_slice();
        for (name, part, types) in [
            ("movie.pptx", Part::stored("ppt/media/media1.mp4", movie), ""),
            ("movie.pptx", Part::stored("ppt/media/clip.bin", b"0123"), "<Override PartName=\"/ppt/media/clip.bin\" ContentType=\"video/mp4\"/>"),
            ("sound.pptx", Part::stored("ppt/media/a.dat", b"0123"), "<Default Extension=\"dat\" ContentType='audio/x-dat'/>"),
            ("sound.pptx", Part::deflated("ppt/media/a.bin", b"RIFF\x10\0\0\0WAVEfmt "), ""),
            ("movie.pptx", Part::stored("ppt/embeddings/clip.mp4", movie), ""),
        ] {
            assert_eq!(check(name, &package(vec![part], types)), Ok(Format::Pptx), "{name}");
        }
        assert_eq!(check("sound.deckcraft", &native(vec![Part::stored("media/9", b"ID3\x04\0\0\0")], "{}")), Ok(Format::Native));
        let json = "{\"presentation\":{\"media\":[{\"id\":9,\"name\":\"a.wav\",\"contentType\":\"audio/wav\"}]}}";
        assert_eq!(check("sound.deckcraft", &native(vec![], json)), Ok(Format::Native));
        // Media is still held to every bound: a movie that lies about its
        // size, or compresses past the ratio, is refused.
        let liar = Part { declared: Some(10), ..Part::deflated("ppt/media/media1.mp4", &[movie, &[0u8; 4096]].concat()) };
        coded(check("movie.pptx", &package(vec![liar], "")), "damaged");
        let bombed = package(vec![Part::deflated("ppt/media/media1.mp4", &[movie.to_vec(), vec![0u8; 16 << 20]].concat())], "");
        let e = coded(check("movie.pptx", &bombed), "held");
        assert!(e.contains("100 to 1"), "{e}");
    }

    /// The two parts every package below starts with.
    fn head() -> Vec<Part> {
        vec![Part::deflated("[Content_Types].xml", b"<Types/>"), Part::deflated("ppt/presentation.xml", b"<p:presentation/>")]
    }

    /// `l`'s entries and directory, then `end`.
    fn with_end(l: &super::fixtures::Layout, end: Vec<u8>) -> Vec<u8> {
        [l.locals.clone(), l.central.clone(), end].concat()
    }

    #[test]
    fn the_readers_never_part_on_the_entry_count() {
        // The review's proof of concept: the end record counts one entry in
        // all and two on this disk. The guard read one ([Content_Types].xml)
        // and the engine's zip reader two, the second a movie.
        let parts = vec![Part::deflated("[Content_Types].xml", b"<Types/>"), Part::stored("ppt/media/evil.mp4", b"\0\0\0\x18ftypisom")];
        let l = layout(&parts);
        let (size, at) = (l.central.len() as u32, l.locals.len() as u32);
        let poc = with_end(&l, end_record(2, 1, size, at, b""));
        let engine = zip::ZipArchive::new(std::io::Cursor::new(&poc)).unwrap();
        assert_eq!((engine.len(), engine.name_for_index(1)), (2, Some("ppt/media/evil.mp4")), "the engine's zip reader sees the movie");
        let e = coded(check("poc.pptx", &poc), "damaged");
        assert!(e.contains("1 entries in all but 2 on this disk"), "{e}");
        coded(check("poc.pptx", &with_end(&l, end_record(1, 2, size, at, b""))), "damaged");
        // The same with a zip bomb as the hidden entry.
        let parts = vec![Part::deflated("[Content_Types].xml", b"<Types/>"), Part::deflated("ppt/slides/slide1.xml", &vec![0u8; 16 << 20])];
        let l = layout(&parts);
        coded(check("poc.pptx", &with_end(&l, end_record(2, 1, l.central.len() as u32, l.locals.len() as u32, b""))), "damaged");
        // ZIP64: an honest record passes; its counts disagreeing, or a
        // record asking for a newer version than wrote it, does not.
        assert_eq!(check("big.pptx", &zip64(&head(), 2, 2, 45)), Ok(Format::Pptx));
        let e = coded(check("big.pptx", &zip64(&head(), 3, 2, 45)), "damaged");
        assert!(e.contains("2 entries in all but 3 on this disk"), "{e}");
        coded(check("big.pptx", &zip64(&head(), 2, 3, 45)), "damaged");
        coded(check("big.pptx", &zip64(&head(), 2, 2, 46)), "damaged");
        // Its extensible data, which the engine's zip reader allocates
        // whole: up to the cap, then held.
        let at_cap = vec![7u8; MAX_ZIP64_EXTRA as usize];
        assert_eq!(check("big.pptx", &zip64_with(&head(), 2, 2, 45, &at_cap)), Ok(Format::Pptx));
        let over = vec![7u8; MAX_ZIP64_EXTRA as usize + 1];
        let e = coded(check("big.pptx", &zip64_with(&head(), 2, 2, 45, &over)), "held");
        assert!(e.contains("ZIP64 end record"), "{e}");
    }

    #[test]
    fn the_directory_fills_exactly_what_its_end_record_declares() {
        let mut parts = head();
        parts.push(Part::stored("ppt/slides/slide1.xml", b"<p:sld/>"));
        let l = layout(&parts);
        let (size, at) = (l.central.len() as u32, l.locals.len() as u32);
        assert_eq!(check("ok.pptx", &with_end(&l, end_record(3, 3, size, at, b""))), Ok(Format::Pptx));
        // Declared shorter, or longer, than the headers there.
        let e = coded(check("short.pptx", &with_end(&l, end_record(3, 3, size - 10, at, b""))), "damaged");
        assert!(e.contains("size and place"), "{e}");
        coded(check("long.pptx", &with_end(&l, end_record(3, 3, size + 10, at, b""))), "damaged");
        // A header past the count: two counted, three there.
        let e = coded(check("past.pptx", &with_end(&l, end_record(2, 2, size, at, b""))), "damaged");
        assert!(e.contains("size and place"), "{e}");
        // A directory that starts elsewhere than where it is.
        coded(check("moved.pptx", &with_end(&l, end_record(3, 3, size, at - 1, b""))), "damaged");
    }

    #[test]
    fn one_end_record_ends_the_file() {
        let l = layout(&head());
        let (size, at) = (l.central.len() as u32, l.locals.len() as u32);
        let end = end_record(2, 2, size, at, b"");
        assert_eq!(check("note.pptx", &with_end(&l, end_record(2, 2, size, at, b"made by hand"))), Ok(Format::Pptx));
        // Two end records, back to back.
        coded(check("two.pptx", &[with_end(&l, end.clone()), end.clone()].concat()), "damaged");
        // One hidden in the other's comment, which a reader searching from
        // the end of the file would try first.
        let comment = [end.clone(), b"pad".to_vec()].concat();
        let e = coded(check("hidden.pptx", &with_end(&l, end_record(2, 2, size, at, &comment))), "damaged");
        assert!(e.contains("second end record"), "{e}");
        coded(check("hidden.pptx", &with_end(&l, end_record(2, 2, size, at, &end))), "damaged");
        // Anything after the comment.
        coded(check("tail.pptx", &[with_end(&l, end), b"tail".to_vec()].concat()), "damaged");
    }

    #[test]
    fn nothing_comes_before_the_zip() {
        // Data before a zip whose offsets count from the zip's own start:
        // the engine's zip reader would move every offset, the guard none.
        let z = package(vec![], "");
        coded(check("junk.pptx", &[b"PK\x03\x04 prepended".to_vec(), z.clone()].concat()), "damaged");
        // Data before a zip whose offsets count from the file's start.
        let junk = b"PK\x03\x04 prepended".to_vec();
        let l = layout_from(&head(), junk.len() as u32);
        let at = (junk.len() + l.locals.len()) as u32;
        let shifted = [junk, l.locals.clone(), l.central.clone(), end_record(2, 2, l.central.len() as u32, at, b"")].concat();
        let e = coded(check("before.pptx", &shifted), "damaged");
        assert!(e.contains("data before its first entry"), "{e}");
        // Data that isn't even a zip's start makes no zip.
        coded(check("exe.pptx", &[b"MZ\x90\0".to_vec(), z].concat()), "unsupported");
    }

    #[test]
    fn extra_fields_read_as_the_engines_zip_reader_reads_them() {
        let with = |extra: Vec<u8>| package(vec![Part { extra, ..Part::stored("ppt/slides/slide1.xml", b"<p:sld/>") }], "");
        let ntfs = |tag: u16, size: u16| field(0x000a, &[vec![0u8; 4], tag.to_le_bytes().to_vec(), size.to_le_bytes().to_vec(), vec![0u8; 24]].concat());
        let times = |flags: u8, len: usize| field(0x5455, &[vec![flags], vec![0u8; len - 1]].concat());
        // The shapes that reader takes, and a field it skips, pass.
        for extra in [ntfs(1, 24), times(1, 5), times(3, 9), times(7, 5), field(0xCAFE, b"ok")] {
            assert_eq!(check("t.pptx", &with(extra)), Ok(Format::Pptx));
        }
        // The ones it refuses, the guard refuses first.
        for extra in [ntfs(2, 24), ntfs(1, 16), times(1, 9), times(0x09, 5), field(0x9901, &[2, 0, 0x41, 0x45, 3, 8, 0]), [field(0xCAFE, b"ok"), vec![1, 2]].concat()] {
            coded(check("t.pptx", &with(extra)), "damaged");
        }
        // Info-ZIP's Unicode path would rename the entry for the engine.
        let e = coded(check("t.pptx", &with(field(0x7075, &[1, 0, 0, 0, 0, b'x']))), "damaged");
        assert!(e.contains("Unicode"), "{e}");
        coded(check("t.pptx", &with(field(0x6375, &[1, 0, 0, 0, 0, b'x']))), "damaged");
        // A field running past the extra field.
        coded(check("t.pptx", &with([0xCAFEu16.to_le_bytes(), 9u16.to_le_bytes()].concat())), "damaged");
        // ZIP64: a field of 24 bytes gives all three values over the 32-bit
        // ones, to the engine's reader and so to the guard. Here it moves an
        // innocent slide onto a zip no header names, which the guard then
        // finds as that reader would.
        let hidden = zip(&[Part::stored("a", b"b")]);
        let n = hidden.len() as u64;
        let moved = field(0x0001, &[n.to_le_bytes(), n.to_le_bytes(), 0u64.to_le_bytes()].concat());
        let parts = vec![
            Part { hidden: true, ..Part::stored("hidden.bin", &hidden) },
            Part::deflated("[Content_Types].xml", b"<Types/>"),
            Part::deflated("ppt/presentation.xml", b"<p:presentation/>"),
            Part { extra: moved, ..Part::stored("ppt/slides/slide1.xml", b"<p:sld/>") },
        ];
        let e = coded(check("moved.pptx", &zip(&parts)), "held");
        assert!(e.contains("zip inside its zip") && e.contains("ppt/slides/slide1.xml"), "{e}");
        // And a ZIP64 field too short for what it must give.
        let short = Part { declared: Some(0xFFFF_FFFF), extra: field(0x0001, &[0u8; 4]), ..Part::stored("ppt/x.xml", b"x") };
        coded(check("short.pptx", &package(vec![short], "")), "damaged");
    }

    #[test]
    fn what_the_engines_zip_reader_sees_differently_is_refused() {
        // "é" in UTF-8 (flagged) and in code page 437 (not flagged): one name
        // to the engine's zip reader, which keeps the second's data under it,
        // and two to the guard.
        let utf8 = Part { flags: 1 << 11, ..Part::stored("ppt/é.xml", b"<a/>") };
        let cp437 = Part { raw_name: Some(b"ppt/\x82.xml".to_vec()), ..Part::stored("ppt/cp437.xml", b"<b/>") };
        let e = coded(check("twins.pptx", &package(vec![utf8, cp437], "")), "damaged");
        assert!(e.contains("reads the presentation differently") && e.contains("3 entries, not 4"), "{e}");
        // The same name twice.
        coded(check("twice.pptx", &package(vec![Part::stored("ppt/a.xml", b"1"), Part::stored("ppt/a.xml", b"2")], "")), "damaged");
    }

    /// The guard's reader of record is the engine's own: the deck service
    /// links the very `zip` crate deckcraft's package and native readers do.
    #[test]
    fn the_guard_reads_alike_with_the_engines_own_zip_crate() {
        let lock = include_str!("../../../../Cargo.lock");
        let zip_of = |package: &str| -> String {
            let block = lock.split("[[package]]").find(|b| b.contains(&format!("name = \"{package}\"\n"))).unwrap_or_else(|| panic!("{package} is not in Cargo.lock"));
            let deps = block.split("dependencies = [").nth(1).unwrap_or_else(|| panic!("{package} has no dependencies"));
            deps.lines().map(|l| l.trim().trim_matches(|c| c == '"' || c == ',')).find(|d| *d == "zip" || d.starts_with("zip ")).unwrap_or_else(|| panic!("{package} has no zip")).to_string()
        };
        let ours = zip_of("octosense-deck-service");
        assert_eq!(ours, zip_of("deckcraft-pptx"));
        assert_eq!(ours, zip_of("deckcraft-format"));
    }
}
