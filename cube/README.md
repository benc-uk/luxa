# Minimal wgpu Cube

A minimal but functional wgpu application that renders a continuously rotating, texture-mapped 3D
cube with depth testing and perspective projection. It runs from the same shared Rust code as a
native desktop application or as WebAssembly in a WebGPU-capable browser.

The project is deliberately small enough to read end to end, but includes the complete path needed
to bootstrap a real rendering loop rather than stopping at a blank window or hard-coded triangle.

## What the application demonstrates

| Area                  | Implementation                                                                                                               |
| --------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| Window and event loop | A `winit` `ApplicationHandler` creates the window or canvas, handles resize and close events, and drives continuous redraws. |
| wgpu initialisation   | Creates the instance, presentation surface, adapter, device, command queue and surface configuration.                        |
| Models and buffers    | Defines a cube model with 24 vertices and 36 indices, describes the vertex layout, and uploads vertex and index buffers.     |
| Camera and transforms | Uses `glam` to build a right-handed view matrix and WebGPU-compatible perspective projection.                                |
| Uniforms              | Stores the camera view-projection matrix in a uniform buffer and uploads it through the queue every frame.                   |
| Textures              | Decodes an embedded JPEG, uploads it to a GPU texture, and binds its view and sampler for the fragment shader.               |
| Pipeline              | Compiles WGSL shaders and configures vertex input, bind groups, colour output, back-face culling and depth testing.          |
| Render loop           | Acquires a surface texture, records a render pass, issues an indexed draw, submits commands and presents the frame.          |
| Resize handling       | Reconfigures the surface, recreates the depth texture and updates the camera aspect ratio.                                   |
| Cross-platform code   | Shares the renderer and application logic between desktop and WASM, with only startup details behind platform adapters.      |

The cube's model transform is the identity matrix. The camera orbits the origin, which gives the
visible rotation while demonstrating view and perspective transforms and the per-frame uniform
update path. The model data is defined in Rust rather than loaded from an OBJ or glTF file.

## The wgpu helper layer

`src/wgpu_helper.rs` is one of the project's core concepts. wgpu is intentionally explicit, which
means even a small application needs a fair amount of resource and descriptor setup. The helper
keeps that reusable construction code separate from the renderer's lifecycle and frame logic.

| Helper                      | Responsibility                                                                                                  |
| --------------------------- | --------------------------------------------------------------------------------------------------------------- |
| `init`                      | Creates the instance, surface, adapter, device, queue and configured presentation surface.                      |
| `create_pipeline`           | Creates the shader module, pipeline layout and render pipeline, including optional depth state.                 |
| `create_render_pass`        | Starts a render pass with colour clearing and an optional depth attachment.                                     |
| `create_texture_from_bytes` | Decodes image bytes, creates a GPU texture, uploads pixels through the queue, and creates its view and sampler. |
| `create_texture_bindgroup`  | Creates the fragment-stage texture and sampler layout and bind group.                                           |
| `create_uniform_bindgroup`  | Creates the vertex-stage uniform-buffer layout and bind group used by the camera.                               |
| `create_depth_texture`      | Creates the `Depth32Float` texture and view used for 3D depth testing.                                          |

The helper is not intended to hide wgpu behind an engine-style API. It accepts and returns normal
wgpu types, and the renderer and camera still own the resources they use. This keeps all important
GPU choices visible while making `Renderer::new` readable as a sequence of setup steps.

## Rendering lifecycle

`Renderer::new` performs one-time setup:

1. Initialise the surface, device, queue and surface configuration through `wgpu_helper::init`.
2. Load the cube geometry and create its vertex and index buffers.
3. Decode and upload the texture, then create its sampler and bind group.
4. Create the camera, uniform buffer and uniform bind group.
5. Create the depth texture.
6. Compile the WGSL shader and create the render pipeline.

For each redraw, `Renderer::update` moves the camera around the cube and writes the new
view-projection matrix to the GPU. `Renderer::render` then performs the frame:

```text
acquire surface texture
	-> create command encoder
	-> begin colour and depth render pass
	-> bind pipeline
	-> bind texture and camera uniform groups
	-> bind vertex and index buffers
	-> draw indexed cube
	-> submit command buffer to queue
	-> present surface texture
```

The application requests another redraw after each frame, producing the continuous animation.

## Structure

The desktop and browser boundary is intentionally small:

| Code                      | Shared       | Purpose                                                                                 |
| ------------------------- | ------------ | --------------------------------------------------------------------------------------- |
| `src/app.rs`              | Yes          | `winit` application lifecycle, events and redraw loop.                                  |
| `src/renderer.rs`         | Yes          | Owns the surface and GPU resources and coordinates setup, resize, update and rendering. |
| `src/wgpu_helper.rs`      | Yes          | Core helpers for wgpu initialisation and resource construction.                         |
| `src/camera.rs`           | Yes          | Camera state, view-projection transform, uniform buffer and binding.                    |
| `src/models.rs`           | Yes          | Vertex format and indexed cube geometry.                                                |
| `shaders/shader.wgsl`     | Yes          | Vertex transform and texture-sampling fragment shader.                                  |
| `src/platform/desktop.rs` | Desktop only | Blocking GPU startup, native window sizing and logging.                                 |
| `src/platform/web.rs`     | Web only     | Async GPU startup, browser canvas integration and DOM error reporting.                  |
| `index.html`              | Web only     | Browser host page and canvas.                                                           |

`src/platform/mod.rs` selects the adapter with `cfg(target_arch = "wasm32")`. The renderer also gets
its `Instant` type through that adapter, avoiding target-specific branches in the rendering path.

Cargo dependencies follow the same boundary. Desktop alone gets `pollster` and `env_logger`; WASM
alone gets `wasm-bindgen`, `web-sys`, `web-time` and console diagnostics. Dependencies for the other
target are not compiled or shipped.

## One renderer, two platforms

Desktop and WebAssembly are first-class targets of the same crate. The application does not contain
separate desktop and browser renderers: `app`, `renderer`, `wgpu_helper`, `camera`, `models`, the
texture and the WGSL shader are shared unchanged.

Only the code needed to enter and host the application differs. `src/platform/mod.rs` selects one of
two adapters at compile time:

```rust
#[cfg(not(target_arch = "wasm32"))]
pub use desktop::*;

#[cfg(target_arch = "wasm32")]
pub use web::*;
```

Both adapters expose the same small `Runtime` API to the shared `App`: create window attributes,  
start the renderer, access it when ready, report errors and indicate readiness. This is resolved at
compile time rather than through a trait object or runtime platform checks. `Renderer` also imports
`Instant` from the selected platform module, using `std::time::Instant` on desktop and
`web_time::Instant` in the browser.

| Concern            | Native desktop                                                                 | WebAssembly/browser                                                                                                    |
| ------------------ | ------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------- |
| Entry point        | `src/main.rs` calls the library's `start`, which launches the desktop adapter. | `wasm-bindgen` invokes the exported library `start` when the generated module loads.                                   |
| Host               | Creates a native `winit` window at 1024 x 768.                                 | Finds `#cube-canvas` in `index.html` and attaches it to a `winit` web window.                                          |
| Event loop         | Uses blocking `EventLoop::run_app`.                                            | Uses browser-compatible `EventLoopExtWebSys::spawn_app`.                                                               |
| GPU startup        | Blocks on async `Renderer::new` with `pollster::block_on`.                     | Starts `Renderer::new` with `wasm_bindgen_futures::spawn_local` without blocking the browser event loop.               |
| Renderer storage   | Stores `Option<Renderer>` directly in the runtime.                             | Uses `Rc<RefCell<Option<Renderer>>>` because initialisation completes asynchronously after startup returns.            |
| Initial resize     | Uses the native window's current size when creating the renderer.              | Catches up to the canvas size after async initialisation, since resize events may arrive before the renderer is ready. |
| Logging and errors | Uses `env_logger` and returns `anyhow::Result` errors to the native caller.    | Uses `console_log`, installs a panic hook, and displays startup or render failures in the page as well as the console. |
| Build output       | Produces a normal native executable with Cargo.                                | Produces a `cdylib` WebAssembly package and JavaScript bindings with `wasm-pack`.                                      |

The target-specific dependencies are declared in separate Cargo target sections. Desktop builds
compile `pollster` and `env_logger`; WASM builds compile `wasm-bindgen`, `web-sys`, `web-time` and
browser console support. Neither target compiles or ships the other platform's adapter or
dependencies.

This boundary keeps platform requirements visible without allowing them to leak into rendering
code. Once a window and renderer exist, both targets execute the same event handling, resource
creation, camera updates, command encoding, queue submission and presentation path.

## Run on desktop

Requirements are a recent stable Rust toolchain and a GPU and driver supported by wgpu.

```sh
make run
```

Use `cargo run` directly if `make` is unavailable. Press Escape or close the window to exit. Use
`make build` or `make release` to build without running.

## Run in a browser

The browser path requires WebGPU support, the WebAssembly Rust target, `wasm-pack`, and Node.js/npm
for the Vite development server. Install the Rust tooling once:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
```

Build and serve the current directory:

```sh
make build-web
make serve
```

Open <http://localhost:8000>. Use `make release-web` for an optimised WebAssembly build. This path
targets WebGPU directly and deliberately has no WebGL fallback.

## Validation

```sh
make check
make clippy
make fmt
```

The checks cover both targets where applicable. `make clean` removes Cargo output and the generated
`wasm-pack` package.

## Deliberate limits

This example does not include a scene graph, external model importer, material or lighting system,
mipmaps, multisampling, input-driven camera or engine-level resource abstraction. It is a compact
foundation for those features, with the complete window, GPU, rendering and desktop/web bootstrap
already in place.
