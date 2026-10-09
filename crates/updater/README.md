# OctoSense updater core

English | [简体中文](README.zh-CN.md)

`octosense-updater` checks OctoSense's public GitHub releases and downloads a verified installer into host-owned private storage. It does not install, run a shell command, replace a running executable, or expose a service to app agents. The shell must ask the person to open the native installer after download.

`Client::check` and `Client::download` are synchronous worker-thread entry points. Network I/O uses a private Tokio runtime so cancellation can interrupt a stalled request; UI events must never call these methods directly. Cloned clients share a busy guard. Failures and cancellation release it, remove that attempt's partial file, and permit Retry. Cancellation does not interrupt a subsequent native installer that the person has opened.

Release identity comes from `OCTOSENSE_RELEASE_TAG` baked by the packaging job, such as `desktop-v0.1.0-rc.2` or `home-v0.1.0-beta.2`. An unset, empty or invalid baked tag is a development build, not a fabricated installed version. `BuildIdentity::default_channel` uses Stable for stable releases, ReleaseCandidate for `rc` releases, Preview for other prereleases and development builds. A check on a development build can show the latest release, but the UI must explain that its installed version cannot be compared. The package's Cargo version is never used for update comparison.

| Channel | Eligible releases |
| --- | --- |
| Stable | Non-draft, non-prerelease GitHub releases with stable semantic versions |
| ReleaseCandidate | Stable releases and `rc` / `rc.<number>` prereleases |
| Preview | Stable releases and all semantic-version prereleases |

Selection uses semantic-version precedence, not publication time; `rc.10` is newer than `rc.2`, and the final version is newer than both. Home and desktop are separate tracks. Exact asset names bind the selected version and platform: macOS ARM64 DMG, Windows x64 setup EXE, Linux x64 AppImage or DEB, Android ARM64 Home APK. Linux selects DEB only for `/usr/bin/octosense`, or AppImage when its launcher environment and running executable agree. Unknown/manual layouts are explicitly unsupported rather than switching formats. Home does not download or replace the privileged Bridge APK; a ROM update is a separate delivery path.

An offer is created only after the exact asset URL, GitHub SHA-256 digest, and release's `SHA256SUMS` agree. The checksum file itself must match its GitHub digest. Initial URLs are fixed to the official repository and selected tag/name. HTTPS redirects are limited to GitHub's release CDN hosts and the official repository's download path. No tokens, cookies or developer-configurable download endpoint are used. These are integrity checks under trust in the project's GitHub release; they are not an independent publisher signature. Native platform signing checks remain necessary, especially Android's installed-package signer check.

Metadata is bounded to 2 MiB per page, ten 100-release pages and two minutes per check. Checksum files are bounded to 256 KiB. Installers are bounded to 1 GiB on desktop or 512 MiB on Android, 20 seconds without network progress and 15 minutes total. Download size and SHA-256 are checked while streaming, then the private staging directory is atomically renamed. Each attempt has a unique directory. A failed attempt cannot replace a previously verified artifact. Windows storage inherits the host's private app-data ACL; Unix cache directories require mode 0700 and files are created as 0600. The host creates the cache's parent; the updater creates the final cache directory. The OS may reclaim cache contents, in which case the person downloads again.

`VerifiedArtifact` exposes read-only identity and path accessors. Call `revalidate()` on a worker immediately before native handoff; it checks file type, confinement, exact size and SHA-256 again. It does not persist a reusable trust receipt across process restarts. Call `discard()` when an unused download is abandoned before native handoff; it deletes only that installer and its empty private directory. Never discard after opening a native installer that may still read the file. Other completed files remain in cache; abrupt process death may leave an unverified pending directory, which is never considered installable.

The `check` example supports `--product`, `--platform`, `--installed`, `--stable`, `--rc`, `--download`, and `--cache`. It prints a JSON result, never launches an installer, and downloads only when `--download` is explicit. Unit tests cover version/channel/platform selection, malicious release metadata, redirect restrictions, checksum disagreements, cancellation, incomplete transfers, retry, cache permissions and changed or symlinked artifacts. See the PR validation record for commands actually run and live-release acceptance results.
