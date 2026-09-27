//! Auth slice — token, profile id, server host, plus a tiny reducer.
//!
//! No I/O here; the keychain wrapper lives in [`super::keychain`] behind a
//! feature flag. Pure data + a deterministic state transition, kept
//! Makepad-free so reducers can be unit-tested in plain `cargo test`.
//!
//! Wire endpoints we model (server: `~/home/octos/crates/octos-cli/src/api/auth_handlers.rs`):
//!
//! - `POST /api/auth/send-code` (`auth_handlers.rs:389`) — request an OTP.
//!   Server returns `ok: true` even on rate-limit / unknown email to prevent
//!   enumeration; the client always advances to `AwaitingCode` on a network
//!   success. See [`AuthEvent::CodeSent`].
//! - `GET /api/auth/status` (`auth_handlers.rs:508`) — boot probe; surfaces
//!   `email_login_enabled` / `bootstrap_mode`. Not modeled in the reducer
//!   directly; consumers fold the response into their own banner state.
//! - `POST /api/auth/verify` (`auth_handlers.rs:543`) — submits the OTP; on
//!   `ok && token` we capture the bearer via [`AuthEvent::CodeVerified`].
//! - `POST /api/auth/logout` (`auth_handlers.rs:680`) — server-side
//!   invalidation; mirrored client-side by [`AuthEvent::Logout`]. Keychain
//!   delete is a separate side-effect (`super::keychain::delete_token`).
//!
//! See `workstreams/W08-auth-tenancy.md` for the full design (LoginScreen
//! state machine, redirect preservation, multi-account note).

use std::fmt;

/// Bearer token. `Debug` and `Display` are redacted; the only way to read
/// the inner value is [`SecretToken::expose`]. Transports must scrub
/// `Authorization` headers from any trace logs they emit (W08 risk table).
#[derive(Clone, PartialEq, Eq)]
pub struct SecretToken(String);

impl SecretToken {
    /// Borrow the underlying token. Callers MUST NOT log the result.
    pub fn expose(&self) -> &str { &self.0 }
}

impl From<String> for SecretToken {
    fn from(s: String) -> Self { Self(s) }
}

impl fmt::Debug for SecretToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretToken(<redacted>)")
    }
}

impl fmt::Display for SecretToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str("<redacted>") }
}

/// Profile identifier (`X-Profile-Id` header value).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProfileId(String);

impl ProfileId {
    pub fn as_str(&self) -> &str { &self.0 }
}

impl From<String> for ProfileId {
    fn from(s: String) -> Self { Self(s) }
}

impl fmt::Display for ProfileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0) }
}

/// Host portion of the server base URL (e.g. `octos.ominix.io`). First
/// segment of the keychain service name; see [`super::keychain`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ServerHost(String);

impl ServerHost {
    pub fn as_str(&self) -> &str { &self.0 }
}

impl From<String> for ServerHost {
    fn from(s: String) -> Self { Self(s) }
}

impl fmt::Display for ServerHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0) }
}

/// Auth slice of `AppState`. The full W08 `Auth` struct also tracks
/// `profile_meta` and `server_auth_status`; those land with the LoginScreen
/// task.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuthSlice {
    pub token: Option<SecretToken>,
    pub profile_id: Option<ProfileId>,
    pub server_host: Option<ServerHost>,
    /// Stashed before the navigator routes to Login on a 401/403, so the
    /// reducer can dispatch `NavigateTo(redirect)` after a successful verify.
    pub redirect_after_login: Option<String>,
}

/// Events the auth reducer consumes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthEvent {
    /// Triggers `POST /api/auth/send-code` (`auth_handlers.rs:389`).
    LoginRequested { email: String },
    /// Successful `/send-code`. Move UI to `AwaitingCode`.
    CodeSent,
    /// Successful `/verify` (`auth_handlers.rs:543`). `profile` is optional
    /// because some deployments resolve it via a follow-up
    /// `GET /api/my/profile`.
    CodeVerified { token: SecretToken, profile: Option<ProfileId> },
    /// `ok: false` from the server, or transport error.
    AuthError(String),
    /// Local logout, server-side logout (`auth_handlers.rs:680`), or 401/403.
    /// Wipes the slice but preserves `server_host` and `redirect_after_login`.
    Logout,
}

/// Apply an `AuthEvent` to an `AuthSlice`. Pure: no I/O, no logging.
pub fn reduce(slice: &mut AuthSlice, event: AuthEvent) {
    match event {
        // No durable state changes — these drive the LoginScreen state
        // machine, not the slice. Kept as variants so the reducer is the
        // single point that observes auth flow events (telemetry hook later).
        AuthEvent::LoginRequested { .. } | AuthEvent::CodeSent | AuthEvent::AuthError(_) => {}
        AuthEvent::CodeVerified { token, profile } => {
            slice.token = Some(token);
            if profile.is_some() {
                slice.profile_id = profile;
            }
        }
        AuthEvent::Logout => {
            slice.token = None;
            slice.profile_id = None;
            // server_host and redirect_after_login intentionally preserved.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tok(s: &str) -> SecretToken { SecretToken::from(s.to_string()) }
    fn pid(s: &str) -> ProfileId { ProfileId::from(s.to_string()) }
    fn host(s: &str) -> ServerHost { ServerHost::from(s.to_string()) }

    #[test]
    fn debug_and_display_redact_token() {
        let t = tok("super-secret-bearer-xyz");
        let dbg = format!("{:?}", t);
        let disp = format!("{}", t);
        assert!(!dbg.contains("super-secret-bearer-xyz"), "Debug leaked: {dbg}");
        assert!(!disp.contains("super-secret-bearer-xyz"), "Display leaked: {disp}");
        assert!(dbg.contains("redacted") && disp.contains("redacted"));
    }

    #[test]
    fn debug_of_slice_redacts_token() {
        let slice = AuthSlice {
            token: Some(tok("leak-me-if-you-can")),
            profile_id: Some(pid("acme")),
            server_host: Some(host("octos.example.com")),
            redirect_after_login: Some("Home".to_string()),
        };
        let dbg = format!("{:?}", slice);
        assert!(!dbg.contains("leak-me-if-you-can"), "AuthSlice Debug leaked: {dbg}");
    }

    #[test]
    fn expose_returns_inner_and_lifecycle_events_are_noop_on_slice() {
        assert_eq!(tok("abc").expose(), "abc");
        let mut s = AuthSlice { token: Some(tok("keep")), ..Default::default() };
        let before = s.clone();
        reduce(&mut s, AuthEvent::LoginRequested { email: "a@b.c".to_string() });
        reduce(&mut s, AuthEvent::CodeSent);
        reduce(&mut s, AuthEvent::AuthError("bad".to_string()));
        assert_eq!(s, before);
    }

    #[test]
    fn code_verified_stores_token_and_profile() {
        let mut s = AuthSlice::default();
        reduce(&mut s, AuthEvent::CodeVerified { token: tok("tok"), profile: Some(pid("acme")) });
        assert_eq!(s.token.as_ref().map(|t| t.expose()), Some("tok"));
        assert_eq!(s.profile_id, Some(pid("acme")));
    }

    #[test]
    fn code_verified_without_profile_keeps_existing_profile() {
        let mut s = AuthSlice { profile_id: Some(pid("existing")), ..Default::default() };
        reduce(&mut s, AuthEvent::CodeVerified { token: tok("tok"), profile: None });
        assert_eq!(s.token.as_ref().map(|t| t.expose()), Some("tok"));
        assert_eq!(s.profile_id, Some(pid("existing")));
    }

    #[test]
    fn logout_clears_token_and_profile_but_preserves_host_and_redirect() {
        let mut s = AuthSlice {
            token: Some(tok("tok")),
            profile_id: Some(pid("acme")),
            server_host: Some(host("octos.example.com")),
            redirect_after_login: Some("Chat".to_string()),
        };
        reduce(&mut s, AuthEvent::Logout);
        assert!(s.token.is_none() && s.profile_id.is_none());
        assert_eq!(s.server_host, Some(host("octos.example.com")));
        assert_eq!(s.redirect_after_login, Some("Chat".to_string()));
    }

    #[test]
    fn full_login_sequence_is_deterministic() {
        let mut s = AuthSlice { server_host: Some(host("octos.example.com")), ..Default::default() };
        reduce(&mut s, AuthEvent::LoginRequested { email: "a@b.c".to_string() });
        reduce(&mut s, AuthEvent::CodeSent);
        reduce(&mut s, AuthEvent::CodeVerified { token: tok("tok"), profile: Some(pid("acme")) });
        assert_eq!(s.token.as_ref().map(|t| t.expose()), Some("tok"));
        assert_eq!(s.profile_id, Some(pid("acme")));
        assert!(s.server_host.is_some());
    }
}
