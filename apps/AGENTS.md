# Working in OctoSense System Apps

These are shipping apps. Keep changes small, test them in a shell, and keep the
rules in README.md.

- An app is `apps/<name>/bundle/`: `manifest.json` + `main.splash` (+ artwork).
  Learn the language, the APIs and the development loop from
  [OctoScript App Design Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow)
  (`docs/QUICKSTART.md`, `docs/SCRIPT-API.md`). Do not invent APIs: if a
  widget or call is not documented there or used by another app here, check the
  runtime source before using it.
- Run a bundle on a desktop with App Hub's `card-host --bundle apps/<name>/bundle
  --system --allow-unsigned` (add `MAKEPAD_REMOTE=<port>` to drive it over HTTP).
  `--system` lets an `os.*` id and an empty digest through, as the shell does.
- Validate on a phone through the ROM's Home as a separate test package; never
  replace the device's installed Home.
- Mail's service: change `apps/mail/host-service` and run
  `cargo test -p octosense-mail-service` from the ROM's `home/`.
- After a change, bump the shells' pin (`home/native-apps.lock.json` in the ROM)
  in a pull request there.
- Never add a password or one-time-code field to an app; secrets belong to a
  host service's sheet.
