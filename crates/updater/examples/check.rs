//! Public release acceptance without installing anything.
use octosense_updater::{BuildIdentity, Cancellation, Channel, Client, Platform, Product};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut product = Product::Desktop;
    let mut platform = None;
    let mut tag = None;
    let mut channel = Channel::Preview;
    let mut download = false;
    let mut args = std::env::args().skip(1);
    let mut cache = std::env::temp_dir().join("octosense-updater-acceptance");
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--product" => {
                product = match args.next().as_deref() {
                    Some("home") => Product::Home,
                    Some("desktop") => Product::Desktop,
                    _ => return Err("expected home or desktop".into()),
                }
            }
            "--platform" => {
                platform = Some(match args.next().as_deref() {
                    Some("mac-arm64") => Platform::MacArm64,
                    Some("windows-x64") => Platform::WindowsX64,
                    Some("linux-appimage") => Platform::LinuxX64AppImage,
                    Some("linux-deb") => Platform::LinuxX64Deb,
                    Some("android-arm64") => Platform::AndroidArm64,
                    _ => return Err("unsupported platform".into()),
                })
            }
            "--installed" => tag = Some(args.next().ok_or("missing tag")?),
            "--stable" => channel = Channel::Stable,
            "--rc" => channel = Channel::ReleaseCandidate,
            "--download" => download = true,
            "--cache" => cache = args.next().ok_or("missing cache path")?.into(),
            "--help" => {
                println!("check [--product desktop|home] [--platform mac-arm64|windows-x64|linux-appimage|linux-deb|android-arm64] [--installed TAG] [--stable|--rc] [--download] [--cache DIR]\nDownloads only with --download; never launches an installer.");
                return Ok(());
            }
            _ => return Err("unknown argument; use --help".into()),
        }
    }
    let platform = platform
        .or_else(|| Platform::current(product))
        .ok_or("specify --platform for this product")?;
    let client = Client::new(cache)?;
    let cancel = Cancellation::new();
    let result = client.check(
        &BuildIdentity::from_tag(product, tag.as_deref())?,
        channel,
        platform,
        &cancel,
    )?;
    let mut receipt = serde_json::json!({"status":format!("{:?}", result.status),"product":format!("{product:?}"),"platform":format!("{platform:?}"),"installer_opened":false});
    if let Some(offer) = result.offer {
        receipt["tag"] = offer.tag().into();
        receipt["asset"] = offer.asset_name().into();
        receipt["size"] = offer.size().into();
        receipt["sha256"] = offer.sha256().into();
        if download {
            let mut last = 0;
            let artifact = client.download(&offer, &cancel, |p| {
                let pct = p.received * 100 / p.total;
                if pct / 10 > last / 10 {
                    eprintln!("Download {pct}%");
                    last = pct;
                }
            })?;
            artifact.revalidate()?;
            receipt["download_verified"] = true.into();
        }
    }
    println!("{}", serde_json::to_string_pretty(&receipt)?);
    Ok(())
}
