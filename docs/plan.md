# Plan

Turn this repository from a single demo binary into a publishable Rust library crate (with examples/bins) that supports Actix WebSocket broadcasting, optional Protobuf encode/decode, and optional Redis integration.

## Requirements
- Builds on stable Rust with current ecosystem crates (Actix Web 4, Tokio 1, Bytes 1, Prost current).
- Clear public API boundaries (library vs. example app).
- Optional integrations are feature-gated (e.g., `redis`, `protobuf`, `gcs-upload`).
- Reasonable defaults, explicit configuration, and good docs/examples.

## Scope
- In:
  - WebSocket connection handling, heartbeats, and broadcasting.
  - Broadcast “hub” actor/state (subscribe/unsubscribe + fanout).
  - Optional Redis pub/sub bridge.
  - Optional Protobuf message handling (generated from `.proto`).
  - Example server binary and example clients.
- Out:
  - Production-ready auth, metrics, tracing, rate limiting, and persistence (can be future work).
  - Full GCS SDK integration (keep upload as optional, pluggable, or move to separate crate).

## Current entry points (today)
- `Cargo.toml` defines only a binary target (`src/main.rs`).
- Reusable-ish modules exist but depend on app-specific types:
  - `src/websocket.rs`
  - `src/subscriptions.rs`
  - `src/multipart_raw.rs`

## Phased TODO list

### Phase 0 — Product definition (API + boundaries)
[ ] Decide the crate name and published surface (what “the library” is responsible for).
[ ] Write a 1–2 paragraph “crate mission” and target audience in `README.md`.
[ ] Define the primary public API shape (suggestion):
    - `Hub` (broadcast fanout)
    - `WsSession` (per-connection actor/handler)
    - optional `RedisBridge`
    - optional protobuf helpers/types
[ ] Identify which pieces are “examples only” (current `gsutil` upload, python scripts, etc.).

### Phase 1 — Restructure into a library + examples
[ ] Add `src/lib.rs` and move reusable modules under the library crate.
[ ] Move the current demo app to `src/bin/server.rs` (or `examples/server.rs`).
[ ] Refactor modules to remove `crate::{AppState, MyObj}` coupling:
    - make `Hub` generic over message type(s) or accept `bytes::Bytes`/`String` payloads
    - make session wiring accept injected state/config instead of reading globals
[ ] Add `examples/`:
    - `examples/server.rs` (minimal runnable example)
    - (optional) `examples/redis_bridge.rs`
[ ] Add `.gitignore` (at least `target/`) and decide `Cargo.lock` policy (typically don’t commit for libraries).

### Phase 2 — Modernize dependencies (make it build)
[ ] Upgrade to `edition = "2021"` (or newer) and set `rust-version`.
[ ] Upgrade core deps:
    - `actix-web = 4`, `actix = 0.13` (or current compatible)
    - `actix-web-actors` for websockets
    - `tokio = 1`, `futures-util`
    - `bytes = 1`
    - replace `failure` with `thiserror` (library) and `anyhow` (bin/examples)
[ ] Remove `*` version requirements and remove git dependencies for core functionality.
[ ] Ensure `cargo check` passes on stable.

### Phase 3 — Protobuf as a first-class (optional) feature
[ ] Move `.proto` sources to `proto/` (single canonical location).
[ ] Add `build.rs` using `prost-build` to generate Rust types.
[ ] Add `protobuf` cargo feature:
    - `default-features = false` for library; users opt in
    - library APIs that require protobuf live behind the feature gate
[ ] Add an example that:
    - accepts JSON, emits protobuf broadcast to WS clients (like today)
    - accepts protobuf POST, echoes/broadcasts

### Phase 4 — Redis as an optional async integration
[ ] Add `redis` feature and remove blocking `get_connection()` usage from request/WS hot paths.
[ ] Choose and standardize on an async Redis approach (e.g. `redis::aio::ConnectionManager` or `deadpool-redis`).
[ ] Implement `RedisBridge`:
    - subscribe to a topic and publish incoming WS messages (optional)
    - publish server-side events to a topic (optional)
[ ] Add an integration test strategy (feature-gated) or a documented manual test recipe.

### Phase 5 — Cleanups, docs, and release readiness
[ ] Add crate metadata: `description`, `license`, `repository`, `homepage`, `documentation`, `categories`, `keywords`.
[ ] Add minimal rustdoc examples for the main types (`Hub`, `WsSession`, etc.).
[ ] Add `CHANGELOG.md` and decide semver policy.
[ ] Add CI (format + clippy + test) and a minimal MSRV check (optional).
[ ] Publish a `0.x` release (or tag internally) once examples and docs are stable.

## Testing and validation
- `cargo fmt --all --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all --all-features`
- Manual smoke test:
  - run example server
  - connect a WS client
  - POST JSON and/or protobuf and observe broadcast
  - (if enabled) verify Redis pub/sub flow

## Risks and edge cases
- Large dependency jump (Actix 0.7 → 4) implies major rewrites; avoid “mechanical port” and re-check behavior.
- Mixing text + binary WS payloads: decide a consistent framing (separate endpoints vs. negotiated protocol).
- Backpressure: broadcasting to many clients needs a strategy (drop policy, bounded mailbox, slow consumer handling).
- Upload functionality via `gsutil` is not portable; keep it optional or move it out-of-crate.

## Open questions
- Should the library expose an Actix-actor-based API only, or also a framework-agnostic “hub” (channel-based) core?
- Do you want protobuf types to be user-provided (generic) or owned/generated by this crate?
- Preferred Redis behavior: best-effort fire-and-forget vs. at-least-once semantics?
