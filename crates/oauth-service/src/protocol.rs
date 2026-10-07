//! OAuth protocol construction uses oauth2; all HTTP still crosses the host's
//! bounded, no-redirect Transport. This adapter does not use oauth2 HTTP clients.
use crate::{
    oauth::AuthorizationCode,
    providers::{ClientRegistration, Provider},
    transport::{json_ok, Body, Request, Transport},
};
use oauth2::{
    basic::BasicClient, AuthType, AuthorizationCode as Code, ClientId, ClientSecret,
    DeviceAuthorizationUrl, EndpointNotSet, EndpointSet, ErrorResponse, HttpRequest, HttpResponse,
    PkceCodeVerifier, RedirectUrl, RefreshToken, RequestTokenError, Scope,
    StandardDeviceAuthorizationResponse, TokenUrl,
};
use serde_json::Value;
use std::{collections::BTreeSet, fmt};
use url::Url;

type TokenClient =
    BasicClient<EndpointNotSet, EndpointNotSet, EndpointNotSet, EndpointNotSet, EndpointSet>;

// Only sanitized host errors enter this type. Never format oauth2 errors:
// their parse/server variants can contain response bodies and credentials.
#[derive(Debug)]
struct TransportError(String);
impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for TransportError {}

fn send(transport: &dyn Transport, request: HttpRequest) -> Result<HttpResponse, TransportError> {
    let endpoint = request.uri().to_string();
    if request.method() != oauth2::http::Method::POST
        || ![
            "https://github.com/login/device/code",
            Provider::Google
                .token_endpoint()
                .expect("fixed Google endpoint"),
        ]
        .contains(&endpoint.as_str())
        || request
            .headers()
            .contains_key(oauth2::http::header::AUTHORIZATION)
        || request.headers().get(oauth2::http::header::CONTENT_TYPE)
            != Some(&oauth2::http::HeaderValue::from_static(
                "application/x-www-form-urlencoded",
            ))
        || request.body().len() > 65_536
    {
        return Err(TransportError("Unsupported OAuth protocol request".into()));
    }
    let fields = url::form_urlencoded::parse(request.body())
        .into_owned()
        .collect();
    let response = transport
        .send(Request {
            method: "POST",
            url: Url::parse(&endpoint).expect("validated fixed OAuth endpoint"),
            bearer: None,
            if_match: None,
            body: Body::Form(fields),
        })
        .map_err(|_| TransportError("Provider connection failed; check the network".into()))?;
    // Keep the existing status policy and its redacted errors. oauth2 handles
    // the successful JSON schema; it never sees or formats provider errors.
    let value = json_ok(response).map_err(TransportError)?;
    let bytes = serde_json::to_vec(&value)
        .map_err(|_| TransportError("Provider returned invalid JSON".into()))?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err(TransportError("Provider response exceeds 2 MiB".into()));
    }
    oauth2::http::Response::builder()
        .status(200)
        .header(oauth2::http::header::CONTENT_TYPE, "application/json")
        .body(bytes)
        .map_err(|_| TransportError("Cannot read OAuth response".into()))
}

fn redacted<E: ErrorResponse>(error: RequestTokenError<TransportError, E>) -> String {
    match error {
        RequestTokenError::Request(error) => error.0,
        _ => "Provider returned an invalid OAuth response".into(),
    }
}

fn token_client(client: &ClientRegistration, secret: Option<&str>) -> TokenClient {
    let mut client = BasicClient::new(ClientId::new(client.client_id.clone()))
        .set_auth_type(AuthType::RequestBody)
        .set_token_uri(
            TokenUrl::new(
                Provider::Google
                    .token_endpoint()
                    .expect("fixed Google endpoint")
                    .into(),
            )
            .unwrap(),
        );
    if let Some(secret) = secret {
        client = client.set_client_secret(ClientSecret::new(secret.into()));
    }
    client
}

fn response_value(value: impl serde::Serialize) -> Result<Value, String> {
    // Tokens::from_response remains the host's bounded token and scope gate.
    serde_json::to_value(value).map_err(|_| "Invalid OAuth response".into())
}

fn google_response_value(value: impl serde::Serialize) -> Result<Value, String> {
    let mut value = response_value(value)?;
    // Google may return these identity scopes as full Google API names even
    // when the native authorization requested their OpenID Connect aliases.
    // Normalize only the two documented equivalents, only on Google responses.
    // Do not infer openid, API permissions, or a missing scope value.
    // https://developers.google.com/identity/protocols/oauth2#basicsteps
    // https://developers.google.com/identity/protocols/oauth2/scopes#openid-connect
    if let Some(scope) = value.get("scope").and_then(Value::as_str) {
        let canonical = scope
            .split_whitespace()
            .map(|scope| match scope {
                "https://www.googleapis.com/auth/userinfo.email" => "email",
                "https://www.googleapis.com/auth/userinfo.profile" => "profile",
                other => other,
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(" ");
        value["scope"] = Value::String(canonical);
    }
    Ok(value)
}

pub(crate) fn github_device_authorization(
    client: &ClientRegistration,
    scopes: &BTreeSet<String>,
    transport: &dyn Transport,
) -> Result<Value, String> {
    let client = BasicClient::new(ClientId::new(client.client_id.clone()))
        .set_auth_type(AuthType::RequestBody)
        .set_device_authorization_url(
            DeviceAuthorizationUrl::new("https://github.com/login/device/code".into()).unwrap(),
        );
    let response: StandardDeviceAuthorizationResponse = client
        .exchange_device_code()
        .add_scopes(scopes.iter().cloned().map(Scope::new))
        .request(&|request| send(transport, request))
        .map_err(redacted)?;
    response_value(response)
}

pub(crate) fn google_code(
    client: &ClientRegistration,
    secret: Option<&str>,
    code: AuthorizationCode,
    transport: &dyn Transport,
) -> Result<Value, String> {
    let client = token_client(client, secret)
        .set_redirect_uri(RedirectUrl::new(code.redirect).map_err(|_| "Invalid OAuth callback")?);
    google_response_value(
        client
            .exchange_code(Code::new(code.code))
            .set_pkce_verifier(PkceCodeVerifier::new(code.verifier))
            .request(&|request| send(transport, request))
            .map_err(redacted)?,
    )
}

pub(crate) fn google_refresh(
    client: &ClientRegistration,
    secret: Option<&str>,
    refresh: &str,
    transport: &dyn Transport,
) -> Result<Value, String> {
    google_response_value(
        token_client(client, secret)
            .exchange_refresh_token(&RefreshToken::new(refresh.into()))
            .request(&|request| send(transport, request))
            .map_err(redacted)?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::Response;
    use serde_json::json;
    use std::sync::Mutex;

    struct Fixture {
        calls: Mutex<Vec<Request>>,
        response: Value,
        status: u16,
    }
    impl Transport for Fixture {
        fn send(&self, request: Request) -> Result<Response, String> {
            self.calls.lock().unwrap().push(request);
            Ok(Response {
                status: self.status,
                body: self.response.clone(),
                etag: None,
            })
        }
    }
    fn client() -> ClientRegistration {
        ClientRegistration {
            client_id: "fixture-client".into(),
        }
    }
    fn fixture(response: Value) -> Fixture {
        Fixture {
            calls: Mutex::new(vec![]),
            response,
            status: 200,
        }
    }

    fn google_responses(response: Value) -> [Value; 2] {
        let fixture = fixture(response);
        [
            google_code(
                &client(),
                None,
                AuthorizationCode {
                    code: "fixture-code".into(),
                    verifier: "fixture-verifier".into(),
                    redirect: "http://127.0.0.1:32123/oauth/callback".into(),
                },
                &fixture,
            )
            .unwrap(),
            google_refresh(&client(), None, "fixture-refresh", &fixture).unwrap(),
        ]
    }

    #[test]
    fn google_code_and_refresh_normalize_only_documented_identity_equivalents() {
        use crate::oauth::Tokens;
        let requested = BTreeSet::from([
            "openid".into(),
            "email".into(),
            "profile".into(),
            "https://www.googleapis.com/auth/calendar.events".into(),
        ]);
        let raw = json!({"access_token":"fixture-token","token_type":"Bearer",
            "scope":"openid https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile https://www.googleapis.com/auth/calendar.events https://www.googleapis.com/auth/gmail.send"});
        // The shared token parser must not acquire provider-specific aliases.
        assert!(Tokens::from_response(&raw, &requested, 0).is_err());
        for value in google_responses(raw) {
            let token = Tokens::from_response(&value, &requested, 0).unwrap();
            assert_eq!(token.scopes, requested);
            assert!(!token
                .scopes
                .contains("https://www.googleapis.com/auth/gmail.send"));
            let returned: BTreeSet<_> = value["scope"]
                .as_str()
                .unwrap()
                .split_whitespace()
                .collect();
            assert!(returned.contains("email") && returned.contains("profile"));
            assert!(returned.contains("https://www.googleapis.com/auth/calendar.events"));
            assert!(!returned.contains("https://www.googleapis.com/auth/userinfo.email"));
        }
    }

    #[test]
    fn google_identity_aliases_do_not_hide_missing_grants_or_accept_lookalikes() {
        use crate::oauth::Tokens;
        let requested = BTreeSet::from(["openid".into(), "email".into(), "profile".into()]);
        for scope in [
            "openid https://www.googleapis.com/auth/userinfo.profile",
            "openid https://www.googleapis.com/auth/userinfo.email",
            "https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile",
            "openid profile https://www.googleapis.com/auth/userinfo.email.evil",
            "openid profile http://www.googleapis.com/auth/userinfo.email",
        ] {
            for value in google_responses(json!({"access_token":"fixture-token","token_type":"Bearer","scope":scope})) {
                assert!(Tokens::from_response(&value, &requested, 0).is_err());
            }
        }
        let mut needs_mail = requested.clone();
        needs_mail.insert("https://www.googleapis.com/auth/gmail.readonly".into());
        for value in google_responses(json!({"access_token":"fixture-token","token_type":"Bearer",
            "scope":"openid https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile"}))
        {
            assert!(Tokens::from_response(&value, &needs_mail, 0).is_err());
        }
        // An omitted scope keeps the existing OAuth rule; no guessed alias is
        // inserted into the response. The generic parser handles this case.
        for value in google_responses(json!({"access_token":"fixture-token","token_type":"Bearer"}))
        {
            assert!(value.get("scope").is_none());
            assert!(Tokens::from_response(&value, &requested, 0).is_ok());
        }
    }

    #[test]
    fn library_code_exchange_keeps_pkce_redirect_and_secret_in_bounded_host_transport() {
        let fixture = fixture(json!({"access_token":"fixture-token","token_type":"Bearer"}));
        google_code(
            &client(),
            Some("fixture-registration"),
            AuthorizationCode {
                code: "fixture+code&with=escapes".into(),
                verifier: "fixture-verifier".into(),
                redirect: "http://127.0.0.1:32123/oauth/callback".into(),
            },
            &fixture,
        )
        .unwrap();
        let calls = fixture.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        let request = &calls[0];
        assert_eq!(
            request.url.as_str(),
            Provider::Google
                .token_endpoint()
                .expect("fixed Google endpoint")
        );
        assert!(request.bearer.is_none() && request.if_match.is_none());
        let Body::Form(fields) = &request.body else {
            panic!("Expected OAuth form")
        };
        let fields: std::collections::BTreeMap<_, _> = fields.iter().cloned().collect();
        assert_eq!(fields.len(), 6);
        assert_eq!(fields["client_id"], "fixture-client");
        assert_eq!(fields["client_secret"], "fixture-registration");
        assert_eq!(fields["code"], "fixture+code&with=escapes");
        assert_eq!(fields["code_verifier"], "fixture-verifier");
        assert_eq!(
            fields["redirect_uri"],
            "http://127.0.0.1:32123/oauth/callback"
        );
        assert_eq!(fields["grant_type"], "authorization_code");
    }

    #[test]
    fn library_parse_and_provider_http_errors_never_include_response_secrets() {
        let mut fixture = fixture(json!({"access_token":"fixture-secret","token_type":123,
            "error_description":"fixture-private-provider-diagnostic"}));
        assert_eq!(
            google_refresh(&client(), None, "fixture-refresh", &fixture).unwrap_err(),
            "Provider returned an invalid OAuth response"
        );
        fixture.status = 401;
        assert_eq!(
            google_refresh(&client(), None, "fixture-refresh", &fixture).unwrap_err(),
            "Sign in again: provider authorization expired"
        );
    }

    #[test]
    fn library_device_start_and_refresh_use_expected_forms_without_authorization_headers() {
        let device = fixture(
            json!({"device_code":"fixture-device-code","user_code":"ABCD-EFGH",
            "verification_uri":"https://github.com/login/device","expires_in":900}),
        );
        github_device_authorization(&client(), &BTreeSet::from(["read:user".into()]), &device)
            .unwrap();
        let calls = device.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].url.as_str(),
            "https://github.com/login/device/code"
        );
        assert!(calls[0].bearer.is_none());
        let Body::Form(fields) = &calls[0].body else {
            panic!("Expected device authorization form")
        };
        let fields: std::collections::BTreeMap<_, _> = fields.iter().cloned().collect();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields["client_id"], "fixture-client");
        assert_eq!(fields["scope"], "read:user");
        let refresh = fixture(json!({"access_token":"fixture-token","token_type":"bearer"}));
        google_refresh(&client(), None, "fixture+refresh&escaped", &refresh).unwrap();
        let calls = refresh.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert!(calls[0].bearer.is_none());
        let Body::Form(fields) = &calls[0].body else {
            panic!("Expected refresh form")
        };
        let fields: std::collections::BTreeMap<_, _> = fields.iter().cloned().collect();
        assert_eq!(fields.len(), 3);
        assert_eq!(fields["client_id"], "fixture-client");
        assert_eq!(fields["refresh_token"], "fixture+refresh&escaped");
        assert_eq!(fields["grant_type"], "refresh_token");
    }

    #[test]
    fn protocol_adapter_refuses_unreviewed_endpoint_and_auth_header_before_transport() {
        let fixture = fixture(json!({}));
        for (endpoint, authorization) in [
            ("https://attacker.invalid/token", false),
            (
                Provider::Google
                    .token_endpoint()
                    .expect("fixed Google endpoint"),
                true,
            ),
        ] {
            let mut request = oauth2::http::Request::builder()
                .method("POST")
                .uri(endpoint)
                .header("content-type", "application/x-www-form-urlencoded");
            if authorization {
                request = request.header("authorization", "Basic fixture-private");
            }
            assert!(send(
                &fixture,
                request.body(b"client_id=fixture".to_vec()).unwrap()
            )
            .is_err());
        }
        assert!(fixture.calls.lock().unwrap().is_empty());
    }
}
