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

## wgpu-engine specifics

These rules apply to code under `wgpu-engine/` only (not `cube`).

### Public API surface

- The only public API is what `lib.rs` re-exports with `pub use` (`Engine`, `Material`, `Mesh`, `MeshBuilder`, `Vertex`, `Node3D`, `Node3DMesh`, `Size`). Every module is declared `mod`, never `pub mod`.
- When adding a consumer-facing type, add a matching `pub use` in `lib.rs`. If it is internal plumbing, do not.
- Consumers must never need to name a `wgpu::*` type to use the engine. If an API forces them to, that is a design smell, push the wgpu detail behind the engine.

### `Engine` owns the wgpu graph

- `Engine` owns `device`, `queue`, `surface`, `surf_config`, the render pipeline, the frame uniforms, the camera, the depth texture, and the texture cache. Other types borrow what they need through it.
- Types that need GPU access take `&Engine` in their constructor and pull `engine.get_device()` / `engine.get_queue()` from it, rather than taking `&wgpu::Device` directly. (`Camera` predates this and takes `&Device`; new types should follow the `&Engine` convention.)
- Textures are shared as `Arc<Texture>` and cached in `Engine` by path. Reuse the cache via `load_texture`; do not load the same file twice.

### GPU-backed type pattern

Every renderable/uniform-carrying type (`Material`, `Node3D`, `Camera`, frame uniforms) follows the same shape. Match it for consistency:

- A `#[repr(C)]` `...Uniform` struct deriving `bytemuck::Pod, bytemuck::Zeroable`. Pad explicitly to 16-byte alignment with a `_padding` field when the layout needs it (WGSL std140-style rules).
- The type owns its `bind_group`, `bind_group_layout`, `uniform`, `uniform_buffer`, and a `dirty: bool` flag.
- Setters mutate CPU state and set `self.dirty = true`. They do **not** touch the GPU.
- A `pub(crate) fn upload_gpu(&mut self, queue: &wgpu::Queue)` early-returns when `!dirty`, writes the buffer with `queue.write_buffer(...)`, then clears the flag. The render loop calls `upload_gpu` once per frame.
- A `pub(crate) fn get_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout` associated function builds the layout. `Engine` calls it when assembling the pipeline, so it must stay in sync with the shader's bindings.
- `get_bind_group(&self)` is `pub(crate)`.

### Visibility discipline

- Consumer-facing methods are `pub`. Everything the render loop or `Engine` needs but consumers should not see is `pub(crate)`: `get_device`, `get_bind_group`, `get_bind_group_layout`, `upload_gpu`, buffer accessors, etc.
- Do not widen visibility to `pub` just to make a call compile from another module. If it is engine internals, keep it `pub(crate)`.

### Put wgpu boilerplate in `wgpu_helper.rs`

- Verbose `wgpu` descriptor construction lives in `wgpu_helper.rs`: `init`, `create_pipeline`, `create_render_pass`, `create_depth_texture`, and the bind-group-layout entry constructors (`uniform_entry`, `texture_entry`, `sampler_entry`). Reuse these instead of hand-writing `wgpu::BindGroupLayoutEntry { ... }` inline.
- New repeated wgpu boilerplate belongs here too, keep the domain types (`Material`, `Node3D`, etc.) readable.

### Bind group layout scheme

Group indices are fixed across the pipeline and shader; keep them aligned (documented in `engine.rs`):

- group 0: frame (camera uniform @VERTEX, time uniform @VERTEX_FRAGMENT)
- group 1: material (uniform, texture, sampler @FRAGMENT)
- group 2: object/node (model matrix @VERTEX)
- group 3: lights (@FRAGMENT)

Shaders live in `shaders/*.wgsl`, are loaded with `include_str!`, and use hard-coded entry points `vert_main` / `frag_main`. Handle sRGB via `format.add_srgb_suffix()`.
