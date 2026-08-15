# Luxa workspace

This is a Rust workspace for learning `wgpu` and building a small, reusable 3D engine. Favour simple, readable designs over production-grade complexity, but call out limitations that affect correctness or the public API.

## Crate boundaries

The workspace has four crates with distinct roles:

- **`luxa/`** is the reusable engine library and owns the internal `wgpu` resource graph. Its public API is defined by the exports in `luxa/src/lib.rs`.
- **`harness/`** is the native desktop example and test application. It may use application-level crates such as `winit` and `glam`, but it must consume `luxa` through its public API and must not depend on `wgpu` or engine internals.
- **`web-viewer/`** is the WebAssembly browser viewer. Browser and DOM integration belongs here; rendering behaviour belongs in `luxa`. It must use the engine's public API rather than raw `wgpu` types.
- **`cube/`** is an older, standalone `wgpu` learning experiment. It does not use `luxa`; do not refactor it into the engine architecture unless explicitly asked.

If a consumer cannot express something through `luxa`'s public API, improve that API instead of exposing an internal GPU object or reaching into a private module.

## Working style

- Default to advice: inspect the relevant code, explain the local issue or design choice, propose a focused change and wait for approval before editing.
- Treat approval to implement a proposal as permission for the complete focused change, including formatting and validation. Do not repeatedly ask about routine implementation details.
- Read-only inspection and validation of the current state do not require approval.

## Conventions

- Rust edition 2024.
- Formatting is controlled by `rustfmt.toml`: 2-space indentation and `max_width = 180`. Run `cargo fmt` after Rust edits.
- Use `glam` for vector, matrix and quaternion maths.
- Follow each crate's existing error boundary. Engine and native fallible operations generally use `anyhow::Result<T>` and `?`; WebAssembly exports may translate failures to `JsValue` or browser logging.
- Use the `log` macros rather than `println!` for runtime diagnostics.
- Use `web-time` for timing in code that targets both native and WebAssembly. Do not introduce threads or blocking IO into WebAssembly code paths.
- `wgpu` is deliberately pinned to `29.0` in `luxa` and `cube` because of the upstream issue noted in their manifests. Do not change it without checking that issue.
- Give created `wgpu` objects meaningful `label: Some("...")` values.
- Preserve the existing module banner style when adding or substantially rewriting a module.

## Validation

- Run the narrowest relevant `cargo check -p <crate>` while iterating.
- NEVER run `cargo test` there are no tests in this workspace yet.
- NEVER check wasm
- For WebAssembly-specific changes, also check the affected crate with `--target wasm32-unknown-unknown`.
- Before finishing Rust changes, run `cargo fmt --all -- --check` and the relevant tests or workspace check.
