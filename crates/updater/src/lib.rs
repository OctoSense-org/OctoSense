//! Host-owned, explicit OctoSense updates from the project's public GitHub releases.
//!
//! This crate checks and downloads; it never launches installers or replaces the
//! running application. Run its blocking public methods on a worker thread. No
//! account, GitHub token, shell command or application-agent capability is used.

use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fmt,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

const RELEASES_API: &str = "https://api.github.com/repos/OctoSense-org/OctoSense/releases";
const RELEASES_WEB: &str = "https://github.com/OctoSense-org/OctoSense/releases";
const MAX_METADATA: u64 = 2 * 1024 * 1024;
const MAX_SUMS: u64 = 256 * 1024;
const MAX_INSTALLER: u64 = 1024 * 1024 * 1024;
const MAX_ANDROID_INSTALLER: u64 = 512 * 1024 * 1024;
const NETWORK_IDLE: Duration = Duration::from_secs(20);
const DOWNLOAD_DEADLINE: Duration = Duration::from_secs(15 * 60);

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Busy,
    Cancelled,
    Network,
    RateLimited,
    Timeout,
    InvalidMetadata(&'static str),
    Integrity(&'static str),
    Storage(&'static str),
    Unsupported,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Busy => f.write_str("An update operation is already running."),
            Self::Cancelled => f.write_str("Update cancelled; no installer was opened."),
            Self::Network => f.write_str(
                "Could not reach the public release server. Check your connection and retry.",
            ),
            Self::RateLimited => {
                f.write_str("GitHub temporarily limited update checks. Try again later.")
            }
            Self::Timeout => f.write_str(
                "The release download timed out. Retry when the connection is available.",
            ),
            Self::InvalidMetadata(reason) => write!(f, "Release metadata was rejected: {reason}"),
            Self::Integrity(reason) => write!(f, "Update verification failed: {reason}"),
            Self::Storage(reason) => write!(f, "Update storage is unavailable: {reason}"),
            Self::Unsupported => f.write_str("No updater package is supported for this platform."),
        }
    }
}
impl std::error::Error for Error {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Product {
    Desktop,
    Home,
}
impl Product {
    fn prefix(self) -> &'static str {
        match self {
            Self::Desktop => "desktop-v",
            Self::Home => "home-v",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Stable,
    ReleaseCandidate,
    Preview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacArm64,
    WindowsX64,
    LinuxX64AppImage,
    LinuxX64Deb,
    AndroidArm64,
}
impl Platform {
    pub fn current(product: Product) -> Option<Self> {
        match (product, std::env::consts::OS, std::env::consts::ARCH) {
            (Product::Desktop, "macos", "aarch64") => Some(Self::MacArm64),
            (Product::Desktop, "windows", "x86_64") => Some(Self::WindowsX64),
            (Product::Desktop, "linux", "x86_64") => {
                let executable = std::env::current_exe().ok()?.canonicalize().ok()?;
                let app_dir = std::env::var_os("APPDIR").and_then(|p| {
                    let path = PathBuf::from(p);
                    path.is_absolute()
                        .then(|| path.canonicalize().ok())
                        .flatten()
                });
                let app_image_is_file = std::env::var_os("APPIMAGE").is_some_and(|p| {
                    let path = PathBuf::from(p);
                    path.is_absolute() && path.is_file()
                });
                linux_platform(&executable, app_dir.as_deref(), app_image_is_file)
            }
            (Product::Home, "android", "aarch64") => Some(Self::AndroidArm64),
            _ => None,
        }
    }
    pub fn product(self) -> Product {
        if self == Self::AndroidArm64 {
            Product::Home
        } else {
            Product::Desktop
        }
    }
    fn maximum_size(self) -> u64 {
        if self == Self::AndroidArm64 {
            MAX_ANDROID_INSTALLER
        } else {
            MAX_INSTALLER
        }
    }
    fn asset_name(self, version: &Version) -> String {
        match self {
            Self::MacArm64 => format!("OctoSense_{version}_aarch64.dmg"),
            Self::WindowsX64 => format!("octosense_{version}_x64-setup.exe"),
            Self::LinuxX64AppImage => format!("octosense_{version}_x86_64.AppImage"),
            Self::LinuxX64Deb => format!("octosense_{version}_amd64.deb"),
            Self::AndroidArm64 => format!("OctoSenseHome_{version}_arm64.apk"),
        }
    }
}

// The DEB and AppImage ship the same binary. Never infer the install format
// from a distro name or silently switch an existing DEB install to AppImage.
fn linux_platform(
    executable: &Path,
    app_dir: Option<&Path>,
    app_image_is_file: bool,
) -> Option<Platform> {
    if app_image_is_file && app_dir.is_some_and(|dir| executable == dir.join("usr/bin/octosense")) {
        return Some(Platform::LinuxX64AppImage);
    }
    if executable == Path::new("/usr/bin/octosense") {
        return Some(Platform::LinuxX64Deb);
    }
    None
}

/// A release tag must be baked by the packaging job. Cargo's package version is
/// not the installed release: untagged local builds remain development builds.
#[derive(Debug, Clone)]
pub struct BuildIdentity {
    product: Product,
    tag: Option<String>,
    version: Option<Version>,
}
impl BuildIdentity {
    pub fn from_env(product: Product) -> Self {
        Self::from_tag(product, option_env!("OCTOSENSE_RELEASE_TAG")).unwrap_or(Self {
            product,
            tag: None,
            version: None,
        })
    }
    pub fn from_tag(product: Product, tag: Option<&str>) -> Result<Self> {
        match tag.filter(|t| !t.is_empty()) {
            Some(tag) => Ok(Self {
                product,
                tag: Some(tag.into()),
                version: Some(parse_tag(product, tag)?),
            }),
            None => Ok(Self {
                product,
                tag: None,
                version: None,
            }),
        }
    }
    pub fn default_channel(&self) -> Channel {
        match self.version.as_ref() {
            Some(v) if is_release_candidate(v) => Channel::ReleaseCandidate,
            Some(v) if !v.pre.is_empty() => Channel::Preview,
            None => Channel::Preview,
            _ => Channel::Stable,
        }
    }
    pub fn product(&self) -> Product {
        self.product
    }
    pub fn tag(&self) -> Option<&str> {
        self.tag.as_deref()
    }
    pub fn version(&self) -> Option<&Version> {
        self.version.as_ref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Available,
    Current,
    Development,
    Unavailable,
}
#[derive(Debug, Clone)]
pub struct CheckResult {
    pub status: CheckStatus,
    pub offer: Option<Offer>,
}

/// Only a successful check can construct an offer. Network locations and
/// digests are private so application code cannot turn this into an arbitrary
/// URL downloader or pass unchecked metadata to the installer path.
#[derive(Debug, Clone)]
pub struct Offer {
    tag: String,
    version: Version,
    platform: Platform,
    asset: ReleaseAsset,
    sha256: String,
}
impl Offer {
    pub fn tag(&self) -> &str {
        &self.tag
    }
    pub fn version(&self) -> &Version {
        &self.version
    }
    pub fn platform(&self) -> Platform {
        self.platform
    }
    pub fn asset_name(&self) -> &str {
        &self.asset.name
    }
    pub fn size(&self) -> u64 {
        self.asset.size
    }
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
    pub fn release_url(&self) -> String {
        format!("{RELEASES_WEB}/tag/{}", self.tag)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
    fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub struct DownloadProgress {
    pub received: u64,
    pub total: u64,
}

/// Holds an immutable verified identity; the file must still be revalidated
/// immediately before native handoff. The native Android installer additionally
/// checks APK package, version and signer against the installed application.
#[derive(Debug, Clone)]
pub struct VerifiedArtifact {
    offer: Offer,
    root: PathBuf,
    directory: PathBuf,
    path: PathBuf,
}
impl VerifiedArtifact {
    pub fn offer(&self) -> &Offer {
        &self.offer
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn sha256(&self) -> &str {
        self.offer.sha256()
    }
    pub fn platform(&self) -> Platform {
        self.offer.platform()
    }
    pub fn tag(&self) -> &str {
        self.offer.tag()
    }
    pub fn asset_name(&self) -> &str {
        self.offer.asset_name()
    }
    /// Delete an unused download when changing channel or starting over. Never
    /// call after native handoff: a mounted DMG/installer can still need it.
    pub fn discard(self) -> Result<()> {
        self.validate_location()?;
        match fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(Error::Storage("cannot remove unused installer")),
        }
        // No recursive deletion, even if another file appeared in this directory.
        fs::remove_dir(&self.directory)
            .map_err(|_| Error::Storage("cannot remove artifact directory"))
    }
    fn validate_location(&self) -> Result<()> {
        validate_directory(&self.root)?;
        validate_directory(&self.directory)?;
        let canonical_root = self
            .root
            .canonicalize()
            .map_err(|_| Error::Storage("missing update directory"))?;
        let canonical_directory = self
            .directory
            .canonicalize()
            .map_err(|_| Error::Storage("missing artifact directory"))?;
        if canonical_root != self.root || canonical_directory.parent() != Some(self.root.as_path())
        {
            return Err(Error::Storage("artifact directory changed"));
        }
        if self.path.parent() != Some(self.directory.as_path())
            || self.path.file_name().and_then(|s| s.to_str()) != Some(self.offer.asset_name())
        {
            return Err(Error::Storage("installer path changed"));
        }
        Ok(())
    }
    pub fn revalidate(&self) -> Result<PathBuf> {
        self.validate_location()?;
        let meta =
            fs::symlink_metadata(&self.path).map_err(|_| Error::Storage("installer is missing"))?;
        if !meta.file_type().is_file() || meta.len() != self.offer.size() {
            return Err(Error::Integrity("installer size or file type changed"));
        }
        let mut file =
            File::open(&self.path).map_err(|_| Error::Storage("cannot read installer"))?;
        let mut hasher = Sha256::new();
        let mut bytes = 0;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let read = file
                .read(&mut buffer)
                .map_err(|_| Error::Storage("cannot read installer"))?;
            if read == 0 {
                break;
            }
            bytes += read as u64;
            if bytes > self.offer.size() {
                return Err(Error::Integrity("installer grew after download"));
            }
            hasher.update(&buffer[..read]);
        }
        if bytes != self.offer.size() || format!("{:x}", hasher.finalize()) != self.offer.sha256 {
            return Err(Error::Integrity("installer changed after download"));
        }
        Ok(self.path.clone())
    }
}

/// Clone shares an operation guard. A failed/cancelled operation releases it so
/// Retry works; partial files never become verified artifacts.
#[derive(Clone)]
pub struct Client {
    cache_dir: PathBuf,
    http: reqwest::Client,
    busy: Arc<AtomicBool>,
}
impl Client {
    pub fn new(cache_dir: impl Into<PathBuf>) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("OctoSense-Updater/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(10))
            // A worker runtime lives for one operation; do not cache connections
            // whose I/O tasks would belong to that completed runtime.
            .pool_max_idle_per_host(0)
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 5 {
                    return attempt.error("too many release redirects");
                }
                if valid_redirect(attempt.url()) {
                    attempt.follow()
                } else {
                    attempt.error("untrusted release redirect")
                }
            }))
            .build()
            .map_err(|_| Error::Network)?;
        Ok(Self {
            cache_dir: cache_dir.into(),
            http,
            busy: Arc::new(AtomicBool::new(false)),
        })
    }
    pub fn check(
        &self,
        current: &BuildIdentity,
        channel: Channel,
        platform: Platform,
        cancel: &Cancellation,
    ) -> Result<CheckResult> {
        let _guard = Operation::enter(&self.busy)?;
        cancel.check()?;
        if platform.product() != current.product {
            return Err(Error::Unsupported);
        }
        run(self.check_async(current, channel, platform, cancel))
    }
    pub fn download(
        &self,
        offer: &Offer,
        cancel: &Cancellation,
        mut progress: impl FnMut(DownloadProgress),
    ) -> Result<VerifiedArtifact> {
        let _guard = Operation::enter(&self.busy)?;
        cancel.check()?;
        run(self.download_async(offer, cancel, &mut progress))
    }
    async fn check_async(
        &self,
        current: &BuildIdentity,
        channel: Channel,
        platform: Platform,
        cancel: &Cancellation,
    ) -> Result<CheckResult> {
        let deadline = Instant::now() + Duration::from_secs(120);
        let mut releases = Vec::new();
        for page in 1..=10 {
            let url = format!("{RELEASES_API}?per_page=100&page={page}");
            let response = self.request(&url, cancel, deadline).await?;
            let bytes = read_response(response, MAX_METADATA, cancel, deadline).await?;
            let page_releases: Vec<Release> = serde_json::from_slice(&bytes)
                .map_err(|_| Error::InvalidMetadata("invalid release list"))?;
            if page_releases.len() > 100 {
                return Err(Error::InvalidMetadata("oversized release list"));
            }
            let count = page_releases.len();
            releases.extend(page_releases);
            if count < 100 {
                break;
            }
            if page == 10 {
                return Err(Error::InvalidMetadata(
                    "release history exceeds safety limit",
                ));
            }
        }
        let Some((release, version, asset)) =
            select_release(releases, current.product, channel, platform)?
        else {
            return Ok(CheckResult {
                status: CheckStatus::Unavailable,
                offer: None,
            });
        };
        let expected_digest = validate_asset(&release.tag_name, &asset, platform.maximum_size())?;
        let sums: Vec<_> = release
            .assets
            .iter()
            .filter(|a| a.name == "SHA256SUMS")
            .collect();
        if sums.len() != 1 {
            return Err(Error::InvalidMetadata(
                "release must include one SHA256SUMS asset",
            ));
        }
        let sums = sums[0];
        let sums_digest = validate_asset(&release.tag_name, sums, MAX_SUMS)?;
        let response = self
            .request(&sums.browser_download_url, cancel, deadline)
            .await?;
        let bytes = read_response(response, MAX_SUMS, cancel, deadline).await?;
        verify_bytes(&bytes, sums.size, &sums_digest)?;
        verify_checksum_manifest(&bytes, &asset.name, &expected_digest)?;
        let status = update_status(current, &version);
        Ok(CheckResult {
            status,
            offer: Some(Offer {
                tag: release.tag_name,
                version,
                platform,
                asset,
                sha256: expected_digest,
            }),
        })
    }
    async fn download_async(
        &self,
        offer: &Offer,
        cancel: &Cancellation,
        progress: &mut impl FnMut(DownloadProgress),
    ) -> Result<VerifiedArtifact> {
        let deadline = Instant::now() + DOWNLOAD_DEADLINE;
        let mut pending = PendingDownload::new(&self.cache_dir, offer)?;
        progress(DownloadProgress {
            received: 0,
            total: offer.size(),
        });
        let mut response = self
            .request(&offer.asset.browser_download_url, cancel, deadline)
            .await?;
        if let Some(length) = response.content_length() {
            if length != offer.size() {
                return Err(Error::Integrity("server returned an unexpected size"));
            }
        }
        let mut last_progress = Instant::now();
        while let Some(chunk) = wait_network(response.chunk(), cancel, deadline).await? {
            pending.append(&chunk)?;
            // A fast CDN can produce thousands of chunks per second. Keep the
            // UI's event queue bounded in practice without hiding completion.
            if last_progress.elapsed() >= Duration::from_millis(100)
                || pending.received == offer.size()
            {
                progress(DownloadProgress {
                    received: pending.received,
                    total: offer.size(),
                });
                last_progress = Instant::now();
            }
        }
        cancel.check()?;
        pending.finish()
    }
    async fn request(
        &self,
        url: &str,
        cancel: &Cancellation,
        deadline: Instant,
    ) -> Result<reqwest::Response> {
        let response = wait_network(
            self.http
                .get(url)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .send(),
            cancel,
            deadline,
        )
        .await?;
        match response.status().as_u16() {
            200 => Ok(response),
            403 | 429 => Err(Error::RateLimited),
            _ => Err(Error::Network),
        }
    }
}

fn run<T>(future: impl std::future::Future<Output = Result<T>>) -> Result<T> {
    // The API deliberately owns a small runtime on its worker. Refuse calls
    // made from an async runtime instead of panicking in block_on.
    if tokio::runtime::Handle::try_current().is_ok() {
        return Err(Error::Storage("updater must run on its own worker thread"));
    }
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| Error::Storage("cannot start updater worker"))?;
    let result = rt.block_on(future);
    // DNS can use an OS blocking resolver. Returning from a cancelled request
    // must not wait indefinitely for that resolver while dropping the runtime.
    rt.shutdown_timeout(Duration::from_millis(100));
    result
}
async fn wait_network<T>(
    future: impl std::future::Future<Output = std::result::Result<T, reqwest::Error>>,
    cancel: &Cancellation,
    deadline: Instant,
) -> Result<T> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .ok_or(Error::Timeout)?;
    let timer = tokio::time::sleep(remaining.min(NETWORK_IDLE));
    tokio::pin!(future, timer);
    loop {
        cancel.check()?;
        tokio::select! {
            result = &mut future => return result.map_err(|_| Error::Network),
            _ = &mut timer => return Err(Error::Timeout),
            _ = tokio::time::sleep(Duration::from_millis(100)) => {},
        }
    }
}
async fn read_response(
    mut response: reqwest::Response,
    maximum: u64,
    cancel: &Cancellation,
    deadline: Instant,
) -> Result<Vec<u8>> {
    if response.content_length().is_some_and(|n| n > maximum) {
        return Err(Error::InvalidMetadata("response exceeds safety limit"));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = wait_network(response.chunk(), cancel, deadline).await? {
        if bytes.len() as u64 + chunk.len() as u64 > maximum {
            return Err(Error::InvalidMetadata("response exceeds safety limit"));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
fn valid_redirect(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.port_or_known_default() == Some(443)
        && url.username().is_empty()
        && url.password().is_none()
        && match url.host_str() {
            Some("release-assets.githubusercontent.com" | "objects.githubusercontent.com") => true,
            Some("github.com") => url
                .path()
                .starts_with("/OctoSense-org/OctoSense/releases/download/"),
            _ => false,
        }
}

#[derive(Debug, Clone, Deserialize)]
struct ReleaseAsset {
    name: String,
    size: u64,
    browser_download_url: String,
    digest: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<ReleaseAsset>,
}
fn parse_tag(product: Product, tag: &str) -> Result<Version> {
    if tag.len() > 100
        || !tag
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".-+".contains(&c))
    {
        return Err(Error::InvalidMetadata("invalid release tag"));
    }
    let version = tag
        .strip_prefix(product.prefix())
        .ok_or(Error::InvalidMetadata("release belongs to another product"))?;
    Version::parse(version).map_err(|_| Error::InvalidMetadata("invalid release version"))
}
fn is_release_candidate(version: &Version) -> bool {
    let pre = version.pre.as_str();
    pre == "rc"
        || pre.strip_prefix("rc.").is_some_and(|suffix| {
            !suffix.is_empty()
                && suffix
                    .split('.')
                    .all(|part| part.bytes().all(|b| b.is_ascii_digit()))
        })
}
fn select_release(
    releases: Vec<Release>,
    product: Product,
    channel: Channel,
    platform: Platform,
) -> Result<Option<(Release, Version, ReleaseAsset)>> {
    let mut candidates = Vec::new();
    for release in releases {
        if release.draft || !release.tag_name.starts_with(product.prefix()) {
            continue;
        }
        let Ok(version) = parse_tag(product, &release.tag_name) else {
            continue;
        };
        match channel {
            Channel::Stable if release.prerelease || !version.pre.is_empty() => continue,
            Channel::ReleaseCandidate
                if !is_release_candidate(&version)
                    && (release.prerelease || !version.pre.is_empty()) =>
            {
                continue
            }
            _ => {}
        }
        let wanted = platform.asset_name(&version);
        let matching: Vec<_> = release
            .assets
            .iter()
            .filter(|a| a.name == wanted)
            .cloned()
            .collect();
        if matching.len() > 1 {
            return Err(Error::InvalidMetadata("ambiguous installer asset"));
        }
        if let Some(asset) = matching.into_iter().next() {
            candidates.push((release, version, asset));
        }
    }
    // API publication order is not version order; old releases can be republished.
    candidates.sort_by(|a, b| b.1.cmp_precedence(&a.1));
    if candidates.len() > 1 && candidates[0].1.cmp_precedence(&candidates[1].1).is_eq() {
        return Err(Error::InvalidMetadata("ambiguous release version"));
    }
    Ok(candidates.into_iter().next())
}
fn update_status(current: &BuildIdentity, newest: &Version) -> CheckStatus {
    match &current.version {
        None => CheckStatus::Development,
        Some(v) if !v.cmp_precedence(newest).is_lt() => CheckStatus::Current,
        Some(_) => CheckStatus::Available,
    }
}
fn digest_string(value: &str) -> Result<String> {
    if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::InvalidMetadata("invalid SHA-256 digest"));
    }
    Ok(value.to_ascii_lowercase())
}
fn validate_asset(tag: &str, asset: &ReleaseAsset, maximum: u64) -> Result<String> {
    if asset.size == 0 || asset.size > maximum {
        return Err(Error::InvalidMetadata("invalid asset size"));
    }
    if asset.name.is_empty()
        || asset.name.len() > 180
        || !asset
            .name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-+".contains(&c))
        || asset.name.starts_with('.')
    {
        return Err(Error::InvalidMetadata("invalid asset name"));
    }
    let expected = format!("{RELEASES_WEB}/download/{tag}/{}", asset.name);
    if asset.browser_download_url != expected {
        return Err(Error::InvalidMetadata(
            "asset URL is outside the selected official release",
        ));
    }
    let digest = asset
        .digest
        .as_deref()
        .and_then(|d| d.strip_prefix("sha256:"))
        .ok_or(Error::InvalidMetadata("GitHub SHA-256 digest is missing"))?;
    digest_string(digest)
}
fn verify_bytes(bytes: &[u8], size: u64, digest: &str) -> Result<()> {
    if bytes.len() as u64 != size || format!("{:x}", Sha256::digest(bytes)) != digest {
        return Err(Error::Integrity(
            "release checksum file does not match GitHub metadata",
        ));
    }
    Ok(())
}
fn verify_checksum_manifest(bytes: &[u8], asset: &str, expected: &str) -> Result<()> {
    let manifest = std::str::from_utf8(bytes)
        .map_err(|_| Error::InvalidMetadata("checksums are not UTF-8"))?;
    let mut seen = HashMap::new();
    for line in manifest.lines().filter(|line| !line.is_empty()) {
        if line.len() < 67 || !line.is_char_boundary(64) || !line.is_char_boundary(66) {
            return Err(Error::InvalidMetadata("malformed checksum line"));
        }
        let digest = digest_string(&line[..64])?;
        if &line[64..66] != "  " && &line[64..66] != " *" {
            return Err(Error::InvalidMetadata("malformed checksum separator"));
        }
        let name = &line[66..];
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-+".contains(&b))
            || name.starts_with('.')
        {
            return Err(Error::InvalidMetadata("unsafe checksum filename"));
        }
        if seen.insert(name, digest).is_some() {
            return Err(Error::InvalidMetadata("duplicate checksum filename"));
        }
    }
    match seen.get(asset) {
        Some(digest) if digest == expected => Ok(()),
        Some(_) => Err(Error::Integrity("GitHub digest and SHA256SUMS disagree")),
        None => Err(Error::Integrity("installer is missing from SHA256SUMS")),
    }
}

struct Operation(Arc<AtomicBool>);
impl Operation {
    fn enter(busy: &Arc<AtomicBool>) -> Result<Self> {
        busy.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .map_err(|_| Error::Busy)?;
        Ok(Self(busy.clone()))
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
fn validate_directory(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path)
        .map_err(|_| Error::Storage("cannot inspect update directory"))?;
    if !meta.file_type().is_dir() {
        return Err(Error::Storage("update directory is not a real directory"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o077 != 0 {
            return Err(Error::Storage("update directory must be private"));
        }
    }
    Ok(())
}
fn create_private_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .mode(0o700)
            .create(path)
            .map_err(|_| Error::Storage("cannot create private update directory"))?;
    }
    #[cfg(not(unix))]
    fs::create_dir(path).map_err(|_| Error::Storage("cannot create update directory"))?;
    validate_directory(path)
}
fn prepare_root(path: &Path) -> Result<PathBuf> {
    if fs::symlink_metadata(path).is_err() {
        // Parent is host-owned app storage; caller must create it. Never recurse
        // into a path selected by release metadata.
        create_private_directory(path)?;
    }
    validate_directory(path)?;
    path.canonicalize()
        .map_err(|_| Error::Storage("cannot locate update directory"))
}
struct PendingDownload {
    offer: Offer,
    root: PathBuf,
    directory: PathBuf,
    path: PathBuf,
    file: Option<File>,
    hasher: Sha256,
    received: u64,
    committed: bool,
}
impl PendingDownload {
    fn new(root: &Path, offer: &Offer) -> Result<Self> {
        validate_asset(&offer.tag, &offer.asset, offer.platform.maximum_size())?;
        let root = prepare_root(root)?;
        let directory = root.join(format!("pending-{}", uuid::Uuid::new_v4()));
        create_private_directory(&directory)?;
        let path = directory.join(offer.asset_name());
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = match options.open(&path) {
            Ok(file) => file,
            Err(_) => {
                let _ = fs::remove_dir(&directory);
                return Err(Error::Storage("cannot create download file"));
            }
        };
        Ok(Self {
            offer: offer.clone(),
            root,
            directory,
            path,
            file: Some(file),
            hasher: Sha256::new(),
            received: 0,
            committed: false,
        })
    }
    fn append(&mut self, bytes: &[u8]) -> Result<()> {
        let next = self
            .received
            .checked_add(bytes.len() as u64)
            .ok_or(Error::Integrity("download size overflow"))?;
        if next > self.offer.size() {
            return Err(Error::Integrity("download exceeds advertised size"));
        }
        self.file
            .as_mut()
            .ok_or(Error::Storage("download is already closed"))?
            .write_all(bytes)
            .map_err(|_| Error::Storage("cannot write download; check free space"))?;
        self.hasher.update(bytes);
        self.received = next;
        Ok(())
    }
    fn finish(mut self) -> Result<VerifiedArtifact> {
        if self.received != self.offer.size() {
            return Err(Error::Integrity("download was incomplete"));
        }
        if format!("{:x}", self.hasher.clone().finalize()) != self.offer.sha256 {
            return Err(Error::Integrity("download checksum does not match release"));
        }
        self.file
            .take()
            .ok_or(Error::Storage("download is already closed"))?
            .sync_all()
            .map_err(|_| Error::Storage("cannot finish download"))?;
        let directory = self.root.join(format!("verified-{}", uuid::Uuid::new_v4()));
        fs::rename(&self.directory, &directory)
            .map_err(|_| Error::Storage("cannot commit verified download"))?;
        self.committed = true;
        Ok(VerifiedArtifact {
            offer: self.offer.clone(),
            root: self.root.clone(),
            path: directory.join(self.offer.asset_name()),
            directory,
        })
    }
}
impl Drop for PendingDownload {
    fn drop(&mut self) {
        self.file.take();
        if !self.committed {
            // Remove only files we created, never a recursive tree supplied by
            // metadata and never another operation's verified artifact.
            let _ = fs::remove_file(&self.path);
            let _ = fs::remove_dir(&self.directory);
        }
    }
}

#[cfg(test)]
mod tests;
