# wgpu-learning

Small, focused projects for learning [wgpu](https://wgpu.rs/) and modern GPU rendering in Rust.
The aim is to keep each example readable end to end while still providing a complete, functional
rendering application.

## Projects

### [Minimal wgpu Cube](cube/)

A texture-mapped 3D cube with perspective projection that runs as both a native desktop application
and WebAssembly in a WebGPU-capable browser. It demonstrates the rendering loop, wgpu helper layer,
GPU setup, models and buffers, camera transforms, shader uniforms, textures, depth testing and
cross-platform startup.

See the [cube README](cube/README.md) for the architecture, rendering lifecycle, project structure,
and desktop and browser build instructions.

### [Reusable 3D engine](wgpu-engine/)

The reusable engine hides the raw `wgpu` resource graph behind engine-owned meshes, materials,
textures, nodes and handles. See the [glTF parser guide](wgpu-engine/PARSER.md) for its asset-loading
flow, terminology, supported features and current limitations.

### Helpful Stuff

https://3dviewer.net/
https://sandbox.babylonjs.com/
https://gltf-viewer.donmccurdy.com/

https://github.khronos.org/glTF-Assets/
https://github.khronos.org/glTF-Sample-Viewer-Release/
