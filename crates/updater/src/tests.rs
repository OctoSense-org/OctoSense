use super::*;

fn asset(tag: &str, name: &str, bytes: &[u8]) -> ReleaseAsset {
    ReleaseAsset {
        name: name.into(),
        size: bytes.len() as u64,
        browser_download_url: format!("{RELEASES_WEB}/download/{tag}/{name}"),
        digest: Some(format!("sha256:{:x}", Sha256::digest(bytes))),
    }
}
fn release(tag: &str, platform: Platform) -> Release {
    let version = parse_tag(platform.product(), tag).unwrap();
    Release {
        tag_name: tag.into(),
        draft: false,
        prerelease: !version.pre.is_empty(),
        assets: vec![asset(tag, &platform.asset_name(&version), b"installer")],
    }
}
fn offer(bytes: &[u8]) -> Offer {
    let tag = "desktop-v0.1.0-rc.2";
    let version = parse_tag(Product::Desktop, tag).unwrap();
    let asset = asset(tag, &Platform::MacArm64.asset_name(&version), bytes);
    let sha256 = validate_asset(tag, &asset, MAX_INSTALLER).unwrap();
    Offer {
        tag: tag.into(),
        version,
        platform: Platform::MacArm64,
        asset,
        sha256,
    }
}

#[test]
fn untagged_build_is_not_a_release() {
    for tag in [None, Some("")] {
        let build = BuildIdentity::from_tag(Product::Desktop, tag).unwrap();
        assert!(build.tag().is_none());
        assert!(build.version().is_none());
    }
    assert!(BuildIdentity::from_tag(Product::Desktop, Some("home-v0.1.0")).is_err());
    assert!(BuildIdentity::from_tag(Product::Desktop, Some("0.1.0")).is_err());
}
#[test]
fn rejects_unsafe_and_non_semver_tags() {
    for tag in [
        "desktop-v../../bad",
        "desktop-v0.1.0/evil",
        "desktop-v0.1.0%2f",
        "desktop-v0.01.0",
        "desktop-v0.1.0\n",
        "desktop-v0.1.0?token=x",
    ] {
        assert!(parse_tag(Product::Desktop, tag).is_err(), "{tag}");
    }
    assert_eq!(
        parse_tag(Product::Home, "home-v1.2.3-beta.4").unwrap(),
        Version::parse("1.2.3-beta.4").unwrap()
    );
}
#[test]
fn selects_semver_not_publication_order() {
    let releases = vec![
        release("desktop-v0.1.0-rc.2", Platform::MacArm64),
        release("desktop-v0.1.0-rc.10", Platform::MacArm64),
        release("desktop-v0.1.0-beta.99", Platform::MacArm64),
    ];
    let selected = select_release(
        releases,
        Product::Desktop,
        Channel::Preview,
        Platform::MacArm64,
    )
    .unwrap()
    .unwrap();
    assert_eq!(selected.0.tag_name, "desktop-v0.1.0-rc.10");
}
#[test]
fn stable_excludes_all_preview_sources() {
    let mut misleading = release("desktop-v0.3.0-rc.1", Platform::MacArm64);
    misleading.prerelease = false;
    let mut flag_only = release("desktop-v0.4.0", Platform::MacArm64);
    flag_only.prerelease = true;
    let all = vec![
        misleading,
        flag_only,
        release("desktop-v0.2.0", Platform::MacArm64),
    ];
    let chosen = select_release(all, Product::Desktop, Channel::Stable, Platform::MacArm64)
        .unwrap()
        .unwrap();
    assert_eq!(chosen.0.tag_name, "desktop-v0.2.0");
}
#[test]
fn stable_reports_none_if_only_previews_exist() {
    assert!(select_release(
        vec![release("desktop-v0.1.0-rc.2", Platform::MacArm64)],
        Product::Desktop,
        Channel::Stable,
        Platform::MacArm64
    )
    .unwrap()
    .is_none());
}
#[test]
fn product_architecture_and_draft_are_fenced() {
    let mut draft = release("desktop-v99.0.0", Platform::MacArm64);
    draft.draft = true;
    let selected = select_release(
        vec![
            draft,
            release("home-v88.0.0", Platform::AndroidArm64),
            release("desktop-v77.0.0", Platform::WindowsX64),
            release("desktop-v0.1.0", Platform::MacArm64),
        ],
        Product::Desktop,
        Channel::Preview,
        Platform::MacArm64,
    )
    .unwrap()
    .unwrap();
    assert_eq!(selected.0.tag_name, "desktop-v0.1.0");
}
#[test]
fn exact_platform_asset_names_match_published_convention() {
    let v = Version::parse("0.1.0-rc.2").unwrap();
    for (platform, expected) in [
        (Platform::MacArm64, "OctoSense_0.1.0-rc.2_aarch64.dmg"),
        (Platform::WindowsX64, "octosense_0.1.0-rc.2_x64-setup.exe"),
        (
            Platform::LinuxX64AppImage,
            "octosense_0.1.0-rc.2_x86_64.AppImage",
        ),
        (Platform::LinuxX64Deb, "octosense_0.1.0-rc.2_amd64.deb"),
        (Platform::AndroidArm64, "OctoSenseHome_0.1.0-rc.2_arm64.apk"),
    ] {
        assert_eq!(platform.asset_name(&v), expected);
    }
}
#[test]
fn duplicate_assets_and_equivalent_versions_are_refused() {
    let mut dup = release("desktop-v0.1.0", Platform::MacArm64);
    dup.assets.push(dup.assets[0].clone());
    assert!(select_release(
        vec![dup],
        Product::Desktop,
        Channel::Stable,
        Platform::MacArm64
    )
    .is_err());
    assert!(select_release(
        vec![
            release("desktop-v0.1.0+first", Platform::MacArm64),
            release("desktop-v0.1.0+second", Platform::MacArm64)
        ],
        Product::Desktop,
        Channel::Stable,
        Platform::MacArm64
    )
    .is_err());
}
#[test]
fn asset_urls_must_match_the_exact_official_tag_and_name() {
    let offer = offer(b"installer");
    for url in [
        "http://github.com/OctoSense-org/OctoSense/releases/download/desktop-v0.1.0-rc.2/OctoSense_0.1.0-rc.2_aarch64.dmg",
        "https://github.com.evil.test/OctoSense-org/OctoSense/releases/download/desktop-v0.1.0-rc.2/OctoSense_0.1.0-rc.2_aarch64.dmg",
        "https://github.com/another/repo/releases/download/desktop-v0.1.0-rc.2/OctoSense_0.1.0-rc.2_aarch64.dmg",
        "https://github.com/OctoSense-org/OctoSense/releases/download/desktop-v0.1.0-rc.1/OctoSense_0.1.0-rc.2_aarch64.dmg",
        "https://github.com/OctoSense-org/OctoSense/releases/download/desktop-v0.1.0-rc.2/OctoSense_0.1.0-rc.2_aarch64.dmg?download=1",
        "file:///tmp/installer.dmg",
    ] {
        let mut bad = offer.asset.clone(); bad.browser_download_url = url.into();
        assert!(validate_asset(offer.tag(), &bad, MAX_INSTALLER).is_err(), "{url}");
    }
}
#[test]
fn redirects_allow_only_github_release_https_hosts() {
    for url in ["https://release-assets.githubusercontent.com/github-production-release-asset/a?token=signed", "https://objects.githubusercontent.com/a", "https://github.com/OctoSense-org/OctoSense/releases/download/desktop-v1.0.0/a"] {
        assert!(valid_redirect(&reqwest::Url::parse(url).unwrap()));
    }
    for url in [
        "http://release-assets.githubusercontent.com/a",
        "https://release-assets.githubusercontent.com:444/a",
        "https://user@release-assets.githubusercontent.com/a",
        "https://api.github.com/a",
        "https://github.com/someone/other/releases/download/a",
        "https://127.0.0.1/a",
        "https://release-assets.githubusercontent.com.evil.test/a",
    ] {
        assert!(!valid_redirect(&reqwest::Url::parse(url).unwrap()), "{url}");
    }
}
#[test]
fn mandatory_github_digest_and_asset_bounds() {
    let o = offer(b"installer");
    for digest in [None, Some("sha512:abc".into()), Some("sha256:xyz".into())] {
        let mut bad = o.asset.clone();
        bad.digest = digest;
        assert!(validate_asset(o.tag(), &bad, MAX_INSTALLER).is_err());
    }
    for size in [0, MAX_INSTALLER + 1] {
        let mut bad = o.asset.clone();
        bad.size = size;
        assert!(validate_asset(o.tag(), &bad, MAX_INSTALLER).is_err());
    }
}
#[test]
fn manifest_must_bind_exact_filename_and_github_digest() {
    let hash = "ab".repeat(32);
    assert!(verify_checksum_manifest(
        format!("{hash}  correct.dmg\n").as_bytes(),
        "correct.dmg",
        &hash
    )
    .is_ok());
    assert!(verify_checksum_manifest(
        format!("{hash} *correct.dmg\r\n").as_bytes(),
        "correct.dmg",
        &hash
    )
    .is_ok());
    for text in [
        format!("{hash}  other.dmg\n"),
        format!("{}  correct.dmg\n", "cd".repeat(32)),
        format!("{hash}  correct.dmg\n{hash}  correct.dmg\n"),
        format!("{hash}  ../correct.dmg\n"),
        format!("{hash}  /correct.dmg\n"),
        format!("{hash}  correct.dmg extra\n"),
        format!("{hash}\t correct.dmg\n"),
        "é".repeat(35),
    ] {
        assert!(
            verify_checksum_manifest(text.as_bytes(), "correct.dmg", &hash).is_err(),
            "{text}"
        );
    }
}
#[test]
fn checksum_manifest_itself_is_verified() {
    let bytes = b"a manifest";
    let hash = format!("{:x}", Sha256::digest(bytes));
    assert!(verify_bytes(bytes, bytes.len() as u64, &hash).is_ok());
    assert!(verify_bytes(b"b manifest", bytes.len() as u64, &hash).is_err());
    assert!(verify_bytes(bytes, 999, &hash).is_err());
}
#[test]
fn completed_download_is_revalidated_before_handoff() {
    let tmp = tempfile::tempdir().unwrap();
    let bytes = b"verified installer";
    let mut pending = PendingDownload::new(&tmp.path().join("updates"), &offer(bytes)).unwrap();
    pending.append(&bytes[..4]).unwrap();
    pending.append(&bytes[4..]).unwrap();
    let artifact = pending.finish().unwrap();
    assert_eq!(fs::read(artifact.revalidate().unwrap()).unwrap(), bytes);
    assert!(artifact
        .path()
        .starts_with(tmp.path().canonicalize().unwrap()));
    assert_eq!(artifact.platform(), Platform::MacArm64);
    assert!(artifact
        .path()
        .parent()
        .unwrap()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("verified-"));
}
#[test]
fn corruption_after_download_blocks_handoff() {
    let tmp = tempfile::tempdir().unwrap();
    let mut pending =
        PendingDownload::new(&tmp.path().join("updates"), &offer(b"installer")).unwrap();
    pending.append(b"installer").unwrap();
    let artifact = pending.finish().unwrap();
    fs::write(artifact.path(), b"corrupted").unwrap();
    assert!(matches!(artifact.revalidate(), Err(Error::Integrity(_))));
}
#[test]
fn incomplete_corrupt_and_oversize_downloads_leave_no_artifact() {
    for (bytes, append_fails) in [
        (b"short".as_slice(), false),
        (b"corrupted".as_slice(), false),
        (b"much longer than installer".as_slice(), true),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("updates");
        let mut pending = PendingDownload::new(&root, &offer(b"installer")).unwrap();
        let result = pending.append(bytes);
        if append_fails {
            assert!(result.is_err());
            drop(pending);
        } else {
            result.unwrap();
            assert!(pending.finish().is_err());
        }
        assert_eq!(fs::read_dir(root).unwrap().count(), 0);
    }
}
#[test]
fn interruption_removes_only_its_partial_file_and_retry_works() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("updates");
    {
        let mut first = PendingDownload::new(&root, &offer(b"installer")).unwrap();
        first.append(b"inst").unwrap();
        fs::write(root.join("unrelated.txt"), b"keep").unwrap();
    }
    let mut second = PendingDownload::new(&root, &offer(b"installer")).unwrap();
    second.append(b"installer").unwrap();
    let artifact = second.finish().unwrap();
    artifact.revalidate().unwrap();
    assert_eq!(fs::read(root.join("unrelated.txt")).unwrap(), b"keep");
    assert_eq!(fs::read_dir(root).unwrap().count(), 2);
}
#[test]
fn busy_guard_is_shared_and_released_after_failure() {
    let busy = Arc::new(AtomicBool::new(false));
    let first = Operation::enter(&busy).unwrap();
    assert!(matches!(Operation::enter(&busy), Err(Error::Busy)));
    drop(first);
    assert!(Operation::enter(&busy).is_ok());
}
#[test]
fn cancellation_is_shared_and_prevents_network_or_disk_work() {
    let tmp = tempfile::tempdir().unwrap();
    let client = Client::new(tmp.path().join("updates")).unwrap();
    let cancel = Cancellation::new();
    cancel.clone().cancel();
    assert!(matches!(
        client.check(
            &BuildIdentity::from_tag(Product::Desktop, None).unwrap(),
            Channel::Preview,
            Platform::MacArm64,
            &cancel
        ),
        Err(Error::Cancelled)
    ));
    assert!(matches!(
        client.download(&offer(b"installer"), &cancel, |_| panic!(
            "should not begin"
        )),
        Err(Error::Cancelled)
    ));
    assert!(!tmp.path().join("updates").exists());
}
#[test]
fn cancellation_interrupts_stalled_network_future() {
    let cancel = Cancellation::new();
    let other = cancel.clone();
    let join = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(40));
        other.cancel();
    });
    let start = Instant::now();
    let result: Result<()> = run(wait_network(
        std::future::pending::<std::result::Result<(), reqwest::Error>>(),
        &cancel,
        Instant::now() + Duration::from_secs(60),
    ));
    join.join().unwrap();
    assert_eq!(result.unwrap_err(), Error::Cancelled);
    assert!(start.elapsed() < Duration::from_secs(2));
}
#[test]
fn total_deadline_bounds_stalled_network() {
    let result: Result<()> = run(wait_network(
        std::future::pending::<std::result::Result<(), reqwest::Error>>(),
        &Cancellation::new(),
        Instant::now() + Duration::from_millis(5),
    ));
    assert_eq!(result.unwrap_err(), Error::Timeout);
}
#[test]
fn a_worker_runtime_is_required_instead_of_nested_runtime_panic() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        assert!(matches!(run(async { Ok(()) }), Err(Error::Storage(_))));
    });
}
#[cfg(unix)]
#[test]
fn cache_and_artifact_symlinks_are_refused() {
    use std::os::unix::fs::symlink;
    let tmp = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let root = tmp.path().join("updates");
    symlink(other.path(), &root).unwrap();
    assert!(PendingDownload::new(&root, &offer(b"installer")).is_err());
    fs::remove_file(&root).unwrap();
    let mut pending = PendingDownload::new(&root, &offer(b"installer")).unwrap();
    pending.append(b"installer").unwrap();
    let artifact = pending.finish().unwrap();
    let outside = other.path().join("installer");
    fs::write(&outside, b"installer").unwrap();
    fs::remove_file(artifact.path()).unwrap();
    symlink(outside, artifact.path()).unwrap();
    assert!(artifact.revalidate().is_err());
}
#[cfg(unix)]
#[test]
fn directories_are_private_and_insecure_existing_cache_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("updates");
    let pending = PendingDownload::new(&root, &offer(b"installer")).unwrap();
    assert_eq!(
        fs::metadata(&root).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(&pending.path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    drop(pending);
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(PendingDownload::new(&root, &offer(b"installer")).is_err());
}
#[test]
fn error_messages_never_echo_network_url_or_file_content() {
    assert!(!Error::Network.to_string().contains("http"));
    assert!(Error::Integrity("download checksum does not match release")
        .to_string()
        .contains("verification failed"));
}

#[test]
fn release_candidate_channel_excludes_beta_alpha_and_mislabelled_prerelease() {
    let mut flagged_stable = release("desktop-v4.0.0", Platform::MacArm64);
    flagged_stable.prerelease = true;
    let selected = select_release(
        vec![
            release("desktop-v8.0.0-nightly.1", Platform::MacArm64),
            release("desktop-v9.0.0-beta.2", Platform::MacArm64),
            release("desktop-v9.0.0-alpha.1", Platform::MacArm64),
            release("desktop-v9.0.0-rc.beta", Platform::MacArm64),
            release("desktop-v1.0.0-rc.2", Platform::MacArm64),
            release("desktop-v1.0.0-rc.10", Platform::MacArm64),
            flagged_stable,
        ],
        Product::Desktop,
        Channel::ReleaseCandidate,
        Platform::MacArm64,
    )
    .unwrap()
    .unwrap();
    assert_eq!(selected.0.tag_name, "desktop-v1.0.0-rc.10");
    let promoted = select_release(
        vec![selected.0, release("desktop-v1.0.0", Platform::MacArm64)],
        Product::Desktop,
        Channel::ReleaseCandidate,
        Platform::MacArm64,
    )
    .unwrap()
    .unwrap();
    assert_eq!(promoted.0.tag_name, "desktop-v1.0.0");
}
#[test]
fn installed_release_selects_its_default_channel_without_upgrading_stable_users_to_rc() {
    for (tag, channel) in [
        (None, Channel::Preview),
        (Some("desktop-v1.0.0"), Channel::Stable),
        (Some("desktop-v1.0.0-rc.2"), Channel::ReleaseCandidate),
        (Some("desktop-v1.0.0-beta.2"), Channel::Preview),
    ] {
        assert_eq!(
            BuildIdentity::from_tag(Product::Desktop, tag)
                .unwrap()
                .default_channel(),
            channel
        );
    }
}

#[test]
fn selecting_an_older_channel_never_offers_an_automatic_downgrade() {
    let installed = BuildIdentity::from_tag(Product::Desktop, Some("desktop-v1.0.0")).unwrap();
    assert_eq!(
        update_status(&installed, &Version::parse("1.0.0-rc.10").unwrap()),
        CheckStatus::Current
    );
    assert_eq!(
        update_status(&installed, &Version::parse("1.0.0+build2").unwrap()),
        CheckStatus::Current
    );
    assert_eq!(
        update_status(&installed, &Version::parse("1.0.1-rc.1").unwrap()),
        CheckStatus::Available
    );
}
#[test]
fn linux_installed_format_is_preserved_and_unknown_layout_is_unsupported() {
    assert_eq!(
        linux_platform(Path::new("/usr/bin/octosense"), None, false),
        Some(Platform::LinuxX64Deb)
    );
    assert_eq!(
        linux_platform(
            Path::new("/mount/image/usr/bin/octosense"),
            Some(Path::new("/mount/image")),
            true
        ),
        Some(Platform::LinuxX64AppImage)
    );
    assert_eq!(
        linux_platform(
            Path::new("/mount/image/usr/bin/octosense"),
            Some(Path::new("/mount/image")),
            false
        ),
        None
    );
    assert_eq!(
        linux_platform(
            Path::new("/mount/other/usr/bin/octosense"),
            Some(Path::new("/mount/image")),
            true
        ),
        None
    );
    assert_eq!(
        linux_platform(Path::new("/home/test/bin/octosense"), None, false),
        None
    );
}

#[test]
fn android_limit_matches_native_installer_before_starting_download() {
    let o = offer(b"installer");
    let mut too_large = o.asset.clone();
    too_large.size = MAX_ANDROID_INSTALLER + 1;
    assert!(validate_asset(o.tag(), &too_large, Platform::AndroidArm64.maximum_size()).is_err());
    assert!(validate_asset(o.tag(), &too_large, Platform::MacArm64.maximum_size()).is_ok());
}
#[test]
fn discarding_unused_artifact_preserves_other_downloads() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("updates");
    let mut first = PendingDownload::new(&root, &offer(b"installer")).unwrap();
    first.append(b"installer").unwrap();
    let first = first.finish().unwrap();
    let first_path = first.path().to_owned();
    let mut second = PendingDownload::new(&root, &offer(b"installer")).unwrap();
    second.append(b"installer").unwrap();
    let second = second.finish().unwrap();
    first.discard().unwrap();
    assert!(!first_path.exists());
    second.revalidate().unwrap();
    assert_eq!(fs::read_dir(root).unwrap().count(), 1);
}
#[test]
fn discarding_never_recursively_deletes_unexpected_files() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("updates");
    let mut pending = PendingDownload::new(&root, &offer(b"installer")).unwrap();
    pending.append(b"installer").unwrap();
    let artifact = pending.finish().unwrap();
    let unexpected = artifact.path().parent().unwrap().join("other.txt");
    fs::write(&unexpected, b"keep").unwrap();
    assert!(artifact.discard().is_err());
    assert_eq!(fs::read(unexpected).unwrap(), b"keep");
}

#[test]
fn finished_worker_does_not_wait_indefinitely_for_os_resolver_tasks() {
    let (unblock, wait) = std::sync::mpsc::channel();
    let (finished, result) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let outcome = run(async {
            tokio::task::spawn_blocking(move || {
                let _ = wait.recv();
            });
            // Ensure the blocking task has been submitted before ending work.
            tokio::task::yield_now().await;
            Ok(())
        });
        finished.send(outcome).unwrap();
    });
    let bounded = result.recv_timeout(Duration::from_secs(2));
    let _ = unblock.send(());
    worker.join().unwrap();
    assert!(bounded.expect("runtime shutdown must be bounded").is_ok());
}
