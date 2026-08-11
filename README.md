# Luxa

Luxa is a compact 3D rendering engine for Rust, built on
[`wgpu`](https://wgpu.rs/) and designed for native and WebAssembly applications. It provides a
scene graph, glTF loading, physically based rendering, image-based lighting and engine-owned GPU
resources behind a small handle-based API.

The project favours a readable, complete rendering path over production-engine complexity. Luxa
owns the WebGPU resource graph while applications own their window, event loop, input and platform
integration.

[Try the WebGPU viewer](https://benc-uk.github.io/luxa/)

> [!NOTE]
> Luxa is under active development. The crate is currently version `0.0.1`, is not published to
> crates.io and does not yet promise a stable public API.

## Features

### Rendering

- Metallic-roughness PBR with direct and image-based lighting.
- Base colour, metallic-roughness, normal, occlusion and emissive texture maps.
- Opaque, alpha-masked and alpha-blended materials, including double-sided rendering.
- HDR equirectangular environment loading with GPU-baked irradiance and prefiltered cubemaps.
- Selectable environment, irradiance and prefiltered skybox views with mip-level control.
- Depth buffering, normal transforms and sRGB-correct surface and texture handling.
- Up to 16 positional lights with colour and intensity.

### Scenes and assets

- Hierarchical scene graph with parent-child transform propagation.
- Multiple independent scenes addressed through opaque handles.
- Mesh, camera, light and transform-only nodes.
- Node transforms using `glam` vectors, matrices and quaternions.
- Per-mesh and per-node axis-aligned bounding boxes.
- glTF 2.0 `.gltf` and `.glb` loading, including embedded or external buffers and images.
- glTF metallic-roughness materials, texture channels, alpha modes and double-sided materials.
- Procedural cube and UV-sphere meshes through `MeshBuilder`.

### Engine integration

- Native surface creation through `Engine::new`.
- Browser canvas creation through `Engine::new_from_canvas`.
- Opaque handles for scenes, nodes, meshes, materials and textures.
- All `wgpu` devices, queues, surfaces, pipelines, bind groups and buffers remain internal.
- Platform-independent frame timing through `web-time`.
- `anyhow` errors and `log`-based runtime diagnostics.

## Architecture

The workspace contains two active crates:

| Path                         | Purpose                                                                                         |
| ---------------------------- | ----------------------------------------------------------------------------------------------- |
| [`luxa/`](luxa/)             | Reusable engine library and WGSL shaders. Owns rendering, scenes and GPU resources.             |
| [`web-viewer/`](web-viewer/) | WebAssembly reference application. Owns the DOM, asset fetching, input and browser render loop. |

Applications interact with the engine through `Engine` and typed slot-map handles. A typical frame
follows this path:

```text
application input
       |
       v
node and scene updates
       |
       v
Engine::update -> Engine::render
       |
       +-> scene traversal and world transforms
       +-> camera, light and material uploads
       +-> opaque pass -> skybox -> blended pass
       v
WebGPU surface
```

The public API exports:

- `Engine` and `SkyboxMode`.
- Handles: `SceneHandle`, `NodeHandle`, `CameraHandle`, `LightHandle`, `MeshHandle`,
  `MaterialHandle` and `TextureHandle`.
- Descriptors: `SceneDescriptor`, `NodeDescriptor`, `CameraDescriptor`, `LightDescriptor`,
  `MeshNodeDescriptor`, `ModelDescriptor` and `MaterialDescriptor`.
- `Node`, `Transform`, `CameraOrientation`, `Mesh`, `MeshBuilder`, `Material`, `Vertex` and
  `AlphaMode`.
- `Aabb`, `Color` and `Size`.

## Getting started

### Requirements

- A current stable Rust toolchain.
- A graphics adapter and driver supported by `wgpu`.
- For the viewer: the `wasm32-unknown-unknown` target, `wasm-pack`, Node.js and a browser with
  WebGPU enabled.

Clone the repository and check the engine:

```sh
git clone https://github.com/benc-uk/luxa.git
cd luxa
cargo check -p luxa
```

To use the source repository directly from another Cargo project:

```toml
[dependencies]
luxa = { git = "https://github.com/benc-uk/luxa" }
glam = "0.33"
```

`Engine::new` accepts a value convertible to `wgpu::SurfaceTarget<'static>`, such as an owned
`winit` window. The application does not otherwise need access to Luxa's internal `wgpu` objects.

## Core API flow

The following shows the engine-side flow after an application has created a surface target and
chosen its initial dimensions:

```rust
use glam::Vec3;
use luxa::{CameraDescriptor, CameraOrientation, Engine, ModelDescriptor, SceneDescriptor};

let mut engine = Engine::new(surface_target, (width, height)).await?;

let scene = engine.create_scene(SceneDescriptor {
  background_color: [0.02, 0.02, 0.03],
  ..Default::default()
});

let camera = engine.create_camera(
  scene,
  CameraDescriptor {
    position: Vec3::new(0.0, 1.0, 4.0),
    orientation: CameraOrientation::LookAt { target: Vec3::ZERO, up: Vec3::Y },
    fov_degrees: 60.0,
    far_plane: 200.0,
    ..Default::default()
  },
)?;

let model = engine.load_model(scene, "assets/model.glb", ModelDescriptor::default())?;

// Once per frame:
engine.node_mut(camera)?.set_position(orbit_position);
engine.render(camera)?;
```

Use `load_model_bytes` when asset bytes have already been fetched, as in a browser. Call `resize`
when the target dimensions change. HDR bytes can be passed to `set_environment`, then IBL can be
enabled on individual scenes with `scene_mut(...)?.set_ibl_enabled(true)`.

The [`web-viewer`](web-viewer/) is the complete reference for canvas setup, asynchronous asset
loading, orbit controls and `requestAnimationFrame` integration.

## Building meshes

`MeshBuilder` assembles geometry and an optional material. It holds no engine reference, so you
build it standalone and hand it to `Engine::create_mesh`, which validates the geometry, fills in the
default material when none was set, and returns a `MeshHandle`. Attach the mesh to a scene with
`create_mesh_node`.

### Primitive meshes

```rust
use glam::Vec3;
use luxa::{MaterialDescriptor, MeshBuilder, MeshNodeDescriptor};

// A unit cube using the engine's default material.
let cube = engine.create_mesh(MeshBuilder::new().cube())?;

// A red-ish, mostly-rough material.
let red = engine.create_material(MaterialDescriptor {
  base_color_factor: [0.8, 0.1, 0.1, 1.0],
  roughness_factor: 0.4,
  ..Default::default()
})?;

// A smooth UV sphere using that material.
let sphere = engine.create_mesh(MeshBuilder::new().uv_sphere(32, 16).material(red))?;

// Place the sphere in the scene, one unit up.
engine.create_mesh_node(
  scene,
  MeshNodeDescriptor {
    position: Vec3::new(0.0, 1.0, 0.0),
    meshes: vec![sphere],
    ..Default::default()
  },
)?;
```

`cube()` and `uv_sphere(slices, stacks)` are chainable and each appends to the geometry already in
the builder, so one mesh can combine several primitives. `uv_sphere` clamps `slices` to at least 3
and `stacks` to at least 2 rather than erroring.

### Custom geometry

Supply your own vertices and indices. A `Vertex` carries a position, texture coordinate, normal and
tangent (the `w` of the tangent is the handedness sign):

```rust
use luxa::{MeshBuilder, Vertex};

let triangle = engine.create_mesh(
  MeshBuilder::new()
    .vertices([
      Vertex { position: [-0.5, -0.5, 0.0], tex_coord: [0.0, 1.0], normal: [0.0, 0.0, 1.0], tangent: [1.0, 0.0, 0.0, 1.0] },
      Vertex { position: [0.5, -0.5, 0.0], tex_coord: [1.0, 1.0], normal: [0.0, 0.0, 1.0], tangent: [1.0, 0.0, 0.0, 1.0] },
      Vertex { position: [0.0, 0.5, 0.0], tex_coord: [0.5, 0.0], normal: [0.0, 0.0, 1.0], tangent: [1.0, 0.0, 0.0, 1.0] },
    ])
    .indices([0, 1, 2]),
)?;
```

Indices are `u16`, so a single mesh is limited to `u16::MAX + 1` (65536) vertices. `create_mesh`
returns an error for empty geometry, out-of-range indices, an invalid material handle, or too many
vertices.

## Run the web viewer

Install the WebAssembly target and `wasm-pack` once:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
```

Build and serve the viewer:

```sh
cd web-viewer
make build
make serve
```

Open <http://localhost:8000>. The viewer supports mouse, pen and touch orbit controls, wheel or
pinch zoom, model switching and HDR environment switching.

Useful viewer commands:

| Command        | Purpose                                                     |
| -------------- | ----------------------------------------------------------- |
| `make build`   | Development WebAssembly build.                              |
| `make release` | Optimised WebAssembly build.                                |
| `make serve`   | Start the Vite development server on port 8000.             |
| `make bundle`  | Build the release WebAssembly package and production site.  |
| `make check`   | Check the viewer for `wasm32-unknown-unknown`.              |
| `make clippy`  | Run Clippy for the WebAssembly target with warnings denied. |

## Current scope

Luxa is functional, but intentionally smaller than a general-purpose production engine. Current
constraints include:

- glTF import selects the default or first scene, supports triangle-list primitives and samples
  `TEXCOORD_0`. Imported node transforms are flattened into the generated mesh data.
- `Mesh` and `Vertex` are exported, but direct insertion of a custom mesh into the engine resource
  store is not yet part of the public API.
- Alpha-blended meshes render after opaque geometry but are not yet sorted by camera depth.
- Lighting is limited to 16 positional lights per scene.
- There is no frustum culling, instancing, level-of-detail system, skeletal animation or general
  animation system.
- Exposure is currently fixed in the shader; configurable tone mapping is on the roadmap.
- The repository currently has no automated unit or integration test suite. The WebAssembly viewer
  is the primary integration and visual test application.
- Native rendering is supported by the library API, but this repository does not currently include
  a native example application.

These limits keep the implementation approachable while leaving clear extension points in the
scene, resource and pipeline layers.

## Development

Run checks from the workspace root:

```sh
cargo check --workspace
cargo check -p web-viewer --target wasm32-unknown-unknown
cargo fmt --all -- --check
```

The workspace uses Rust edition 2024, 2-space Rust formatting and `glam` for maths. `wgpu` is pinned
to `29.0` because of [gfx-rs/wgpu#9855](https://github.com/gfx-rs/wgpu/issues/9855); check that issue
before updating the dependency.

## Licence

Luxa is available under the [MIT Licence](luxa/LICENSE).
