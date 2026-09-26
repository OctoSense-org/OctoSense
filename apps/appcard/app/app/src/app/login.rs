//! W08 — LoginScreen.
//!
//! A centered, vertically stacked card with three progressively-revealed
//! sections:
//!
//!   1. **Server discovery** — `Server URL` + `Profile ID` text inputs and a
//!      `Continue` button. Hidden once a server URL is configured (loaded
//!      from `~/.config/octos-app/server.json` on boot, written by
//!      `Continue`).
//!   2. **Email** — `Email` input + `Send code` button. Drives
//!      `POST /api/auth/send-code` (octos-cli `auth_handlers.rs:389`).
//!   3. **Verification code** — `Verification code` input + `Verify` button.
//!      Drives `POST /api/auth/verify` (octos-cli `auth_handlers.rs:543`).
//!
//! On a successful verify the bearer token is written to the OS keychain
//! (`octos_app_store::keychain::store_token`) and the App swaps the active
//! page in the parent `PageFlip` from `login_page` to `home_page`.
//!
//! No custom Rust `Widget` impl: the screen is a plain `View` tree and the
//! state machine lives on `App` (`main.rs`). The `script_mod!` block here
//! only registers the DSL prototype, which `App::script_mod` aggregates into
//! the live tree so `body +: { LoginScreen { … } }` parses.
//!
//! See `workstreams/W08-auth-tenancy.md` § "LoginScreen flow" and
//! `04-IA-AND-NAVIGATION.md` § "LoginScreen" for the design.

use makepad_widgets::*;
use crate::fpath;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    // Field row (TextInput) used inside the login card. Lifts the styling
    // from the chat composer's `input` (main.rs:1038–1067) so colors and
    // glyph fallbacks stay consistent — but trimmed to a single row, smaller
    // height, and a visible glassy chrome (the composer input is
    // chrome-less because the surrounding GlassPanel carries the border).
    let LoginField = TextInput {
        width: Fill
        height: 38
        empty_text: ""
        draw_bg +: {
            color: #x06241DCC
            color_hover: #x0A2D24DD
            color_focus: #x0F362DEE
            color_empty: #x06241DCC
            border_color: #x72E4FF44
            border_color_hover: #x72E4FF66
            border_color_focus: #x72E4FF99
            border_color_empty: #x72E4FF44
            border_size: 1.0
            border_radius: 10.0
        }
        draw_text +: {
            color: #xF3E3C7
            color_empty: #xF3E3C766
            text_style: theme.font_regular {
                line_spacing: theme.font_wdgt_line_spacing
                font_size: 13
                font_family: FontFamily {
                    latin := FontMember{res: file_resource(#(fpath("sans_latin"))) asc: 0.0 desc: 0.0}
                    chinese := FontMember{res: file_resource(#(fpath("cjk"))) asc: 0.0 desc: 0.0}
                    symbols := FontMember{res: file_resource(#(fpath("sans_latin"))) asc: 0.0 desc: 0.0}
                    emoji := FontMember{res: file_resource(#(fpath("emoji"))) asc: 0.0 desc: 0.0}
                }
            }
        }
    }

    let LoginFieldLabel = Label {
        width: Fill
        height: Fit
        margin: Inset{left: 2 bottom: 2 top: 6}
        draw_text.color: #xCDBF9FCC
        draw_text.text_style.font_size: 11
    }

    // Pill-style action button used by the three login steps. Re-shapes the
    // existing `PillButton` (main.rs:168–182) for full-width inside the
    // card and a slightly louder accent so it reads as the primary action.
    let LoginActionButton = ButtonFlat {
        width: Fill
        height: 40
        align: Align{x: 0.5 y: 0.5}
        padding: Inset{left: 14 right: 14 top: 0 bottom: 0}
        margin: Inset{top: 8}
        draw_text +: {
            color: #x06130F
            text_style +: { font_size: 12 }
        }
        draw_bg +: {
            color: #xF6BE63
            color_hover: #xFFD18A
            border_color: #xFFF0D2AA
            border_size: 1.0
            border_radius: 10.0
        }
    }

    // The LoginScreen is registered as a regular widget prototype so the
    // main DSL can place it as `LoginScreen { … }` inside the parent
    // PageFlip. It is a plain View tree — the state machine lives in
    // `App::handle_actions`, which toggles the `visible` flag on each step
    // container.
    mod.widgets.LoginScreen = View {
        width: Fill
        height: Fill
        flow: Down
        align: Align{x: 0.5 y: 0.5}
        show_bg: true
        // Faint matching backdrop (slightly darker than `app_shell`) so the
        // card pops without a hard contrast jump if we re-enter Login from
        // Home (`Logout` keeps the window content visible underneath).
        draw_bg +: {
            color: #x07181599
        }

        login_card := GlassPanel {
            width: Fill{min: 360 max: 420}
            height: Fit
            new_batch: true
            flow: Down
            padding: Inset{left: 30 top: 28 right: 30 bottom: 26}
            spacing: 8
            draw_bg +: {
                tint_color: #x0B3B31
                tint_alpha: 0.92
                border_color: #x72E4FF
                border_alpha: 0.42
                border_width: 1.0
                corner_radius: 22.0
                halo_color: #x72E4FF
                halo_strength: 0.18
                halo_radius: 9.0
                highlight_strength: 0.32
                highlight_band_height: 60.0
                chroma_strength: 0.0
                noise_strength: 0.005
            }

            login_title := Label {
                width: Fill
                height: Fit
                margin: Inset{bottom: 4}
                align: Align{x: 0.5}
                draw_text.color: #xF3E3C7
                draw_text.text_style.font_size: 22
                draw_text.text_style.line_spacing: 1.2
                text: "Octos"
            }

            login_subtitle := Label {
                width: Fill
                height: Fit
                margin: Inset{bottom: 12}
                align: Align{x: 0.5}
                draw_text.color: #xCDBF9FAA
                draw_text.text_style.font_size: 12
                text: "Sign in to your Octos server"
            }

            // Step 1 — Server URL + Profile ID. Hidden once
            // `~/.config/octos-app/server.json` exists.
            login_server_step := View {
                width: Fill
                height: Fit
                flow: Down

                LoginFieldLabel { text: "Server URL" }
                login_server_url_input := LoginField {
                    empty_text: "https://octos.example.com"
                }

                LoginFieldLabel { text: "Profile ID" }
                login_profile_id_input := LoginField {
                    empty_text: "acme"
                }

                login_continue_button := LoginActionButton {
                    text: "Continue"
                }

                Label {
                    width: Fill
                    height: Fit
                    margin: Inset{top: 8}
                    align: Align{x: 0.5}
                    draw_text.color: #xCDBF9F77
                    draw_text.text_style.font_size: 10
                    text: "Tip: if you sign in at acme.octos.ominix.io your Profile ID is `acme`."
                }
            }

            // Step 2 — Email + Send code. Always visible after server step.
            login_email_step := View {
                width: Fill
                height: Fit
                flow: Down

                LoginFieldLabel { text: "Email" }
                login_email_input := LoginField {
                    empty_text: "you@example.com"
                }

                login_send_code_button := LoginActionButton {
                    text: "Send code"
                }
            }

            // Step 3 — Verification code + Verify. Hidden until `Send code`
            // succeeds (server-side advance is unconditional per
            // `auth_handlers.rs:389`, so this flips on as long as the network
            // call returned without a transport error).
            login_code_step := View {
                width: Fill
                height: Fit
                flow: Down
                visible: false

                LoginFieldLabel { text: "Verification code" }
                login_code_input := LoginField {
                    empty_text: "123456"
                }

                login_verify_button := LoginActionButton {
                    text: "Verify"
                }
            }

            // Status / error label. Updated from `App::handle_actions`.
            login_status_label := Label {
                width: Fill
                height: Fit
                margin: Inset{top: 12}
                align: Align{x: 0.5}
                draw_text.color: #xF6BE63
                draw_text.text_style.font_size: 11
                text: ""
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Server-config persistence (`~/.config/octos-app/server.json`).
// ---------------------------------------------------------------------------
//
// Substitute for octos-web's subdomain inference (W08 design § Profile
// resolution mode 2). On a packaged native app there's no host to parse, so
// the first-run dialog asks for both server URL and profile id explicitly,
// and we drop a tiny JSON file alongside other user config so a second
// launch boots straight to the email step.

/// Returns `<config_dir>/octos-app/server.json`. On macOS this resolves to
/// `~/Library/Application Support/octos-app/server.json`; on Linux to
/// `~/.config/octos-app/server.json`. Falls back to `~/.octos-app/` if
/// `dirs`-style discovery fails.
/// `~/.config/octos-app/` — where this app's own files live.
///
/// Shared so the durable user store (§5.12) lands beside `server.json` rather
/// than inventing a second location. Two config directories is how one of them
/// ends up unbacked-up and the other undiscoverable.
pub fn config_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("OCTOS_APP_CONFIG_DIR").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    let home = std::env::var_os("HOME")?;
    let mut p = PathBuf::from(home);
    // Match the W08 spec wording (`~/.config/octos-app/server.json`)
    // verbatim — XDG dirs aren't worth a new dep here, and macOS users
    // already use this layout for many CLI tools.
    p.push(".config");
    p.push("octos-app");
    Some(p)
}

fn server_config_path() -> Option<PathBuf> {
    Some(config_dir()?.join("server.json"))
}

/// On-disk shape of the server config file. Kept in this module so we don't
/// thread a dedicated crate boundary just for two strings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub server_url: String,
    pub profile_id: String,
}

/// Read `~/.config/octos-app/server.json` if it exists. Returns `None` for
/// "no config yet" (which is the new-user path) and logs+returns `None` on
/// parse errors so a corrupt file doesn't brick the app.
pub fn load_server_config() -> Option<ServerConfig> {
    let path = server_config_path()?;
    let bytes = std::fs::read(&path).ok()?;
    match serde_json::from_slice::<ServerConfig>(&bytes) {
        Ok(cfg) => Some(cfg),
        Err(e) => {
            log::warn!("failed to parse {}: {e}", path.display());
            None
        }
    }
}

/// Write `~/.config/octos-app/server.json`. Creates the parent directory if
/// needed. Returns the path written so callers can surface it on error.
pub fn save_server_config(cfg: &ServerConfig) -> std::io::Result<PathBuf> {
    let path = server_config_path()
        .ok_or_else(|| std::io::Error::other("HOME is unset; cannot write server config"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_vec_pretty(cfg)
        .map_err(|e| std::io::Error::other(format!("serialize server config: {e}")))?;
    std::fs::write(&path, body)?;
    Ok(path)
}

/// Non-UI provisioning entry: parse `base_url|profile_id|token` (token
/// optional) and persist the server config + bearer in one shot. Today this
/// is fed only by the `makepad.APP_CONFIG` launch-intent extra on Android.
pub fn apply_provision_string(prov: &str) -> Result<(), String> {
    let mut parts = prov.trim().splitn(3, '|');
    let url_str = parts.next().unwrap_or("");
    let profile = parts.next().unwrap_or("").trim();
    let token = parts.next().unwrap_or("").trim();
    let url = validate_server_url(url_str)?;
    if profile.is_empty() {
        return Err("provision: profile id missing (want base_url|profile|token)".into());
    }
    save_server_config(&ServerConfig {
        server_url: url.to_string(),
        profile_id: profile.to_string(),
    })
    .map_err(|e| format!("provision: save config: {e}"))?;
    if !token.is_empty() {
        let host = octos_app_store::auth::ServerHost::from(host_from_url(&url));
        let pid = octos_app_store::auth::ProfileId::from(profile.to_string());
        let secret = octos_app_store::auth::SecretToken::from(token.to_string());
        octos_app_store::keychain::store_token(&host, &pid, &secret)
            .map_err(|e| format!("provision: store token: {e}"))?;
    }
    log::info!("provisioned profile `{profile}` @ {url}");
    Ok(())
}

/// Apply an LLM-only QR / intent payload. Accepted payloads (all parsed by
/// `octosense_llm_config::qr`):
///
/// - the legacy self-contained JSON object
///   `{"llm_family":..,"llm_model":..,"llm_key":..,"llm_base_url":..,"llm_api_type":..}`
///   — replaces `config.llm.primary` only, fallbacks are kept;
/// - octos's `OCTOS1:` profile QR — replaces the whole provider set
///   (primary + fallbacks) and upserts every key it carries;
/// - `OCTOS1E:` (PIN-wrapped) — refused here with a "needs PIN" error; the
///   caller collects the PIN and uses [`apply_provision_payload`].
///
/// Keys land in `_main.json` → `config.env_vars.<KEY_ENV>` under the name
/// octos's registry reads for the family. Server connection/auth settings are
/// deliberately handled only by `makepad.APP_CONFIG` (a code carrying them is
/// refused).
pub fn apply_provision_config_json(payload: &str) -> Result<String, String> {
    apply_provision_payload(payload, None)
}

/// [`apply_provision_config_json`] with the PIN for an `OCTOS1E:` code.
///
/// With a PIN this runs Argon2id over 64 MiB: call it off the UI thread.
pub fn apply_provision_payload(payload: &str, pin: Option<&str>) -> Result<String, String> {
    let path = octos_profile_config_path()?;
    apply_provision_payload_at_path(&path, payload, pin)
}

/// Path-injected implementation so persistence can be tested without touching
/// the real desktop or Android profile.
fn apply_provision_payload_at_path(path: &Path, payload: &str, pin: Option<&str>) -> Result<String, String> {
    use octosense_llm_config::qr;
    let format = qr::format_of(payload);
    if format == Some(qr::Format::Encrypted) && pin.is_none_or(|p| p.trim().is_empty()) {
        return Err("provision: this QR is PIN-protected — enter the PIN shown beside it".into());
    }
    let provisioning = qr::decode(payload, pin).map_err(|e| format!("provision: {e}"))?;
    if format == Some(qr::Format::LegacyJson) {
        let primary = provisioning
            .set
            .primary
            .as_ref()
            .ok_or_else(|| "provision: no provider in LLM config".to_string())?;
        octosense_llm_config::profile::save_merge_primary(path, primary, &provisioning.secrets)
            .map_err(|e| format!("provision: write {}: {e}", path.display()))?;
        log::info!("provisioned LLM {} (key {})", primary.label(), primary.key_env);
        return Ok(format!("llm={}", primary.family));
    }
    let summary = octosense_llm_config::apply_to_profile(path, &provisioning)
        .map_err(|e| format!("provision: write {}: {e}", path.display()))?;
    log::info!("provisioned LLM set: {summary}");
    let primary = provisioning.set.primary.as_ref().map(|p| p.label()).unwrap_or_default();
    Ok(match provisioning.set.fallbacks.len() {
        0 => format!("llm={primary}"),
        n => format!("llm={primary} (+{n} fallback{})", if n == 1 { "" } else { "s" }),
    })
}

/// The embedded kernel's `_main.json` profile config (same HOME the kernel is
/// spawned with: `$HOME/octos-home/.octos/profiles/_main.json`).
fn octos_profile_config_path() -> Result<PathBuf, String> {
    octosense_llm_config::profile::default_core_dir()
        .map(|dir| octosense_llm_config::profile::profile_path(&dir))
        .ok_or_else(|| "no HOME set".to_string())
}

// Test adapters: the provisioning tests below predate the shared crate and
// drive it through the old legacy-JSON entry points.

#[cfg(test)]
#[derive(Debug)]
struct LlmProvisionConfig {
    llm_family: String,
    llm_model: Option<String>,
    llm_key: String,
    llm_base_url: Option<String>,
    llm_api_type: Option<String>,
}

#[cfg(test)]
fn parse_llm_provision_config(payload: &str) -> Result<LlmProvisionConfig, String> {
    use octosense_llm_config::qr;
    if qr::format_of(payload) != Some(qr::Format::LegacyJson) {
        return Err("provision: not an LLM config JSON object".into());
    }
    let p = qr::decode(payload, None).map_err(|e| format!("provision: {e}"))?;
    let primary = p.set.primary.expect("legacy JSON always has a primary");
    Ok(LlmProvisionConfig {
        llm_key: p.secrets.get(&primary.key_env).cloned().unwrap_or_default(),
        llm_family: primary.family,
        llm_model: primary.model,
        llm_base_url: primary.base_url,
        llm_api_type: primary.api_type.map(|t| t.as_str().to_string()),
    })
}

/// The octos provider `family_id` → the env var octos reads its key from
/// (`octosense_llm_config::registry`, mirrored from octos-llm's registry).
#[cfg(test)]
fn key_env_for(family: &str) -> String {
    octosense_llm_config::registry::key_env_for(family)
}

#[cfg(test)]
fn apply_llm_config_at_path(
    path: &Path,
    family: &str,
    model: Option<&str>,
    key: Option<&str>,
    base_url: Option<&str>,
    api_type: Option<&str>,
) -> Result<(), String> {
    let mut primary = octosense_llm_config::Provider::new(family, model.map(str::to_string));
    primary.base_url = base_url.map(|u| u.trim().to_string());
    primary.api_type = api_type.and_then(octosense_llm_config::ApiType::parse);
    let env = key
        .map(|k| std::collections::BTreeMap::from([(primary.key_env.clone(), k.to_string())]))
        .unwrap_or_default();
    octosense_llm_config::profile::save_merge_primary(path, &primary, &env).map_err(|e| e.to_string())
}

/// Cheap URL validation for the Step 1 input. Accepts `http://` and
/// `https://`; surfaces a one-line error suitable for the status label.
pub fn validate_server_url(s: &str) -> Result<url::Url, String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Err("Server URL is required".to_string());
    }
    let parsed = url::Url::parse(trimmed)
        .map_err(|e| format!("Invalid server URL: {e}"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        other => return Err(format!("Server URL scheme must be http or https (got `{other}`)")),
    }
    if parsed.host_str().is_none() {
        return Err("Server URL must include a host".to_string());
    }
    Ok(parsed)
}

/// Extract the host portion (no port, no scheme) from a URL — the keychain
/// service name uses this as the first segment so multi-server M2 work can
/// enumerate by prefix.
pub fn host_from_url(u: &url::Url) -> String {
    u.host_str().unwrap_or("unknown-host").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_server_url_accepts_https() {
        let u = validate_server_url("https://octos.example.com").unwrap();
        assert_eq!(u.scheme(), "https");
        assert_eq!(host_from_url(&u), "octos.example.com");
    }

    #[test]
    fn validate_server_url_rejects_empty() {
        assert!(validate_server_url("").is_err());
        assert!(validate_server_url("   ").is_err());
    }

    #[test]
    fn validate_server_url_rejects_unknown_scheme() {
        let e = validate_server_url("ftp://octos.example.com").unwrap_err();
        assert!(e.contains("scheme"));
    }

    #[test]
    fn validate_server_url_rejects_garbage() {
        assert!(validate_server_url("not a url").is_err());
    }

    #[test]
    fn server_config_round_trips_via_serde_json() {
        let cfg = ServerConfig {
            server_url: "https://octos.example.com".to_string(),
            profile_id: "acme".to_string(),
        };
        let bytes = serde_json::to_vec(&cfg).unwrap();
        let back: ServerConfig = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back.server_url, cfg.server_url);
        assert_eq!(back.profile_id, cfg.profile_id);
    }

    #[test]
    fn llm_qr_payload_rejects_server_configuration() {
        let result = parse_llm_provision_config(
            r#"{"llm_family":"zai","llm_key":"sk-test","base_url":"https://example.com"}"#,
        );
        assert!(result.unwrap_err().contains("unknown field `base_url`"));
    }

    #[test]
    fn llm_qr_payload_accepts_only_llm_configuration() {
        let config = parse_llm_provision_config(
            r#"{"llm_family":"zai","llm_model":"glm-5.2","llm_key":"sk-test"}"#,
        )
        .unwrap();
        assert_eq!(config.llm_family, "zai");
        assert_eq!(config.llm_model.as_deref(), Some("glm-5.2"));
        assert_eq!(config.llm_key, "sk-test");
    }

    #[test]
    fn llm_qr_custom_endpoint_preserves_profile_and_clears_on_provider_switch() {
        let config = parse_llm_provision_config(
            r#"{"llm_family":"openai","llm_model":"qwen3.8-27b","llm_key":"local-test","llm_base_url":"http://127.0.0.1:30880/v1","llm_api_type":"openai"}"#,
        ).unwrap();
        let dir = std::env::temp_dir().join(format!("octos-custom-route-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("_main.json");
        std::fs::write(&path, br#"{"config":{"custom":"keep"}}"#).unwrap();
        apply_llm_config_at_path(&path, &config.llm_family, config.llm_model.as_deref(),
            Some(&config.llm_key), config.llm_base_url.as_deref(), config.llm_api_type.as_deref()).unwrap();
        let first: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(first["config"]["llm"]["primary"]["route"]["base_url"], "http://127.0.0.1:30880/v1");
        assert_eq!(first["config"]["llm"]["primary"]["route"]["api_type"], "openai");
        assert_eq!(first["config"]["custom"], "keep");
        apply_llm_config_at_path(&path, "zai", Some("glm-5.3-flash"), None, None, None).unwrap();
        let second: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert!(second["config"]["llm"]["primary"].get("route").is_none());
        assert_eq!(second["config"]["custom"], "keep");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn llm_qr_custom_endpoint_rejects_invalid_route() {
        for url in ["", "file:///tmp/model", "http://user:secret@localhost/v1", "http://localhost/v1?key=secret"] {
            let payload = serde_json::json!({"llm_family":"openai", "llm_key":"test", "llm_base_url":url});
            assert!(parse_llm_provision_config(&payload.to_string()).is_err());
        }
        let payload = serde_json::json!({"llm_family":"openai", "llm_key":"test", "llm_api_type":"unknown"});
        assert!(parse_llm_provision_config(&payload.to_string()).is_err());
    }

    #[test]
    fn llm_qr_persistence_updates_profile_without_clobbering_other_config() {
        let unique = format!(
            "octos-qr-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let dir = std::env::temp_dir().join(unique);
        let path = dir.join("_main.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            &path,
            br#"{"config":{"memory":{"max_inject_tokens":60000},"custom":"keep-me"}}"#,
        )
        .unwrap();

        apply_llm_config_at_path(&path, "zai", Some("glm-5.2"), Some("sk-fake-test"), None, None)
            .unwrap();
        let profile: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();

        assert_eq!(profile["id"], "_main");
        assert_eq!(profile["name"], "Main");
        assert_eq!(profile["enabled"], true);
        assert!(profile["created_at"].is_string());
        assert!(profile["updated_at"].is_string());
        assert_eq!(profile["config"]["llm"]["primary"]["family_id"], "zai");
        assert_eq!(profile["config"]["llm"]["primary"]["model_id"], "glm-5.2");
        assert_eq!(profile["config"]["env_vars"]["ZAI_API_KEY"], "sk-fake-test");
        assert_eq!(profile["config"]["memory"]["max_inject_tokens"], 60000);
        assert_eq!(profile["config"]["custom"], "keep-me");

        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod qr_provision_audit {
    use super::*;

    /// What a QR actually writes, for the family this device is running.
    ///
    /// Not a behaviour change — an audit. The coding-plan route needs a
    /// base_url and reads `KIMI_API_KEY`; this records what the QR path
    /// supplies so a gap is visible rather than discovered on a phone.
    #[test]
    fn what_a_coding_plan_qr_provisions() {
        let dir = std::env::temp_dir().join(format!("qr-audit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("_main.json");
        let payload = r#"{"llm_family":"moonshot-coding","llm_model":"k3","llm_key":"sk-kimi-TEST"}"#;
        let cfg = parse_llm_provision_config(payload).expect("payload parses");
        apply_llm_config_at_path(&path, &cfg.llm_family, cfg.llm_model.as_deref(), Some(&cfg.llm_key), None, None)
            .expect("writes");
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let primary = &v["config"]["llm"]["primary"];
        eprintln!("primary  = {primary}");
        eprintln!("env_vars = {}", v["config"]["env_vars"]);
        eprintln!("key_env  = {}", key_env_for("moonshot-coding"));
        assert_eq!(primary["family_id"], "moonshot-coding");
        assert_eq!(primary["model_id"], "k3");
        // The key must land under a name octos actually reads for this family
        // (registry: api_key_env KIMI_CODING_API_KEY, aliases KIMI_API_KEY /
        // MOONSHOT_API_KEY). No route is needed — the family carries
        // default_base_url https://api.kimi.com/coding/v1.
        assert_eq!(
            v["config"]["env_vars"]["KIMI_CODING_API_KEY"], "sk-kimi-TEST",
            "a coding-plan key must be readable by octos: {}", v["config"]["env_vars"]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Families whose key env the old hand-written table got wrong: octos's
    /// registry reads these names, so a key filed under the old
    /// `<FAMILY>_API_KEY` guess (`VERTEX_API_KEY`, `Z.AI_API_KEY`,
    /// `GLM_API_KEY`, `QWEN_API_KEY`, `GOOGLE_API_KEY`, `NIM_API_KEY`) was
    /// never read.
    #[test]
    fn aliases_use_the_registry_key_env() {
        for (family, env) in [
            ("vertex", "VERTEX_SA_JSON"),
            ("z.ai", "ZAI_API_KEY"),
            ("glm", "ZHIPU_API_KEY"),
            ("qwen", "DASHSCOPE_API_KEY"),
            ("google", "GEMINI_API_KEY"),
            ("nim", "NVIDIA_API_KEY"),
        ] {
            assert_eq!(key_env_for(family), env, "{family}");
        }
    }

    /// An octos profile QR: PIN-wrapped needs the PIN, and then provisions
    /// the whole set (primary + fallback + both keys).
    #[test]
    fn octos_qr_needs_a_pin_then_provisions_the_set() {
        let dir = std::env::temp_dir().join(format!("qr-octos1e-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("_main.json");
        std::fs::write(&path, br#"{"config":{"custom":"keep"}}"#).unwrap();
        let set = octosense_llm_config::ProviderSet {
            primary: Some(octosense_llm_config::Provider::new("deepseek", Some("deepseek-chat".into()))),
            fallbacks: vec![octosense_llm_config::Provider::new("zai", Some("glm-4.6".into()))],
        };
        let secrets = std::collections::BTreeMap::from([
            ("DEEPSEEK_API_KEY".to_string(), "sk-test-0000".to_string()),
            ("ZAI_API_KEY".to_string(), "zai-test-0000".to_string()),
        ]);
        let text = octosense_llm_config::qr::encode_encrypted(
            &octosense_llm_config::qr::Provisioning { set, secrets },
            "7K3M-9QX2",
        )
        .unwrap();

        let err = apply_provision_payload_at_path(&path, &text, None).unwrap_err();
        assert!(err.contains("PIN"), "{err}");
        let err = apply_provision_payload_at_path(&path, &text, Some("0000-0000")).unwrap_err();
        assert!(err.contains("wrong PIN") && !err.contains("sk-test"), "{err}");

        let what = apply_provision_payload_at_path(&path, &text, Some("7K3M-9QX2")).unwrap();
        assert_eq!(what, "llm=deepseek/deepseek-chat (+1 fallback)");
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(v["config"]["custom"], "keep");
        assert_eq!(v["config"]["llm"]["fallbacks"][0]["family_id"], "zai");
        assert_eq!(v["config"]["env_vars"]["DEEPSEEK_API_KEY"], "sk-test-0000");
        assert_eq!(v["config"]["env_vars"]["ZAI_API_KEY"], "zai-test-0000");

        // A legacy JSON code afterwards replaces only the primary.
        let what = apply_provision_payload_at_path(
            &path,
            r#"{"llm_family":"moonshot-coding","llm_model":"k3","llm_key":"sk-kimi-TEST"}"#,
            None,
        )
        .unwrap();
        assert_eq!(what, "llm=moonshot-coding");
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(v["config"]["llm"]["primary"]["family_id"], "moonshot-coding");
        assert_eq!(v["config"]["llm"]["fallbacks"][0]["family_id"], "zai");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// No family may produce a name that cannot be an environment variable.
    #[test]
    fn no_family_yields_an_illegal_env_var() {
        for f in ["zai", "deepseek", "moonshot-coding", "kimi-coding", "zai-coding",
                  "some-new-vendor", "openrouter"] {
            let e = key_env_for(f);
            assert!(
                !e.contains('-') && e.chars().all(|c| c.is_ascii_uppercase() || c == '_'
                    || c.is_ascii_digit()),
                "{f} -> {e} is not a legal env var name"
            );
        }
    }
}
