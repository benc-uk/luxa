# wgpu-learning

A Rust workspace for learning `wgpu` (WebGPU). This is a learning project, not AAA or production code, so favour simple, readable designs over maximum performance or completeness. Flag when a simplification has real limits, but do not over-engineer.

## Workspace layout

Three crates, each with a distinct role. Do not blur them together.

- **`wgpu-engine/`** — the actual reusable 3D engine (library crate). This is where the real work happens. Its whole point is to hide the raw `wgpu` graph behind a small, ergonomic public API.
- **`harness/`** — a test app (binary) that consumes the engine. It should use **only** the engine's public API (whatever `wgpu-engine/src/lib.rs` re-exports); never reach into engine internals or depend on `wgpu` types directly. If the harness needs something the public API can't express, that's a signal to improve the engine's API, not to bypass it.
- **`cube/`** — a standalone, self-contained experiment. Older and independent; it does **not** follow the engine's conventions and does not use `wgpu-engine`. Do not refactor it to match the engine unless explicitly asked.

## Shared conventions (all crates)

- Rust edition 2024.
- Formatting: 2-space indent, `max_width = 180` (see `rustfmt.toml`). Run `cargo fmt`; do not hand-format against these settings.
- Maths: use `glam` (`Vec3`, `Mat4`, `Quat`). Do not roll your own vector/matrix maths.
- Errors: fallible constructors and IO return `anyhow::Result<T>`; use `?` and `anyhow::bail!`.
- Logging: use the `log` crate macros (`log::info!`, `log::debug!`), never `println!`.
- Timing: use `web-time::Instant`, not `std::time::Instant`. `wgpu-engine` and `cube` target `wasm32-unknown-unknown` as well as desktop, so avoid non-wasm-safe std APIs (threads, blocking IO, `std::time`).
- `wgpu` is pinned (currently `29.0`) on purpose due to an upstream bug. Do not bump it without checking the note in the relevant `Cargo.toml`.
- Give every `wgpu` object a `label: Some("...")` for debuggability.
- Start each module file with a `// ===...` banner comment summarising its purpose, matching the existing files.
