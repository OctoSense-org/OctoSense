# Generic app-tool acceptance runner

English | [简体中文](README.zh-CN.md)

This development-only native example invokes the real `script-tools-v1` ABI
of an explicitly supplied local fixture. It addresses a testing gap: stock
`card-host` refuses this ABI, while `host-api-lab` tests one fixed app/tool.
It does not load a model, grant an app agent consent, or use a normal profile.

Build from a prepared OctoSense checkout:

```sh
python3 tools/setup.py
cargo build --locked --offline --release -p octosense-shell \
  --example app-tool-acceptance --features acceptance-fixtures -j2
```

Use the companion App Flow `examples/script-tool-state/verify-native.py`
driver to create an isolated profile, control a hidden native window and
record evidence. It requires this binary, a built `hub`, and a new output
directory. The [migration guide](https://github.com/OctoSense-org/OctoScript-App-Design-Flow/blob/main/docs/SUBMISSION-API-MIGRATIONS.md)
and fixture ship in the companion change; they are not part of an older App
Flow checkout.

The runner accepts explicit arguments:

| Argument | Meaning |
| --- | --- |
| `--bundle=<dir>` | Stamped editable fixture; original source is unchanged |
| `--app-data=<dir>` | New empty profile, or this fixture's marked profile for a restart test |
| `--receipt=<file>` | New JSON result path |
| `--trigger-file=<file>` | New path the driver creates only after observing loaded UI |
| `--tool=<name>` | Declared app-implemented tool to invoke |
| `--args=<JSON>` | Valid input for that tool |
| `--invalid-args=<JSON>` | Input its schema must reject |
| `--expected=<JSON>` | Exact expected tool result |
| `--preview` | Render only to capture listing artwork; no tool is dispatched and no gate acceptance is claimed |

The normal path copies the bundle, signs its manifest with an ephemeral key,
runs the actual gate with signature required, checks runtime requirements,
applies the admitted isolate policy, and binds its real Splash `app_tool`
handler. It checks wrong-account, undeclared-tool and invalid-input refusals,
then submits the valid call. Once its callback returns, it compares the exact
result, unregisters the tool owner and verifies `app_not_running`. It leaves
the UI alive for the instrument's screenshot and local editing checks.

The driver restarts the same marked profile and calls a read tool, proving
that native editing and the tool use the same persisted value. No keys leave
memory, and the signature creates no public publisher identity. This is signed
admission, **not** public-catalog installation or GitHub publisher verification.
The test caller enters after the production consent/peer relay, so it proves
neither human agent consent nor model reasoning.

Set `MAKEPAD_HIDE_WINDOWS=1` and a unique `MAKEPAD_REMOTE` port. The driver
isolates `RINX_DATA_DIR`, `OCTOSENSE_HOME` and `OCTOS_APP_CORE_DIR`, disables
system-font fallback, retains failure receipts/logs, captures real native
pixels, and terminates only the process it owns. Review screenshots as well as
JSON; widget text alone can miss a repaint or clipping failure.

**Validated on macOS:** the release build passed; Script Tool State passed
signed admission, live tool/UI mutation, manual Unicode editing, exact restart
persistence and refusal checks. The API Migration Lab passed its native edit,
restart and missing-service paths. All seven native Metal captures were
inspected. Two private mutations were correctly rejected: the old callback
name and a missing runtime marker. See [validation.json](validation.json) for
binary/source identity, exact results and scope. Phone, provider, OS approval,
agent consent, public installation and external actions are not covered.
