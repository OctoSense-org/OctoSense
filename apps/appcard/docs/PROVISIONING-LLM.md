# Provisioning an LLM (bring your own key)

The app needs an LLM to generate cards, but **keys are never in the repo, typed,
or sent over the network.** A user provisions their own provider + key, which is
written into the on-device octos profile config and read by the embedded kernel.

## Where it ends up

`octos-home/.octos/profiles/_main.json`:
```jsonc
{ "config": {
    "llm":      { "primary": { "family_id": "zai", "model_id": "glm-5.2" }, "fallbacks": [] },
    "env_vars": { "ZAI_API_KEY": "sk-…" }          // key env is provider-specific
} }
```
`family_id` selects the provider from octos's registry (`zai`, `deepseek`,
`openai`, `anthropic`, …); each reads its key from the env var the registry
names (`octosense_llm_config::registry`).

## The QR flow

1. **Encode** a provider set on a trusted machine:
   ```bash
   cargo run --manifest-path tools/llm-qr/Cargo.toml -- \
       --family deepseek --model deepseek-chat --prompt-key \
       --fallback zai:glm-4.6 --secret ZAI_API_KEY=… --out qr.png
   # prints the OCTOS1E:… payload and a PIN such as 7K3M-9QX2
   ```
   The AI providers app (`apps/ai-providers`, "Show QR for phone") draws the
   same code, with its PIN, from the providers saved on a desktop.
   The default payload is octos's own PIN-wrapped profile QR (`OCTOS1E:`,
   Argon2id + ChaCha20-Poly1305 — see `apps/ai-providers/config` in this repository): a
   primary provider, fallbacks, and their keys by env var name. The PIN is
   told separately, never printed beside the code. `--plain` gives `OCTOS1:`
   (refused with keys unless `--allow-plain-secrets`); `--legacy` gives the
   old single-provider JSON:
   ```json
   {"llm_family":"zai","llm_model":"glm-5.2","llm_key":"sk-XXXX"}
   ```
2. **Scan** it on the device → the app decodes it (asking for the PIN for
   `OCTOS1E:`), writes the providers into `config.llm` and each key into
   `config.env_vars.<KEY_ENV>` (the env var octos's registry reads for that
   family, e.g. `ZHIPU_API_KEY` for `glm`, `VERTEX_SA_JSON` for `vertex`), and
   the next turn uses it. A legacy JSON code replaces only the primary.

> The QR carries secrets — treat it like a password (don't paste it into chats,
> don't commit the PNG). The keys stay on the device once scanned. A code that
> carries server configuration (endpoint / auth token) is refused.

## Provisioning without the camera (dev / headless)

The same JSON payload can be applied via the launch intent (no scanning), which is
how the flow is tested:
```bash
adb shell am start -S -n dev.makepad.octos_app/.MakepadApp \
    --es makepad.PROVISION_CONFIG '{"llm_family":"zai","llm_model":"glm-5.2","llm_key":"sk-XXXX"}'
```
Server auth (`base_url|profile|token`) still has its own `makepad.APP_CONFIG`
entry point and is never accepted from an LLM QR — see
[BUILDING-ANDROID.md](BUILDING-ANDROID.md).
