use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Github,
    Google,
}

pub fn scope_words(scope: &str) -> &str {
    match scope {
        "read:user" => "Read your GitHub profile",
        "public_repo" => "Read and update your public repositories",
        "repo" => "Read and update your public and private repositories",
        "openid" => "Verify your Google account identity",
        "email" => "Read your verified account email address",
        "profile" => "Read your account profile",
        "https://www.googleapis.com/auth/calendar.calendarlist.readonly" => "List your Google calendars",
        "https://www.googleapis.com/auth/calendar.events" => "Read and edit Google Calendar events",
        "https://www.googleapis.com/auth/gmail.readonly" => "Read Gmail messages and folders",
        "https://www.googleapis.com/auth/gmail.send" => "Send email after native reply review",
        _ => scope,
    }
}

impl Provider {
    pub(crate) fn sign_in_unavailable(self) -> &'static str {
        match self {
            Self::Github => "GitHub sign-in is unavailable in this build. Check for an OctoSense update or contact its distributor.",
            Self::Google => "Google sign-in is unavailable in this build. Check for an OctoSense update or contact its distributor.",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Github => "github",
            Self::Google => "google",
        }
    }
    pub fn token_endpoint(self) -> &'static str {
        match self {
            Self::Github => "https://github.com/login/oauth/access_token",
            Self::Google => "https://oauth2.googleapis.com/token",
        }
    }
    pub fn api_origin(self) -> &'static str {
        match self {
            Self::Github => "https://api.github.com",
            Self::Google => "https://www.googleapis.com",
        }
    }
    pub fn validate_scopes(self, scopes: &[String]) -> Result<BTreeSet<String>, String> {
        let allowed: &[&str] = match self {
            Self::Github => &["read:user", "public_repo", "repo"],
            Self::Google => &[
                "openid",
                "email",
                "profile",
                "https://www.googleapis.com/auth/calendar.calendarlist.readonly",
                "https://www.googleapis.com/auth/calendar.events",
                "https://www.googleapis.com/auth/gmail.readonly",
                "https://www.googleapis.com/auth/gmail.send",
            ],
        };
        // App-facing aliases keep provider scope URI constants in the host.
        // A URI in an OAuth scope is not a grant of arbitrary network access.
        let canonical: Vec<&str> = scopes.iter().map(|scope| match (self, scope.as_str()) {
            (Provider::Google, "calendar.list") => "https://www.googleapis.com/auth/calendar.calendarlist.readonly",
            (Provider::Google, "calendar.events") => "https://www.googleapis.com/auth/calendar.events",
            (Provider::Google, "mail.read") => "https://www.googleapis.com/auth/gmail.readonly",
            (Provider::Google, "mail.send") => "https://www.googleapis.com/auth/gmail.send",
            (_, scope) => scope,
        }).collect();
        if scopes.is_empty()
            || scopes.len() > allowed.len()
            || canonical.iter().any(|s| !allowed.contains(s))
        {
            return Err("Unsupported or empty OAuth scope request".into());
        }
        Ok(canonical.into_iter().map(str::to_string).collect())
    }
}

/// Public registration data, read from host configuration, not a bundle.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientRegistration {
    pub client_id: String,
}

impl ClientRegistration {
    pub fn validate(&self) -> Result<(), String> {
        if self.client_id.is_empty()
            || self.client_id.len() > 256
            || self.client_id.chars().any(char::is_control)
        {
            return Err("Configure the provider's OAuth client ID in OctoSense".into());
        }
        Ok(())
    }
}
