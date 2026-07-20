# wgpu-learning

A Rust workspace for learning [wgpu](https://wgpu.rs/) (WebGPU) and modern GPU rendering, built up
from a minimal single-file cube into a small but reasonably complete 3D engine.

The goal is a **simple but complete WebGPU based engine** that hides the raw `wgpu` resource graph
behind a small, ergonomic public API, while keeping every crate readable end to end. This is a
learning project, not AAA or production code, so it favours clarity over maximum performance or
completeness.

## Crates

The workspace is a Cargo workspace with three crates, each with a distinct role.

### [luxa](luxa/) - the engine

`luxa` is the reusable 3D engine (a library crate) and the heart of the project. It wraps the whole
`wgpu` graph (instance, adapter, device, queue, surface, pipelines, bind groups, buffers, command
encoders) so consumers work with engine-owned resources and opaque handles instead of raw GPU
objects.

Features:

- **glTF 2.0 loading** of `.gltf` and `.glb` assets, including meshes, materials and textures. See
  the [glTF parser guide](luxa/PARSER.md) for the import flow, terminology, supported features and
  current limitations.
- **Scene graph** with a node hierarchy (`Node3D`), parent/child transforms and world-matrix
  propagation via depth-first traversal.
- **Multiple scenes**, each with its own root node, addressed by `SceneHandle`.
- **Node types** for meshes, cameras and lights, created through a small factory API
  (`create_mesh_node`, `create_camera_node`, `create_light_node`).
- **Multiple lights** (currently up to 16) with position, colour and intensity, gathered from the
  scene graph each frame.
- **PBR metallic-roughness shading**. See the [PBR notes](luxa/PBR.md).
- **Handle-based resources** (meshes, materials, textures, nodes, scenes) stored in slot maps, so
  the raw `wgpu` types never leak into the public API.
- **Cross-platform**: targets native desktop and `wasm32-unknown-unknown` for WebGPU-capable
  browsers.

### [harness](harness/) - the test app

A small binary that drives `luxa` through a `winit` window and event loop. It consumes **only** the
engine's public API, so it doubles as a worked example of how to set up an engine, build a scene,
load a glTF model, add lights and a camera, and render each frame. If the harness needs something
the public API cannot express, that is a signal to improve the engine, not to bypass it.

### [cube](cube/) - the learning exercise

A self-contained, texture-mapped 3D cube that runs on desktop and in the browser. It predates the
engine and does **not** use `luxa`; it stays as a from-scratch reference showing the rendering loop,
a thin `wgpu` helper layer, GPU setup, buffers, camera transforms, shader uniforms, textures, depth
testing and cross-platform startup. See the [cube README](cube/README.md) for its architecture and
build instructions.

## Building

Standard Cargo workflows apply from the workspace root:

```sh
cargo run -p harness      # run the engine test harness
cargo run -p cube         # run the standalone cube
cargo build               # build everything
```

See each crate's `Makefile` and README for WebAssembly build steps.

## Conventions

- Rust edition 2024, `glam` for maths, `anyhow` for errors, the `log` crate for logging.
- Formatting via `rustfmt` (2-space indent, `max_width = 180`); run `cargo fmt`.
- `wgpu` is pinned (currently `29.0`) due to an upstream bug. See the note in the relevant
  `Cargo.toml` before bumping it.

## Helpful references

- <https://3dviewer.net/>
- <https://sandbox.babylonjs.com/>
- <https://gltf-viewer.donmccurdy.com/>
- <https://github.khronos.org/glTF-Assets/>
- <https://github.khronos.org/glTF-Sample-Viewer-Release/>
