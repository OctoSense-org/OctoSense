# Model media APIs

English | [简体中文](MEDIA.zh-CN.md)

Apps granted `model` can ask the host to generate an image, synthesize speech,
embed text, or submit a video job. The host uses credentials from **AI
providers** and selects the media model. Apps never supply or receive a key,
provider endpoint, provider model name, or provider job ID.

This implementation needs a host built with the media service; publishing a
manifest alone does not add it to an older host. The standalone `card-host`
does not provide these services. Protocol and lifecycle tests use synthetic
provider replies. Live provider calls and phone rendering are **unverified**.

## Discover and declare

Declare `"model"` in `capabilities`. Declare `"runtime"` as well if the app uses
`runtime.list` or `runtime.describe`. Required APIs prevent installation on a
host that lacks their version; optional APIs allow a fallback UI:

```json
{
  "capabilities": ["model", "runtime"],
  "host_api": {
    "required": {"model.embeddings": 1},
    "optional": {"model.image": 1, "model.audio": 1, "model.video": 1}
  }
}
```

`runtime.describe({method:"model.image"})` describes the executable API.
`model.capabilities({})` reports **configured route availability** and resource
bounds. Neither proves that the provider account has the necessary entitlement
or balance. Each operation checks current app admission and provider access.
A host with only DeepSeek configured does not advertise a working embedding,
image, speech, or video route.

Use the normal `host.request("model.embeddings", args, callback)` transport.
Success data is in `r.data`; failure is in the transport's error envelope.
All argument objects reject unknown fields. `class` is `fast` or `strong`,
defaulting to `fast`; `output:{class:"fast"}` is also accepted. Specify it in
one place only. Prompts and text are UTF-8 bytes for input limits.

## Methods, version 1

| Method | Arguments | Success data |
| --- | --- | --- |
| `model.image` | `prompt` (1–4096 bytes), optional `size`, `n:1`, `seed`, `class` | `b64_json`, `format`, `mime`, `width`, `height`, `meta` |
| `model.audio` | `text` (1–4096 bytes), optional `voice:"default"` or `"warm-female"`, `format:"mp3"`, `class` | `b64_json`, `format:"mp3"`, `mime`, `bytes`, optional `duration_s`, `meta` |
| `model.embeddings` | `input`: text or 1–16 texts, each 1–4096 bytes; optional `class` | `embedding` for one string, or ordered `embeddings` for an array; `dimensions`, `meta` |
| `model.video` | `prompt` (1–4096 bytes), optional `duration_s` (4–12, default 4), `resolution:"768P"`, `ratio` (`16:9`, `9:16`, `1:1`), `class` | Opaque `job`, `status`, `poll_after_ms`, `expires_at` |
| `model.video.status` | `job` | Same job envelope; on success, `result:{url,format:"mp4",duration_s,resolution}` |
| `model.video.cancel` | `job` | Confirmed cancellation of a queued remote job, or an error |
| `model.capabilities` | `{}` | `configured` flags, limits and lifecycle note |
| `model.budget` | `{}` | Existing call/token budget plus `media` resource usage and limits |

Image sizes are `512x512`, `1024x1024` (default), `1536x1024`, and
`1024x1536`. The OpenAI route rejects `512x512` and `seed` before submission;
the MiniMax route supports both. Only one image per request is supported.
Generated PNG/JPEG dimensions must match the request. Audio output is MP3;
apps should disclose that the voice is AI-generated. Duration is omitted
when the provider does not report it; the host does not invent a value.
Embeddings must be finite numeric vectors with consistent dimensions, at
most 4096 dimensions. Provider batch indices are validated and reordered.

H3 exposes resolution and aspect ratio, not a selectable exact pixel size,
frame rate, or seed. Unsupported video arguments such as `size`, `fps`, and
`seed` are refused. Actual video dimensions are not guessed from the
resolution label. The video result URL is provider-hosted and may expire.

## Provider selection

The host walks the configured provider list in its existing order, selecting
the first compatible route. It never retries a billable submission or falls
back after an uncertain HTTP result: the first request might already have
been accepted. Correct the configuration or request before submitting again.

| Configured family | Host-selected media models |
| --- | --- |
| OpenAI | Image: `gpt-image-1-mini` / `gpt-image-1`; speech: `gpt-4o-mini-tts`; embeddings: `text-embedding-3-small` / `text-embedding-3-large` |
| MiniMax / MiniMax CN | Image: `image-01`; speech: `speech-2.8-turbo` / `speech-2.8-hd`; video: `MiniMax-H3` |
| OpenAI-compatible / llama-server / LM Studio | Embeddings only when the operator explicitly configured a `text-embedding-*` model on an HTTPS route with a credential |

These names describe **host implementation choices**, not app request fields.
The configured chat model is not sent to a media endpoint. An Anthropic-only
route is not reused for media. The host preserves a configured proxy origin
and path, refuses credential-bearing URLs, and follows no HTTP redirects.
No key is forwarded to a returned media URL.

MiniMax H3 requires its pay-as-you-go API entitlement; an M Plan/chat account
alone is insufficient evidence. A route can be configured while its provider
still refuses an operation. Provider bodies and account details are not
returned in errors.

## Budgets and job lifecycle

Every generation/embedding submission consumes the existing shared per-app
model call/rate budget. Media also reserves its own daily units before HTTP:

| Resource | Default per app per UTC day |
| --- | --- |
| Image output pixels | 8,388,608 |
| Speech input characters | 10,000 |
| Video duration seconds | 12 |
| Embedding input UTF-8 bytes | 100,000 |

These are resource ceilings, **not a currency spending guarantee**. Hosts can
lower them through `complete::Options.media_limits`, including to zero.
Reservations persist under the host-owned `model/media-ledger.json`, outside
app storage, and are not refunded after an uncertain provider failure or
cancellation. An unreadable quota ledger refuses new submissions. Text token
accounting is not fabricated for images, audio, or video.

There are at most four media workers globally and two per app/profile. Provider
HTTP requests time out after 120 seconds; responses are capped at 4 MiB,
decoded image/audio assets at 2 MiB. No worker queue grows without a bound.
The service rechecks signed app admission, active account scope, provider
binding, and request liveness around asynchronous work. Uninstall,
withdrawal, account switching, or credential changes can refuse delivery.
Closing an isolate drops its pending reply; it cannot undo a request already
accepted by the provider.

Video jobs use random handles bound to the app, host profile, active account,
and original provider credential/route. Another app cannot query or cancel
them. At most 64 jobs are retained, for 24 hours, in the running host process;
restarting the host loses these handles. The provider may continue work after
a restart or lost submission response. Do not automatically resubmit.

Poll no faster than `poll_after_ms` (5000). Polling returns cached status inside
that window and never submits another video. Final states are `succeeded`,
`failed`, `cancelled`, or `expired`. Remote cancellation succeeds only while
MiniMax still considers the job queued. If it is already running, cancellation
returns an error and the app can keep polling; the service does not claim that
billing or generation stopped.

Results are data, never evaluated as Splash or HTML. Apps may store returned
JSON in their own storage. The image/audio base64 contract does not itself add
a playback widget or file-download permission; wire the app's renderer/player
separately. Video URLs are restricted to public-looking HTTPS hostnames, with
no credentials or IP literals, and are not fetched by this service. Ordinary
renderer/network permission and URL checks still apply.

## Agent tool aliases

A compatible App Hub policy accepts these seven reviewed `host_method`
aliases. Declare `implemented_by:"host-service"`, the `model` capability, and
`private_data:true`. Generation, embeddings and cancellation require at least
`risk:"act"`; capabilities and status allow `risk:"read"`. The app chooses its
own namespaced tool name, schemas, background/sharing policy and agent grants.
The alias does not grant provider access, remove budgets, or start an agent.

```json
{
  "name": "example.embed",
  "description": "Embed this app's short text for local search.",
  "implemented_by": "host-service",
  "host_method": "model.embeddings",
  "risk": "act",
  "private_data": true,
  "input_schema": {"type":"object","properties":{"input":{"type":"string"}},"required":["input"]},
  "output_schema": {"type":"object"}
}
```

## Implementation and verification

`complete/media/mod.rs` owns asynchronous dispatch, quotas, scopes and video
jobs. `complete/media/wire.rs` validates app requests, selects configured
routes and normalizes provider responses. `ai-host` takes admission/account
callbacks from the shell; the shell uses the same signed installed-bundle
check as app-tool execution. Existing `model.complete` now also checks request
liveness/admission/account scope before each attempt and before delivery.

`tests/media.rs` dispatches through the actual host-service registry and
adapters using synthetic responses, including malformed output, entitlement
failure, quota persistence, account/app isolation, withdrawal, cancellation
and bounded concurrency. It does not substitute for provider entitlement or
real-device rendering acceptance.

Protocol sources: [OpenAI images](https://developers.openai.com/api/reference/resources/images/methods/generate),
[OpenAI speech](https://developers.openai.com/api/reference/resources/audio/subresources/speech/methods/create),
[OpenAI embeddings](https://developers.openai.com/api/reference/resources/embeddings/methods/create),
[MiniMax's image adapter](https://github.com/MiniMax-AI/cli/blob/main/src/sdk/image/index.ts),
[MiniMax speech](https://platform.minimax.io/docs/api-reference/speech-t2a-http),
[MiniMax H3 submission](https://platform.minimax.io/docs/api-reference/video-generation-v2-create),
[status](https://platform.minimax.io/docs/api-reference/video-generation-v2-query),
and [cancellation](https://platform.minimax.io/docs/api-reference/video-generation-v2-delete).
