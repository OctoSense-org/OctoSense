# Local HTTPS test credentials

`localhost.der` is a self-signed certificate valid only for `localhost`.
`localhost-key.der` is its PKCS#8 **test-only** private key. These are used by
the loopback TLS 1.3 regression server, never by the app's production client.
The certificate expires in September 2036. Tests explicitly trust it where
needed; production uses the normal WebPKI root certificates.
