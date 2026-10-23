//! Code signatures, checked with Apple's Security framework: no `codesign`
//! process and no added crate. Only code that satisfies the installed app's
//! own signing identity may replace it, or be launched by the update helper.
use super::Result;
use std::{ffi::c_void, os::unix::ffi::OsStrExt, path::Path, ptr};

type CFTypeRef = *const c_void;
type OSStatus = i32;

const UTF8: u32 = 0x0800_0100; // kCFStringEncodingUTF8
const CHECK_ALL_ARCHITECTURES: u32 = 1 << 0;
const CHECK_NESTED_CODE: u32 = 1 << 3;
const STRICT_VALIDATE: u32 = 1 << 4;
const RESTRICT_SYMLINKS: u32 = 1 << 7;
const RESTRICT_SIDEBAND_DATA: u32 = 1 << 9;
/// `codesign --verify --deep --strict` (which the release workflow runs on
/// the app it signs), for every architecture.
const VALIDATE: u32 = CHECK_ALL_ARCHITECTURES
    | CHECK_NESTED_CODE
    | STRICT_VALIDATE
    | RESTRICT_SYMLINKS
    | RESTRICT_SIDEBAND_DATA;
const SIGNING_INFORMATION: u32 = 1 << 1; // kSecCSSigningInformation
const UNSIGNED: OSStatus = -67062; // errSecCSUnsigned
const REQUIREMENT_FAILED: OSStatus = -67050; // errSecCSReqFailed
const UNSIGNED_INSTALLATION: &str = "Install & Restart needs a signed OctoSense release, and this installation is unsigned or ad-hoc signed (a source or development build). Use Open installer manually.";

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(value: CFTypeRef);
    fn CFGetTypeID(value: CFTypeRef) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFURLCreateFromFileSystemRepresentation(
        allocator: CFTypeRef,
        path: *const u8,
        length: isize,
        directory: u8,
    ) -> CFTypeRef;
    fn CFStringCreateWithBytes(
        allocator: CFTypeRef,
        bytes: *const u8,
        length: isize,
        encoding: u32,
        external: u8,
    ) -> CFTypeRef;
    fn CFStringGetLength(string: CFTypeRef) -> isize;
    fn CFStringGetMaximumSizeForEncoding(length: isize, encoding: u32) -> isize;
    fn CFStringGetCString(string: CFTypeRef, buffer: *mut u8, size: isize, encoding: u32) -> u8;
    fn CFDictionaryGetTypeID() -> usize;
    fn CFDictionaryGetValue(dictionary: CFTypeRef, key: CFTypeRef) -> CFTypeRef;
}

#[link(name = "Security", kind = "framework")]
extern "C" {
    static kSecCodeInfoIdentifier: CFTypeRef;
    static kSecCodeInfoPList: CFTypeRef;
    static kSecCodeInfoTeamIdentifier: CFTypeRef;
    fn SecStaticCodeCreateWithPath(path: CFTypeRef, flags: u32, code: *mut CFTypeRef) -> OSStatus;
    fn SecStaticCodeCheckValidity(code: CFTypeRef, flags: u32, requirement: CFTypeRef) -> OSStatus;
    fn SecCodeCopyDesignatedRequirement(
        code: CFTypeRef,
        flags: u32,
        requirement: *mut CFTypeRef,
    ) -> OSStatus;
    fn SecCodeCopySigningInformation(
        code: CFTypeRef,
        flags: u32,
        information: *mut CFTypeRef,
    ) -> OSStatus;
    fn SecRequirementCopyString(
        requirement: CFTypeRef,
        flags: u32,
        text: *mut CFTypeRef,
    ) -> OSStatus;
    fn SecRequirementCreateWithString(
        text: CFTypeRef,
        flags: u32,
        requirement: *mut CFTypeRef,
    ) -> OSStatus;
}

/// An owned Core Foundation object, released once.
struct Owned(CFTypeRef);
impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0) }
        }
    }
}

/// Who signed the installed app. The requirement is its designated
/// requirement, joined with an Apple-issued chain for the same Team ID and
/// signing identifier, so a weak or unusual designated requirement cannot
/// admit another developer's code.
pub(super) struct Identity {
    requirement: Owned,
    team: Option<String>,
    identifier: String,
    version: String,
}

/// What a validated signature seals: its Team ID and signing identifier,
/// and the release version in the Info.plist it validated.
struct Signer {
    team: Option<String>,
    identifier: Option<String>,
    version: Option<String>,
}

impl Identity {
    /// The installed app's `CFBundleShortVersionString`, as its validated
    /// signature sealed it, rather than as the file reads later.
    pub(super) fn version(&self) -> &str {
        &self.version
    }

    /// The installed app's identity, after strict validation of the installed
    /// bundle itself (an unvalidated signature's requirement means nothing).
    /// An unsigned or ad-hoc signed installation, which is every source or
    /// development build, names no developer and is refused.
    pub(super) fn installed(bundle: &Path) -> Result<Self> {
        let unreadable =
            |reason: String| format!("The installed app {reason}. Use Open installer manually.");
        let code = static_code(bundle).map_err(unreadable)?;
        match validate(&code, ptr::null()) {
            0 => {}
            UNSIGNED => return Err(UNSIGNED_INSTALLATION.into()),
            status => {
                return Err(format!(
                    "The installed app's code signature is not valid ({status}), so it cannot vouch for an update. Use Open installer manually."
                ))
            }
        }
        let signer = signer(&code).map_err(unreadable)?;
        let identifier = signer
            .identifier
            .filter(|id| plain_identifier(id))
            .ok_or(UNSIGNED_INSTALLATION)?;
        let team = signer
            .team
            .filter(|team| team_id(team))
            .ok_or(UNSIGNED_INSTALLATION)?;
        let version = signer
            .version
            .ok_or("The installed app's signed Info.plist has no version.")?;
        let designated = designated(&code)?;
        let requirement = compile(&format!(
            "({designated}) and anchor apple generic and certificate leaf[subject.OU] = \"{team}\" and identifier \"{identifier}\""
        ))?;
        // The installed app must meet its own requirement. A signature this
        // cannot describe (a Mac App Store one, whose leaf certificate is
        // Apple's) would refuse every update with a misleading reason.
        if validate(&code, requirement.0) != 0 {
            return Err("The installed app's signature cannot identify an update from the same developer. Use Open installer manually.".into());
        }
        Ok(Self {
            requirement,
            team: Some(team),
            identifier,
            version,
        })
    }

    /// Tests only: an ad-hoc signed fixture's identity, its designated
    /// requirement alone (the exact code directory hash). Release code has no
    /// way to build an identity without a Team ID.
    #[cfg(test)]
    pub(super) fn ad_hoc_fixture(bundle: &Path) -> Result<Self> {
        let code = static_code(bundle)?;
        if validate(&code, ptr::null()) != 0 {
            return Err("fixture is not validly signed".into());
        }
        let signer = signer(&code)?;
        Ok(Self {
            requirement: compile(&designated(&code)?)?,
            team: signer.team,
            identifier: signer.identifier.ok_or("fixture is unsigned")?,
            version: signer.version.ok_or("fixture has no version")?,
        })
    }

    /// Strictly validates `bundle`, every architecture and nested code
    /// included, against this identity, and returns its
    /// `CFBundleShortVersionString` as the signature sealed it. An error is a
    /// phrase that follows the bundle's name: "The update" + " is not signed".
    pub(super) fn admit(&self, bundle: &Path) -> Result<String> {
        let code = static_code(bundle)?;
        match validate(&code, self.requirement.0) {
            0 => {}
            UNSIGNED => return Err("is not signed".into()),
            REQUIREMENT_FAILED => {
                return Err("is not signed by the installed app's developer".into())
            }
            status => return Err(format!("has an invalid code signature ({status})")),
        }
        let signer = signer(&code)?;
        if signer.team != self.team
            || signer.identifier.as_deref() != Some(self.identifier.as_str())
        {
            return Err("is not signed by the installed app's developer".into());
        }
        signer
            .version
            .ok_or_else(|| "has no version in its signed Info.plist".into())
    }
}

fn static_code(bundle: &Path) -> Result<Owned> {
    let path = bundle.as_os_str().as_bytes();
    let url = Owned(unsafe {
        CFURLCreateFromFileSystemRepresentation(ptr::null(), path.as_ptr(), path.len() as isize, 1)
    });
    if url.0.is_null() {
        return Err("has a path the Security framework cannot read".into());
    }
    let mut code = ptr::null();
    let status = unsafe { SecStaticCodeCreateWithPath(url.0, 0, &mut code) };
    let code = Owned(code);
    if status != 0 || code.0.is_null() {
        return Err(format!(
            "could not be read for signature checking ({status})"
        ));
    }
    Ok(code)
}

fn validate(code: &Owned, requirement: CFTypeRef) -> OSStatus {
    unsafe { SecStaticCodeCheckValidity(code.0, VALIDATE, requirement) }
}

/// What a signature sealed, read from the code object it was just validated
/// on: the Security framework keeps the Info.plist it checked, so the
/// version cannot differ from the validated one (`kSecCodeInfoPList`).
fn signer(code: &Owned) -> Result<Signer> {
    let mut information = ptr::null();
    let status =
        unsafe { SecCodeCopySigningInformation(code.0, SIGNING_INFORMATION, &mut information) };
    let information = Owned(information);
    if status != 0 || information.0.is_null() {
        return Err(format!("has unreadable signing information ({status})"));
    }
    let get =
        |dictionary: CFTypeRef, key: CFTypeRef| unsafe { CFDictionaryGetValue(dictionary, key) };
    let plist = get(information.0, unsafe { kSecCodeInfoPList });
    let version = if !plist.is_null() && unsafe { CFGetTypeID(plist) == CFDictionaryGetTypeID() } {
        let key = cf_string("CFBundleShortVersionString")?;
        string(get(plist, key.0))
    } else {
        None
    };
    Ok(Signer {
        team: string(get(information.0, unsafe { kSecCodeInfoTeamIdentifier })),
        identifier: string(get(information.0, unsafe { kSecCodeInfoIdentifier })),
        version,
    })
}

fn designated(code: &Owned) -> Result<String> {
    let mut requirement = ptr::null();
    let status = unsafe { SecCodeCopyDesignatedRequirement(code.0, 0, &mut requirement) };
    let requirement = Owned(requirement);
    if status != 0 || requirement.0.is_null() {
        return Err(format!(
            "The installed app has no designated requirement ({status})."
        ));
    }
    let mut text = ptr::null();
    let status = unsafe { SecRequirementCopyString(requirement.0, 0, &mut text) };
    let text = Owned(text);
    if status != 0 {
        return Err(format!(
            "The installed app's designated requirement is unreadable ({status})."
        ));
    }
    string(text.0).ok_or_else(|| "The installed app's designated requirement is unreadable.".into())
}

fn cf_string(text: &str) -> Result<Owned> {
    let value = Owned(unsafe {
        CFStringCreateWithBytes(ptr::null(), text.as_ptr(), text.len() as isize, UTF8, 0)
    });
    if value.0.is_null() {
        return Err("Could not build a string for the Security framework.".into());
    }
    Ok(value)
}

fn compile(text: &str) -> Result<Owned> {
    let text = cf_string(text)?;
    let mut requirement = ptr::null();
    let status = unsafe { SecRequirementCreateWithString(text.0, 0, &mut requirement) };
    let requirement = Owned(requirement);
    if status != 0 || requirement.0.is_null() {
        return Err(format!(
            "Could not build the signing requirement ({status})."
        ));
    }
    Ok(requirement)
}

/// A borrowed CFString's text; None for null, another type or invalid UTF-8.
fn string(value: CFTypeRef) -> Option<String> {
    if value.is_null() || unsafe { CFGetTypeID(value) != CFStringGetTypeID() } {
        return None;
    }
    let size = unsafe { CFStringGetMaximumSizeForEncoding(CFStringGetLength(value), UTF8) }
        .checked_add(1)
        .filter(|size| (1..=65536).contains(size))?;
    let mut buffer = vec![0u8; size as usize];
    if unsafe { CFStringGetCString(value, buffer.as_mut_ptr(), size, UTF8) } == 0 {
        return None;
    }
    buffer.truncate(buffer.iter().position(|b| *b == 0)?);
    String::from_utf8(buffer).ok()
}

/// Apple Team IDs: ten uppercase letters or digits. Checked before either
/// value is written into the requirement text.
fn team_id(team: &str) -> bool {
    team.len() == 10
        && team
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
fn plain_identifier(identifier: &str) -> bool {
    !identifier.is_empty()
        && identifier.len() <= 255
        && identifier
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_')
}
