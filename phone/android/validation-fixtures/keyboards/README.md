# Keyboard validation fixtures

`keyboards-aware` and `keyboards-unaware` are separately signed third-party IMEs.
They share Java sources from `keyboards-provider`, expose English and Chinese
subtypes, and differ in their declared Direct Boot capability. Neither requests
network access, input-method privileges, or any other permission. Android owns
their BIND_INPUT_METHOD-protected service binding. Their read-only observation
receivers require DUMP.

The IMEs never read surrounding text. The sole commit button sends a fixed test
string only when both the latest `onStartInput` and the current EditorInfo name
the exact `dev.makepad.octosense.keyboardsfixture` package. Other editors receive
no input and have no text or package identifiers recorded. The provider-owned
Settings activity records only its own launch count.

The `keyboards` app supplies a native EditText consumer and DUMP-gated observations
of that editor's own text, visibility, and focus. These are independent of Home's
Settings model and Binder protocol. A successful input test must observe both
the provider's accepted commit and the actual editor contents.

`home/android/scripts/build-keyboards-fixtures.py` builds all three APKs against
SDK35 with distinct temporary signing identities. It does not call ADB. Keep the
output directory while exercising uninstall/reinstall so incarnation tests use
the intended original signer.

Device validation belongs only to the disposable emulator5560; ordinary
unavailable/recovery checks belong to emulator5556. Do not install or enable these
fixtures on a physical phone. Before any enabled/default/subtype change, capture
the complete native provider inventory and exact secure rows (including absent
and empty values), prove the original keyboard remains recoverable, and restore
native state before removing the fixtures. Native warning Cancel/Back must not
be treated as consent. The end-to-end runner is added with the matching candidate
UI; compilation alone does not establish keyboard behavior.
