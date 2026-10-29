# ZakoAI Proxy

[简体中文](README.md) · English · [Security](SECURITY.md) · [API contract](API_CONTRACT.md)

A self-hosted Rust Responses / Chat Completions / Anthropic Messages API relay with a responsive desktop/mobile administration console, SQLite persistence, and Docker Compose deployment. The console includes a dashboard, groups, upstream accounts, client API keys, service providers, request logs, and system settings.

Current release: **v0.13.2**, fixing provider import status labels and supporting current New API personal access tokens. It retains the chat layout from **v0.13.1** and provider login/session fixes from **v0.12.2**.

## Relay Recommendation: 杂鱼 API

[Visit 杂鱼 API](https://zayuapi.com)

![杂鱼 API](https://www.mizaawa.com/usr/uploads/2026/10/2898908578.png)

[Deployment tutorial (Chinese)](https://www.mizaawa.com/index.php/archives/95/)

## Deploy With Docker Compose

Requires Docker Engine/Desktop with Docker Compose v2. Build from the repository:

```sh
git clone https://github.com/mizaawa/ZakoProxyCLI.git
cd ZakoProxyCLI
docker compose pull
docker compose build --pull
docker compose up -d --no-build
docker compose ps
```

Open **http://localhost:1990**. On a fresh data volume, create your administrator username and a password of at least 12 characters. There are no built-in administrator credentials. Subsequent visits use the same account. Initialize the account before making the service reachable by other people.

The pull command downloads the published image; `build --pull` refreshes its Node/Rust/Debian base images and rebuilds the application from this checkout. `up -d` starts it. The Compose image name is `ghcr.io/mizaawa/zakoai-proxy-cli:latest`; successful default-branch CI runs also publish this image and a commit-SHA tag.

Compose binds port **1990 to loopback by default**. Set `ZAKO_BIND_IP=0.0.0.0` only when you intend to expose the port. `ZAKO_PORT` changes the host port. Put these values in an uncommitted `.env`, for example:

```dotenv
ZAKO_BIND_IP=127.0.0.1
ZAKO_PORT=1990
TRUST_PROXY=0
```

For public deployments, terminate HTTPS at a trusted reverse proxy and keep the application port private. Set `TRUST_PROXY=1` only behind that proxy; preserve the original `Host` and send `X-Forwarded-Proto: https` so sessions receive `Secure` cookies. Forward streaming responses without buffering and allow a read timeout longer than the configured request timeout.

```sh
docker compose logs --tail=100 -f
curl --fail http://localhost:1990/health
docker compose down
```

`down` preserves the named data volume. `down -v` deletes the administrator account, configuration, keys, and statistics.

### Update From The Console

From v0.5.0, signing in checks for updates automatically, then every 15 minutes while the page is visible. Hidden tabs pause checks and signing out stops them. A yellow dotted **有新版本** badge beside the title opens System Settings and focuses the update section. Manual checks share the same state. Server lookups coalesce requests, cache success for 5 minutes, and back off failures for 1 minute. Notifications work without an updater; executing an update still requires one.

From v0.3.0, **System Settings → GitHub Source Updates** checks the latest stable GitHub Release. Starting an update requires the current administrator password again. The updater checks out the official release tag, builds a new image from source, and reports build, deployment, and health-check logs. The old version keeps serving during the build; switching containers briefly interrupts connections, after which the page can reconnect to show the result.

Compose also runs an isolated `updater` service with no published host port. The public app uses the internal `UPDATER_URL=http://updater:9000/` and a read-only `/control/token` mount. Only the updater receives the Docker socket. Docker control grants effectively root-level access to the host, so deploy it only on a trusted server and never expose its internal interface or token.

After a successful build, the updater stops the proxy, backs up its complete data directory, and starts the new image. Failed deployment or health checks restore the previous version and its matching data backup. Update state and backups persist in `update-state`; they do not replace an off-host backup. Accounts, API keys, groups, statistics, and `master.key` remain in the data volume. Only stable tags from the fixed official repository are accepted, not arbitrary URLs or commands.

Existing installations must first pull this source version and run the Compose build/start commands above once to install the updater. Keep `COMPOSE_PROJECT_NAME` consistent when using a custom project name. Without an updater, such as in local development, the console shows version information but cannot execute updates. Updating the proxy image does not replace the updater itself or change the host's Compose file; deployment changes called out in release notes still require administrator action.

### First Upgrade After The v0.6.0 Repository Rename

The official repository is now ZakoProxyCLI. The updater in v0.5.0 and earlier strictly matches the old Release URL, so console checks or updates may fail after the rename. Run the following once from the existing deployment directory to replace both proxy and updater. Keep the directory, Compose project name, and named volumes unchanged. Resolve local changes first if the pull cannot fast-forward.

```sh
git remote set-url origin https://github.com/mizaawa/ZakoProxyCLI.git
git pull --ff-only origin main
docker compose build --pull
docker compose up -d --no-build
docker compose ps
```

Images retain ghcr.io/mizaawa/zakoai-proxy-cli for existing deployments. v0.6.0 accepts both exact official Release URL paths; the old updater cannot automatically perform this initial migration.

## Configure The Relay

1. **Groups:** create routing groups with an optional multiline introduction (`description`, up to 2,000 characters). Groups determine account membership and client-key access scope. Specific profiles and the default-strategy switch belong to each client key. Introductions default to empty and appear when choosing a key's group. Detach referenced accounts and keys before deleting a group.
2. **Accounts:** choose **From API** to enter an upstream endpoint and key, or **From service provider** to bind a key from a saved, authenticated provider; keep or adjust the default `responses`, `compact`, `chat`, and `anthropic` selections according to your relay provider, discover/select actual upstream models, and assign groups. `chat` uses `/v1/chat/completions`; `anthropic` uses `/v1/messages` and Anthropic authentication for model discovery. Model names do not impose a protocol: Grok, Gemini, DeepSeek, or GLM can use the OpenAI-compatible or Anthropic-compatible interface exposed by your relay provider. Optional `model_mappings` maps requested model IDs to actual upstream model IDs; every target must be selected in the account's available models. Resolution is single-hop, never recursive; a request without a mapping uses the same model ID. Endpoint roots with or without `/v1` work. Set RPM and concurrency to `0` for unlimited. A blank secret when editing a manually configured account retains the existing secret. Provider-bound accounts resolve the endpoint and secret on the server; switching to manual API import requires a new secret. The optional proxy input between model mapping and groups accepts `socks5://host:port` or `socks5h://host:port`, including username/password authentication (percent-encode special characters). The adjacent test button requests the upstream models endpoint through the proxy and shows elapsed time, HTTP status, or a sanitized error. HTTP 401/403 means the proxy connected but upstream authorization failed. URLs are encrypted at rest; an untouched blank edit retains the saved proxy, while Remove Proxy plus Save clears it. All four protocols, streaming, and model discovery use the account proxy. A failed proxy never silently falls back to a direct connection for that account; failover uses the next account transport. Docker localhost refers to the container itself; use an address reachable from that container.
3. **API keys:** create a client key for a group and independently set its `profile_ids` and `default_strategy_enabled`. New keys start with no profiles and the default strategy off. Choices are filtered by actual models selected on enabled, available accounts in the group; invalid provider bindings do not contribute models, and alias names alone do not enable profiles. A selected profile matching the mapped actual model takes precedence; the default applies only with no match and this key's switch on. At most one profile is injected, with overlapping variants mutually exclusive. Changing the group prunes unsupported choices without rotating the key or changing other keys. Copy or reveal it in the authenticated console. Disabling/deleting it blocks new requests; the base URL is the site origin plus `/v1`.
4. **System settings:** set the console entry path, retry budget, failure threshold, cooldown, request timeout, and statistics retention. Changing a password requires the current password and matching new-password confirmation; it revokes all sessions. Confirmation is not TOTP or another independent authentication factor.

For example, the account configuration fragment `{"models":["gpt-6-astra"],"model_mappings":{"chat-main":"gpt-6-astra"}}` accepts a client request for `chat-main` and sends `model: "gpt-6-astra"` upstream. Selecting `gpt-6-astra` on the authenticated client key applies that actual model's profile, not a profile for the alias. An unmapped request for `gpt-6-astra` still uses that same actual model. These mapping and profile matching rules apply to every supported protocol.

### Service Providers And Upstream Keys

**Service providers** supports Sub2API and New API using the site's account/password or access-token authentication. Cards show balance, daily and total usage, and cumulative recharge. Missing values remain `—`, estimated recharge is labeled, and CNY presentation uses each provider's recharge multiplier. Credentials are encrypted at rest and omitted from public results.

The built-in **杂鱼AI · 官方** Sub2API entry uses the fixed URL `https://www.zayuapi.com` and is pinned first. Its name and site URL cannot be changed. Choose **Edit** on its card to enter your site account/password or access token, test the login and save. The configured entry can refresh statistics, read and create upstream keys, and import accounts; its configuration and encrypted credentials persist across restarts. Before configuration, it displays a neutral statistics-unavailable state. The official entry's credentials can be edited, but the entry cannot be deleted. Other saved providers can be edited, deleted and refreshed.

In account creation, **From service provider** lists existing keys with their names, upstream group names, ratios and availability. Upstream groups are separate from local routing groups. The backend owns the bound endpoint and secret. Editing an imported account refreshes its binding; confirmed deleted, disabled, expired or exhausted keys stop participating in routing. A changed site or authentication identity requires explicit reselection; transient network errors preserve the last confirmed binding.

Choose **Create from provider** at the top of the key list, then manually select an available group in the second dialog. **Create** sends the server-generated name `【ZakoAI】` followed by the original group name; **Cancel** sends no creation request. On success, the dialog closes and the list refreshes. The UI snapshots fresh key IDs immediately before creation and selects the unique new, available key matching the returned name and group, excluding old same-name keys. Refresh again if visibility is delayed; multiple matching new keys require manual selection. A failed response can follow a committed upstream write, so creation is never automatically retried: refresh and confirm first.

Sub2API creates a key in the selected group. New API uses unlimited quota, no expiry, no model or IP restrictions and no cross-group retry; adjust restrictions at the upstream site if needed. These are upstream credentials, distinct from local client keys used to access this relay.

### v0.13.2 Provider Import and New API Compatibility

- Show the official provider's actual connected, sync-failed, pending, or signed-out state in the account import picker.
- Accept current New API `nap_` personal access tokens without a manually entered user ID for balance, usage, and API-key import. Grant `profile:read`, `usage:read`, `api_key:read`, and `api_key:reveal`; creating upstream keys also requires `api_key:write`.
- Support the short-lived access token returned by current password login while preserving the legacy user-ID, `New-Api-User`, and session-cookie flow. Legacy system tokens still require a user ID. No database migration is needed.

### v0.13.1 Chat Layout Refinements

- Rename the sidebar section to Conversation and its menu item to Chat; let the conversation fill the workspace with a transparent background.
- Place copy, edit, and delete actions below each message and remove redundant composer hints.
- Float the entire rounded composer above the bottom edge with responsive side spacing, while keeping the final message visible when scrolling. No database migration is needed.

### v0.13.0 Chat Playground

- A new Chat section opens the playground. Choose an enabled local key, its group and an available Chat Completions model; requests reuse the relay's routing, streaming and logs without exposing key material to the browser.
- Attach UTF-8 text, Markdown, CSV, JSON, extractable PDF text or PNG/JPEG/WebP/GIF images. The server checks type and size before passing content to the selected upstream.
- Optional web search retrieves up to five results from a fixed Bing RSS endpoint and supplies them as cited, untrusted reference context. The composer now sits directly below the conversation, and group, model and key selectors use searchable themed popovers. No database migration is needed.

### v0.12.2 Provider Verification And Session Renewal

- Discover the Sub2API site's public Turnstile requirements and let the user complete the challenge in the browser. The one-time proof is forwarded to upstream login and is not stored in configuration or logs. The upstream site key must allow the console hostname; upstream remains responsible for verification.
- Reuse encrypted login sessions across testing, saving, statistics, key/group reads and imports. Renew expired access tokens; prompt for a new challenge when an upstream session cannot be renewed and verification is required.
- Restore password login for older configurations without saved session tokens when the site does not require Turnstile. Retry failed widget scripts and recheck stale login tests before saving.
- Fix the official provider to 杂鱼AI, `https://www.zayuapi.com`, and Sub2API while keeping credentials editable.

### v0.12.1 Official Provider Login Fix

- Edit the official provider’s account/password or tokens, test the login, save credentials and refresh statistics; saved configuration persists across restarts.
- Keep the official name and `https://www.zayuapi.com` URL fixed in both the form and API. Other providers still allow name and URL changes.
- Offer a clear login-configuration action before authentication; keep the official entry pinned and protected from deletion.

### v0.12.0 Service Providers And Per-Key Policies

- Adds provider cards, the pinned official entry, bound account imports, and upstream key creation with automatic selection of a uniquely identified new key.
- Adds multiline group introductions and moves specific/default instruction policies to individual client keys, filtered by the group's available actual models.
- The first upgraded startup migrates legacy group policies to existing keys only where corresponding key fields were absent. Explicit empty lists and false switches remain unchanged; exactly recognized legacy bundled presets are retained, while arbitrary custom instructions are not activated. Migration runs once; new keys start empty with the default switch off.
- Providers, encrypted credentials and bindings are retained. Back up the full `/data` directory and `master.key` before upgrading; a downgrade must restore the matching pre-upgrade backup.
- Preserves Sakura Pudding styling while improving rounded mobile controls, dialog scrolling and content-area loading during sidebar navigation.

### v0.11.1 Cyber Detection And Key Display Fixes

Cyber detection now checks only assistant output text and refusal text. The previous detector scanned the entire response, so echoed `instructions`, `input`, `metadata`, `model`, or `id` could incorrectly mark a request as Cyber. Tool arguments, reasoning content, errors, and metadata no longer participate in detection; matching also works across streaming deltas and network chunks.

The fix applies to new requests after upgrading and reuses the existing database without a migration. Historical logs do not store response bodies, so their classifications cannot be reliably recalculated; existing flags and statistics are retained rather than automatically cleared.

Key management now displays `sk-zako-8f2a078••••••••bba6`, keeping the prefix and final four characters with eight bullets between them. The redundant trailing mask is removed; the copy button still copies the complete key.

### v0.11.0 GLM / Grok Lightweight Profiles And Account Scheduling

Six opt-in AI-Jailbreak API profiles cover `glm-5.3`, `glm-5.3-flash`, `glm-5.3-air`, `grok-4.7`, `grok-4.6`, and `grok-build`. Existing GLM-5.3 / Grok-4.7 coldcoffee profiles and group selections remain available. Variants targeting the same exact model are mutually exclusive. The catalog now contains 42 profiles for 41 unique model IDs; lightweight entries expose provenance, instruction size, and an unverified status. Smaller text and structural scores are not evidence of better model behavior.

The dashboard data-export button and its export implementation are removed; Request Logs JSON export remains available. Cooldown uses the candidate pool eligible for the current request's group, protocol, and actual model: a single candidate never enters cooldown, and a fully cooled pool has its cooldown cleared before polling again. Each request still attempts at most `max_retries + 1` distinct accounts, with unchanged RPM and concurrency limits. Existing data and strategy selections are retained without a database migration.

### v0.10.0 Sakura Pudding Theme

The console has been redesigned in an anime-inspired "Sakura Pudding" theme: sakura pink, warm honey yellow and cream white, with larger corner radii on cards, buttons, inputs and dialogs. The sidebar and top bar now float as cards, settings and request logs use card layouts, charts use soft gradient fills, and a small "Zako-chan" fish illustration appears on the sign-in page and in empty states. Latin UI text now uses Nunito (OFL-1.1, bundled locally with the frontend) instead of Inter; Chinese text keeps the system fonts. This release only changes the frontend appearance; APIs, data and deployment are unchanged and no upgrade steps are required.

### v0.9.0 Turnstile Security and Log Statistics Improvements

System settings can configure Cloudflare Turnstile site and secret keys for login challenges; the secret is encrypted at rest and never returned by the API. The request-log paginator now supports keyboard navigation and touch-friendly controls, dashboard counts use compact formatting for large values, and coverage includes Cyber statistics beyond one thousand events.

### v0.9.3 Turnstile `.env` Configuration and Release Synchronization

The Turnstile site and secret keys are stored together in the project root `.env`; saving System Settings updates that file and removes obsolete database values. System Settings includes a background connectivity test with top-right toast feedback. GitHub source updates follow the latest stable Release, including synchronizing a local version that is newer than that Release.

### v0.8.0 DeepSeek API Compatibility Profiles

A shared API adapter is now composed into all five exact DeepSeek model profiles, adapting dsh-purge's request-time prompt composition and context-continuity concepts to the relay. Profile selection still uses the mapped actual model and the existing Responses / Compact, Chat Completions, and Anthropic injection paths. Existing profile IDs and legacy preset recognition remain intact, and the catalog count is unchanged. The adapter uses only messages, tools, and results present in the current request; it assumes no Harness-local files, hooks, approvals, sandbox controls, cross-request memory, or permissions, and does not claim to change upstream model policy. See [profiles/dsh-purge/SOURCE.json](profiles/dsh-purge/SOURCE.json) and [third_party/dsh-purge/NOTICE.md](third_party/dsh-purge/NOTICE.md) for provenance, adaptation scope, and the MIT notice.

### v0.7.0 Request Logs and Request-Path Fixes

A dedicated Request Logs page now lives under API keys. It retains the latest 7 days by default, shows 20 entries per page, and supports page selection, JSON export, clearing all logs, status codes, structured JSON/SSE error details, first-token latency, and cache read/write token counts. The dashboard no longer duplicates the log view. Responses and Chat streaming tool calls release concurrency as soon as a terminal event is confirmed; normal length-limit and content-filter incomplete results do not count as account failures. Account cooldowns return 503 with the actual remaining delay, while local RPM and concurrency limits remain 429 with machine-readable reasons. Actual upstream HTTP status, sanitized error body, and Retry-After continue to pass through.

### Bundled Profile Models

The default strategy adapts task-execution and continuation mechanisms from the existing `gpt-6-astra` profile for multiple models and actual API capabilities. [profiles/default.md](profiles/default.md) is 3,844 UTF-8 bytes; [profiles/default.SOURCE.json](profiles/default.SOURCE.json) records the source and adaptations. Fixed model identity, local-tool assumptions, and mandatory four-artifact output are removed while client context and actual tool definitions remain. When enabled, any eligible actual model without a selected specific match can use it, including models outside the catalog. Disable the switch if it disrupts requests. Client-key `PUT` omission preserves the value, explicit false disables it, and `null` or non-boolean values are rejected.

| Family | Exact actual model IDs |
| --- | --- |
| GPT (retained) | `gpt-5.6-sol`, `gpt-6.1-sol`, `gpt-6-astra` |
| Claude | `claude-fable-5`, `claude-fable-5-1`, `claude-haiku-4-5`, `claude-haiku-5-5`, `claude-opus-4-5`, `claude-opus-4-6`, `claude-opus-4-7`, `claude-opus-4-8`, `claude-opus-5`, `claude-opus-5-5`, `claude-sonnet-4-5`, `claude-sonnet-4-6`, `claude-sonnet-5`, `claude-sonnet-5-5` |
| Grok | `grok-4.7`, `grok-4.6`, `grok-build` |
| Gemini | `gemini-3-flash`, `gemini-3.1-flash-image`, `gemini-3.1-pro`, `gemini-3.1-pro-preview`, `gemini-3.6-flash`, `gemini-3.7-flash`, `gemini-3.8-flash` |
| DeepSeek | `deepseek-v4-flash`, `deepseek-v4-flash-0731`, `deepseek-v4-pro`, `deepseek-v4-pro-0813`, `deepseek-v4.1-flash` / `deepseek-4.1-flash` / `deepseek-flash` (`deepseek-v4.1-flash` and `deepseek-4.1-flash` share the historical profile; `deepseek-flash` has a separate Hanshuang profile) |
| GLM | `glm-5.3`, `glm-5.3-flash`, `glm-5.3-air` |
| Kimi | `kimi-k3`, `kimi-k2.7`, `kimi-k2.7-code`, `kimi-k2.6` |

Specific profiles match exact models without family wildcards. Hanshuang retains the persisted ID `coldcoffee-deepseek-v4.1-flash-v1` so existing selections receive the updated text; it matches `deepseek-v4.1-flash` and `deepseek-4.1-flash`. The separate `hanshuang-deepseek-flash-v1` profile matches `deepseek-flash` and reuses the same instruction. Other DeepSeek profiles are unchanged. This catalog lists profile choices, not guaranteed model availability. Use the models and protocols actually provided by the relay. Imported coldcoffee, Hanshuang, Kimi, and AI-Jailbreak content is inline request text; it does not install local Skills, tools, or client configuration, or fabricate tool results. Verify actual model behavior with your provider.

The six lightweight AI-Jailbreak profiles use `aijailbreak-<exact-model-id>-v1`. Their GLM and Grok instructions contain 2,114 and 2,014 UTF-8 bytes respectively, not token counts. These are curated English API adaptations of technical delivery, concise output, explicit uncertainty, and context-grounded continuation; they follow the client's requested language and are not verbatim upstream prompts. Existing `coldcoffee-glm-5.3-v1` and `coldcoffee-grok-4.7-v1` text is unchanged, and no client key automatically switches variants. Optional `metadata` includes the source URL, pinned revision, `lightweight-api` variant, and `unverified` validation status. Local tests cover routing, exclusivity, and client-request preservation; model effectiveness has not been measured.

Other families retain their current profiles after review. The DeepSeek candidate forces rewritten-request output and a stable first-turn structure, which can alter JSON or tool workflows. GPT, Claude, Gemini, and Kimi candidates lack evidence for the existing exact model versions, and the universal candidate has not demonstrated an improvement over the current default. [profiles/ai-jailbreak/SOURCE.json](profiles/ai-jailbreak/SOURCE.json) records these decisions.

Eligible upstream accounts must match the client's group and protocol, and resolve the requested model to a selected actual model; accounts with no selected models do not receive requests. **Lower priority numbers are attempted first**; equal priorities use a stable account-ID order. Each client request uses at most `max_retries + 1` distinct upstream accounts. Every attempt starts from the original client request, applies that account's single-hop mapping, and then selects the authenticated client key's profile for its actual model. Failover resolves each account independently and never carries over or stacks a previous attempt's strategy. Transport failures, HTTP 401, 403, 408, 429, and HTTP 5xx permit failover before a response starts. Cooldown applies only when multiple matching accounts are available; a single-account pool never freezes. When every account eligible for a request is cooling down, the scheduler clears cooldown for that pool and polls it again; if all accounts fail again, the same cycle repeats. Other 4xx responses are returned without failover. Active streaming responses are not replayed if they fail midway. RPM windows, concurrency counters, and cooldown state are process-local and reset on restart; run one instance per data volume.

The security entry moves the console from `/` to `/<entry>` while leaving `/health` and `/v1/*` unchanged. It supplements the login, not an independent authentication factor. Save the new URL before leaving the page.

## Client API

Supported endpoints are `GET /v1/models`, `POST /v1/responses`, `POST /v1/responses/compact`, `POST /v1/chat/completions`, and `POST /v1/messages`. `/v1/models` includes selected actual models and valid requested-model aliases from accounts in the key's group. Authentication uses a generated proxy key via `Authorization: Bearer` or `x-api-key`. Client and upstream account use the same protocol; the proxy does not convert API protocols or add provider-native interfaces such as Google's.

For Chat Completions clients, use this site's origin plus `/v1` as the base URL and enable the account's `chat` protocol. `/v1/chat/completions` preserves existing messages, tool fields, and SSE. The strategy selected by the specific-first/default-fallback rules is prepended as a separate `system` message, preserving original client messages and other request fields. JSON and SSE responses retain the upstream Chat Completions format.

For Anthropic clients, use this site's origin as the base URL and select models on accounts with `anthropic` enabled. `/v1/messages` preserves native messages, tool calls, and SSE, forwards `anthropic-version` (default `2023-06-01`) and `anthropic-beta`, and prepends the strategy selected by the same client-key policy rules to `system` while preserving strings or text block arrays. Mapping, failover, limits, and logging use the same routing rules. Total input usage includes Anthropic cache reads and writes; cached usage counts reads.

```sh
export ZAKO_API_KEY='YOUR_GENERATED_PROXY_KEY'
curl http://localhost:1990/v1/responses \
  -H "Authorization: Bearer $ZAKO_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"model":"YOUR_REQUEST_MODEL_ID","input":"Hello"}'

curl -N http://localhost:1990/v1/responses \
  -H "Authorization: Bearer $ZAKO_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"model":"YOUR_REQUEST_MODEL_ID","input":"Hello","stream":true}'
```

`YOUR_REQUEST_MODEL_ID` may be a selected actual model or an account mapping alias. Compaction uses `/v1/responses/compact` and depends on provider support. Responses and Compact resolve the actual model, prefer the matching exact profile selected on the authenticated client key, and otherwise use the default only when that key's `default_strategy_enabled` is true. The selected text is prepended to `instructions` while client instructions and other fields are preserved. With the switch off and no specific match, no strategy is injected. Chat Completions and Anthropic Messages use the same selection order through a separate `system` message and the `system` field respectively. SSE events pass through while usage and completion are observed; reflected credentials are redacted. Profiles do not guarantee priority over upstream system instructions, changed behavior, or refusal bypass.

## Statistics And Storage

- Dashboard: input/output/cached/uncached tokens, cache hit rate, RPM, TPM, request count, success rate, average duration, daily charts, date-range history, and Cyber marker count.
- Token totals, Token Distribution, and related compact displays use `K` (thousand), `M` (million), `B` (billion), and `T` (trillion), with exactly two decimals such as `90.48M` or `1.00B`. Values below one thousand remain integers. This is presentation only: API usage values remain raw integers.
- Cached tokens are a subset of input tokens; total tokens are input plus output, without double counting cache. Token usage depends on upstream usage fields, including a completed SSE event. Missing usage remains zero; it is not estimated.
- RPM/TPM cover the most recent 60 seconds of completed/logged requests. Daily buckets use UTC. Totals and per-key usage cover retained request metadata, not an immutable lifetime billing ledger. Default retention is 90 days; expired rows are pruned as new requests are logged.
- Cyber count is the number of responses containing the case-insensitive `cyber` marker, at most once per response. It is a text heuristic, not proof of a specific popup, refusal, or model behavior.
- `/data/zako.db` stores configuration, hashed sessions, and request metadata. Upstream/client key values are encrypted using AES-256-GCM with `/data/master.key`; administrator passwords are Argon2 hashes. Prompts and response text are not persisted in request logs. SQLite files and other metadata are not whole-disk encrypted.

### Backup And Restore

Back up the **entire `/data` directory, including `master.key`**, to restricted storage. A consistent portable backup can be made while stopped:

```sh
docker compose stop
docker compose cp proxy:/data ./zako-backup
docker compose start
```

Restore the contents of that directory to the volume's `/data` root while the service is stopped, retaining ownership `10001:10001`. Restore `zako.db`, any SQLite sidecar files, and `master.key` from the same backup. A database without its matching master key cannot decrypt stored API keys. Verify `/health`, administrator login, and a small upstream request after restoration. Do not commit backups to Git.

## Local Development And CI

Requires Rust **1.99**, Node.js **22**, and npm. The SQLite library is bundled in the Rust build.

```sh
cd web
npm ci
npm run build
cd ..
cargo run --locked
```

The server defaults to `0.0.0.0:1990`, `DATA_DIR=data`, and `WEB_DIR=web/dist`. Use `BIND_ADDR=127.0.0.1:1990` for a loopback-only local server. `RUST_LOG` controls tracing verbosity.

GitHub Actions contains four checks: **Rust Quality** (`cargo fmt`, strict Clippy), **Backend Tests** (unit/integration tests with mock upstreams), **Frontend** (clean install and TypeScript/production build), and **Docker** (Compose validation, image build, nonroot HTTP smoke test, and persisted setup after restart). Docker runs after the other three checks, then publishes the tested image on default-branch pushes. CI uses generated ephemeral credentials, not a live provider key. Passing checks establishes covered behavior, not an absence of all possible bugs.

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
```

## License And Third-Party Notices

Frontend dependency notices, including Nunito's SIL Open Font License, are collected from the locked packages during each build. The resulting `assets/third-party-licenses.txt` is served by the app and included in the container at `/app/licenses/frontend-notices.txt`. Third-party components retain their own license terms.

This project is released under the [MIT License](LICENSE). The bundled instruction profiles include text adapted from [MDX-Tom/gpt-instruct](https://github.com/MDX-Tom/gpt-instruct), also MIT licensed. Its required copyright and license notice is retained in [third_party/gpt-instruct.LICENSE](third_party/gpt-instruct.LICENSE). Profiles are imported configuration data, not upstream application source or a model implementation. Retain both MIT notices when redistributing source or containers.

The original 28 extended Claude, Grok, Gemini, DeepSeek, and GLM profiles reference the coldcoffee project [3641397194-wq/gpt6-Astra](https://github.com/3641397194-wq/gpt6-Astra) at [09c7799a1fc2ef5f6f6cf4223b7a48abb39d2732](https://github.com/3641397194-wq/gpt6-Astra/tree/09c7799a1fc2ef5f6f6cf4223b7a48abb39d2732). Version 0.6.0 retains 27 texts and switches DeepSeek 4.1 Flash to Hanshuang below. Thanks to the author and contributors.

- **Copyright and license:** Copyright (c) 2026 3641397194-wq, MIT License. The original notice is retained in [third_party/coldcoffee.LICENSE](third_party/coldcoffee.LICENSE) and shipped at `/app/licenses/coldcoffee.LICENSE` in the container.
- **Adaptation:** Claude, Grok, Gemini, DeepSeek, and GLM client seat instructions and Skill text are adapted into per-model inline API request profiles. The upstream desktop installer, private relay workflows, and local Skill execution environment are not included.
- **Scope:** attribution does not imply upstream endorsement or guarantee refusal bypass, tool availability, or model capabilities. Preserve this project's license and these third-party notices when redistributing source or containers.

The DeepSeek 4.1 Flash Hanshuang profile references [aimeoa/hanshuang-codex](https://github.com/aimeoa/hanshuang-codex) at [cf9c1537c2bd4ed8577ed6509ee665a39709d7d4](https://github.com/aimeoa/hanshuang-codex/tree/cf9c1537c2bd4ed8577ed6509ee665a39709d7d4). Its `memory/README.md` and `install-dsh.ps1` select the shared `memory/破甲记忆.md`; no separate DeepSeek-specific source exists. This project adapts selected goal binding, direct execution, real evidence, recovery, continuation, and verification mechanisms to actual API capabilities. Attack examples, blanket authorization assumptions, installers, and invented tool environments are excluded. The runtime text is 4,748 UTF-8 bytes.

- **Provenance:** [profiles/hanshuang/SOURCE.json](profiles/hanshuang/SOURCE.json) records hashes, selected line ranges, and scope; [third_party/hanshuang](third_party/hanshuang) retains excerpts and notices. `python scripts/import-hanshuang.py --check` verifies offline; omit `--check` to regenerate.
- **License facts:** the upstream README declares MIT, but its linked root `LICENSE` is absent at that revision. No license text or copyright owner is invented, and separate `seep-kit` licenses are not treated as the prompt license. See [third_party/hanshuang/NOTICE.md](third_party/hanshuang/NOTICE.md), also shipped at `/app/licenses/hanshuang.NOTICE.md`.

The 27 profiles still using coldcoffee retain its seat text, workflow, and full routing skills, adding approximately 64–66 KB of UTF-8 instructions per match and consuming context/input tokens. [profiles/coldcoffee/SOURCE.json](profiles/coldcoffee/SOURCE.json) records historical provenance, hashes, and adaptations. Reproduce with `python scripts/import-coldcoffee.py UPSTREAM_CHECKOUT` against the pinned clean checkout; the importer reads data without executing installers or Skills.

The four Kimi profiles reference and adapt [YD-233/kimi-break](https://github.com/YD-233/kimi-break) at pinned commit [`c6a7f6dfffd17c45e60f6eb965f1d4bc169cd8f6`](https://github.com/YD-233/kimi-break/tree/c6a7f6dfffd17c45e60f6eb965f1d4bc169cd8f6). Thanks to its author and contributors.

- **Copyright and license:** Copyright (c) 2026 YD-233, MIT License. The complete notice and original `README.md` and `SYSTEM.md` are retained in [third_party/kimi-break](third_party/kimi-break); containers ship `/app/licenses/kimi-break.LICENSE`.
- **API adaptation:** upstream persona, behavioral strategy, coding guidance, and user-style text are retained. Kimi Code CLI template variables and environment/project sections use actual API request context without assuming local tools, paths, Skills, or plugins. The four exact models are selected separately and use the existing OpenAI / Anthropic compatible injection paths. Each match adds 13,264 bytes of UTF-8 instructions.
- **Reproduction and scope:** [profiles/kimi-break/SOURCE.json](profiles/kimi-break/SOURCE.json) pins source and generated SHA-256 hashes. Run `python scripts/import-kimi-break.py --check` for fully offline verification, or omit `--check` to regenerate. A clean upstream checkout at the pinned commit can also be supplied. The importer only reads text data. Profile IDs do not guarantee relay model availability; local tests verify injection and request preservation, while actual model behavior depends on the provider.

The six GLM / Grok lightweight candidates reference [Baimuking/AI-Jailbreak](https://github.com/Baimuking/AI-Jailbreak) at pinned commit [`8c278b10f47f305546a52d7a0ce044d2c643dc71`](https://github.com/Baimuking/AI-Jailbreak/tree/8c278b10f47f305546a52d7a0ce044d2c643dc71).

- **Copyright and license:** Copyright (c) 2026 Baimuking, MIT License. Original reference text, license, and adaptation notice are retained in [third_party/ai-jailbreak](third_party/ai-jailbreak); containers ship `/app/licenses/ai-jailbreak.LICENSE` and `/app/licenses/ai-jailbreak.NOTICE.md`.
- **API adaptation:** direct delivery, complete artifacts, explicit uncertainty, and continuation concepts use actual request context and tools. Fixed local identity, blanket authorization, instruction-hierarchy overrides, unconditional phrase bans, and assumed cross-request memory are omitted. No upstream CLI, installer, or agent configuration is executed or installed.
- **Provenance and reproduction:** [profiles/ai-jailbreak/SOURCE.json](profiles/ai-jailbreak/SOURCE.json) records source and output SHA-256 hashes, byte counts, and assessment decisions. `python scripts/import-ai-jailbreak.py --check` verifies everything offline; omit `--check` to regenerate. Both upstream cards and these adaptations remain unverified for model effectiveness. Structural scores and protocol tests are not model-success measurements.
