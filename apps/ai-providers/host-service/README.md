# octosense-llm-service: the `llm` host service

The AI providers app (`os.ai-providers`, [../bundle](../bundle)) edits the
LLM providers the AppCard assistant runs on, through this service. The app
never sees a key, a PIN or a QR: keys are typed on a host-owned sheet, the
phone QR is drawn on a sheet, and a scanned code is decoded here. The method
table is in [src/lib.rs](src/lib.rs).

## Registering it (shells)

Register it once, before the first system app opens, next to Mail:

```rust
// Defaults: the kernel's core dir from octosense_llm_config::profile::
// default_core_dir() ($OCTOS_APP_CORE_DIR, else $HOME/octos-home/.octos),
// the platform's vault, no scanner, no change hook.
octosense_llm_service::register();

// What a shell normally passes:
octosense_llm_service::register_with(
    octosense_llm_service::Options::default()
        .core_dir(core_dir)                       // where profiles/_main.json lives
        .scanner(Arc::new(MyScanner::default()))  // phone only
        .image_picker(Arc::new(MyPicker::default())) // a QR from a picture
        .image_drops(true)                        // desktop: drops go to offer_image
        .on_changed(|| restart_appcard_core()),   // any thread
);
```

- `core_dir`: the embedded kernel's octos home (`<core_dir>/profiles/_main.json`).
- `vault`: overrides where keys go (tests, `OCTOSENSE_LLM_VAULT=file`).
- `scanner`: an `Arc<dyn QrScanner>`. Leave it out on the desktop; the import
  sheet then takes a pasted `OCTOS1E:` code only.
- `image_picker`: an `Arc<dyn QrImagePicker>`: a file dialog on the desktop,
  the photo picker on a phone. The import sheet then offers "Choose image" and
  reads the provider QR out of the picture (a screenshot, a photo of the
  screen). `llm.providers` reports `image_picker: true`.
- `image_drops`: the shell passes image files dropped on the AI providers app
  to `octosense_llm_service::offer_image(bytes)`; the import sheet then says
  a screenshot can be dropped on it.
- `on_changed`: called after the saved provider set changed (save, reorder,
  removal, import). Restart the AppCard kernel from it (it may run on a worker
  thread: post to the UI thread).

The bundle's manifest asks for `llm`; the shell packs `apps/ai-providers/bundle`
like any system app (`system-apps.json`). App Hub's admission knows only the
capabilities in `octosense_app_policy::KNOWN_CAPABILITIES` and refuses any
other, so `llm` has to be added there (beside `mail`, OctoSense-App-Hub
`crates/app-policy/src/manifest.rs`) before the app opens in a shell. The
service serves `os.` apps only.

### A scanner over Makepad

The service does not link Makepad. On Android a shell implements
`QrScanner` over Makepad's `cx.show_qr_scanner()` (OctoSense-org/makepad
`feat/qr-scanner-api`), which answers with exactly one `NativeQrScanned { json }`
or `NativeQrCancelled { reason }` action:

```rust
#[derive(Default)]
struct ShellScanner { pending: Mutex<Option<octosense_llm_service::ScanDone>> }

impl octosense_llm_service::QrScanner for ShellScanner {
    fn scan(&self, done: octosense_llm_service::ScanDone) {
        if let Some(earlier) = self.pending.lock().unwrap().replace(done) {
            earlier(Err("replaced".into()));
        }
        SignalToUI::set_ui_signal(); // the shell calls cx.show_qr_scanner() on its next event
    }
}

// In the shell's event handling, on the UI thread:
if scanner.wants_open() { cx.show_qr_scanner(); }
for action in actions {
    if let Some(s) = action.downcast_ref::<NativeQrScanned>() { scanner.finish(Ok(s.json.clone())) }
    if let Some(c) = action.downcast_ref::<NativeQrCancelled>() { scanner.finish(Err(c.reason.clone())) }
}
```

### An image picker

```rust
pub enum PickError { Cancelled, Failed(String) }
pub type ImageDone = Box<dyn FnOnce(Result<Vec<u8>, PickError>) + Send>;

pub trait QrImagePicker: Send + Sync {
    /// Let the person choose one image and call `done` exactly once.
    fn pick(&self, done: ImageDone);
}
```

`pick` is called on the UI thread when the person taps "Choose image" on the
import sheet; `done` may be called from any thread, exactly once. Hand over
the file's encoded bytes as stored (PNG or JPEG; do not decode them): the
service decodes, finds the QR (it tries the image as is, thresholded, scaled
down and scaled up, so a phone screenshot with a small code in it works),
refuses anything over 20 MB or 40 megapixels, and then asks for the PIN on
the same sheet, exactly as after a camera scan. `Err(PickError::Cancelled)`
leaves the sheet as it was; `Err(PickError::Failed(why))` shows `why`.

A shell whose picker is modal and synchronous can call `done` inside `pick`;
one driven by Makepad events keeps `done` until its result action arrives, as
the scanner above does. On the desktop that is Makepad's native open panel:

```rust
// pick(): keep `done`, then on the UI thread:
cx.open_select_file_dialog(FileDialog::new().set_id(id).add_filter("Images".into(), vec!["png".into(), "jpg".into(), "jpeg".into()]));
// FileDialogAction::FileSelected { id, paths } -> done(Ok(std::fs::read(&paths[0])?))  (on a worker)
// FileDialogAction::FileCancelled { id }        -> done(Err(PickError::Cancelled))
```

### Dropped images

Sheets (Splash isolates) receive no file drops, so a shell that can take one
answers it itself: while `octosense_llm_service::wants_image()` is true (an
import sheet is up and waiting), a drag of an image file over the AI
providers app gets `DragResponse::Copy`, and the drop's bytes go to
`octosense_llm_service::offer_image(bytes)`, which reads the code on a worker
and hands it to the sheet (it then asks for the PIN). Register with
`.image_drops(true)` so the sheet waits for one.

### Reading the image

`image_qr::find_code(bytes)` (PNG or JPEG) tries the picture as it is and
Otsu-thresholded, scaled down (long side 1600, 1000, 700) and scaled up
smoothly (1.5 to 3 times when small), and takes the first code rqrr reads
that is an OctoSense provider code. Measured: QR-A (69 modules with its quiet
zone) reads from about 2 pixels a module in a 1080×2280 screenshot (about
140 px wide) and from 90 px as a crop; JPEG at quality 40 reads; a 12 MP
photo reads through a scaled copy. Errors are "No QR code found in that
image.", "…not an OctoSense provider code.", "That image is too large (at
most 20 MB and 40 megapixels)." (checked from the header, before decoding)
and "That file is not a PNG or JPEG image.".

## Where keys go

octos reads a key from `config.env_vars.<ENV>` in the profile; a `keychain:`
value there means "read it from octos's secret store" (octos-cli
`auth/keychain.rs`). So ([src/vault.rs](src/vault.rs)):

| platform | key goes to | profile value |
| --- | --- | --- |
| macOS | login keychain, service `octos`, account `<ENV>`, via the `security` tool | `keychain:` |
| Linux | `<core_dir>/secrets/<ENV>` (0600; the kernel's octos home is the core dir) | `keychain:` |
| Android, iOS, others | the profile itself (app-private, 0600) | the key |
| `OCTOSENSE_LLM_VAULT=file` | the profile itself (development: no keychain) | the key |

octos has no secret store on Android and reads raw `env_vars` there, so a key
sealed with an Android Keystore key (as Mail's passwords are) would be
unreadable to the kernel. A key the keychain refuses also stays in the profile.

## Tests

From `apps/ai-providers`:

```sh
cargo test --workspace
```

The workspace resolves Makepad, Octoscript-Makepad and Octoscript from
checkouts beside this repository (`../makepad` and so on, as the shells do),
and the makepad one must carry the contained-app runtime the shells build with
(splash host requests and sheets). To use another checkout without editing
the manifest, pass a `--config` file with `[patch."https://github.com/OctoSense-org/makepad.git"]`
entries pointing at it. The macOS keychain test runs only when asked:
`cargo test -p octosense-llm-service -- --ignored keychain`.
