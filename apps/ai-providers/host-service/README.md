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
        .on_changed(|| restart_appcard_core()),   // any thread
);
```

- `core_dir`: the embedded kernel's octos home (`<core_dir>/profiles/_main.json`).
- `vault`: overrides where keys go (tests, `OCTOSENSE_LLM_VAULT=file`).
- `scanner`: an `Arc<dyn QrScanner>`. Leave it out on the desktop; the import
  sheet then takes a pasted `OCTOS1E:` code only.
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
