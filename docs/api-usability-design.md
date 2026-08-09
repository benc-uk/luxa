# Public API usability redesign

Status: Approved design, pending implementation  
Target: Luxa `0.1`

## Decision

Keep Luxa's generational handles, but stop exposing scene-root plumbing and long positional
constructors in the normal workflow.

The redesigned API will:

1. Create scenes, nodes, cameras and lights from descriptor structs.
2. Attach new objects to the scene root unless an explicit parent is supplied.
3. Add a `Transform` value for setting position, rotation and scale together.
4. Use typed `CameraHandle` and `LightHandle` wrappers around `NodeHandle`.
5. Return `Result` for invalid handles and invalid cross-scene operations instead of panicking.
6. Make `MeshBuilder` independent of `Engine` and expose custom mesh insertion.
7. Advance frame time inside `render`, removing the normal `update()` call.

This is one intentional breaking change before `0.1`. The old API will be removed rather than
deprecated because Luxa is not yet published and has no stability promise.

## Explicitly deferred

The following are not part of this work:

- `SceneEditor` or `edit_scene`. Passing `SceneHandle` to scene operations provides the useful part
  without another borrowing facade.
- An ECS or component system.
- Asset unloading and reference counting. Meshes, materials and textures remain alive for the
  lifetime of the engine.
- Typed public error enums. The first implementation continues to use `anyhow::Result`.
- Multiple views or cameras in one frame. One call to `render` represents one frame.
- Compatibility aliases for old names.

## Target user experience

```rust
use glam::Vec3;
use luxa::{CameraDescriptor, ModelDescriptor, SceneDescriptor, Transform};

let mut engine = Engine::new(surface_target, (width, height)).await?;

let scene = engine.create_scene(SceneDescriptor {
  background_color: [0.1, 0.1, 0.1],
  ambient_intensity: 0.3,
  ..Default::default()
});

let camera = engine.create_camera(
  scene,
  CameraDescriptor {
    transform: Transform::from_position(Vec3::new(0.0, 1.0, 4.0)),
    look_at: Vec3::ZERO,
    vertical_fov_degrees: 70.0,
    far_plane: 200.0,
    ..Default::default()
  },
)?;

let model = engine.load_gltf_bytes(scene, &model_bytes, ModelDescriptor::default())?;
engine.node_mut(model)?.set_transform(Transform {
  position,
  scale,
  ..Default::default()
});

// Once per frame
engine.node_mut(camera)?.set_position(orbit_position);
engine.render(scene, camera)?;
```

The user retains handles only for objects needed across frames. The scene root is not retrieved or
stored, creation arguments are named, and the camera role is checked by its handle type.

## Public API

### Core types

```rust
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Transform {
  pub position: Vec3,
  pub rotation: Quat,
  pub scale: Vec3,
}

impl Default for Transform {
  fn default() -> Self {
    Self {
      position: Vec3::ZERO,
      rotation: Quat::IDENTITY,
      scale: Vec3::ONE,
    }
  }
}

impl Transform {
  pub fn from_position(position: Vec3) -> Self;
  pub fn from_rotation(rotation: Quat) -> Self;
  pub fn from_scale(scale: Vec3) -> Self;
}

pub struct SceneHandle(/* private */);
pub struct NodeHandle(/* private */);
pub struct CameraHandle(NodeHandle);
pub struct LightHandle(NodeHandle);
pub struct MeshHandle(/* private */);
pub struct MaterialHandle(/* private */);
pub struct TextureHandle(/* private */);

impl From<CameraHandle> for NodeHandle { /* ... */ }
impl From<LightHandle> for NodeHandle { /* ... */ }
```

All handles remain `Copy`, `Clone`, `Debug`, `Eq` and `Hash`. Handles are local to the `Engine`
that created them.

### Descriptors

```rust
#[derive(Debug, Clone, Default)]
pub struct SceneDescriptor {
  pub background_color: [f32; 3],
  pub ambient_color: [f32; 3],
  pub ambient_intensity: f32,
    pub ibl_enabled: bool,
    }

    #[derive(Debug, Clone, Default)]
    pub struct NodeDescriptor {
      pub parent: Option<NodeHandle>,
  pub transform: Transform,
}

#[derive(Debug, Clone)]
pub struct CameraDescriptor {
  pub parent: Option<NodeHandle>,
  pub transform: Transform,
  pub look_at: Vec3,
  pub up: Vec3,
  pub vertical_fov_degrees: f32,
  pub near_plane: f32,
  pub far_plane: f32,
}

#[derive(Debug, Clone)]
pub struct LightDescriptor {
  pub parent: Option<NodeHandle>,
  pub transform: Transform,
  pub color: Vec3,
  pub intensity: f32,
}

#[derive(Debug, Clone, Default)]
pub struct ModelDescriptor {
  pub parent: Option<NodeHandle>,
  pub transform: Transform,
}

#[derive(Debug, Clone)]
pub struct MeshNodeDescriptor {
  pub parent: Option<NodeHandle>,
  pub transform: Transform,
  pub meshes: Vec<MeshHandle>,
}
```

`parent: None` means the scene root. An explicit parent must belong to the same scene.

Defaults:

| Descriptor         | Default                                                                                                   |
| ------------------ | --------------------------------------------------------------------------------------------------------- |
| `SceneDescriptor`  | Existing scene defaults.                                                                                  |
| `NodeDescriptor`   | Root parent and identity transform.                                                                       |
| `CameraDescriptor` | Root parent, identity transform, looking down negative Z with Y up, 60 degree FOV, 0.1 near and 1000 far. |
| `LightDescriptor`  | Root parent, identity transform, white colour and intensity 1.0.                                          |
| `ModelDescriptor`  | Root parent and identity transform.                                                                       |

### Engine methods

```rust
impl Engine {
  // Scenes
  pub fn create_scene(&mut self, descriptor: SceneDescriptor) -> SceneHandle;
  pub fn remove_scene(&mut self, scene: SceneHandle) -> Result<()>;
  pub fn scene(&self, scene: SceneHandle) -> Result<&Scene>;
  pub fn scene_mut(&mut self, scene: SceneHandle) -> Result<&mut Scene>;

  // Nodes
  pub fn create_node(&mut self, scene: SceneHandle, descriptor: NodeDescriptor) -> Result<NodeHandle>;
  pub fn create_mesh_node(&mut self, scene: SceneHandle, descriptor: MeshNodeDescriptor) -> Result<NodeHandle>;
  pub fn create_camera(&mut self, scene: SceneHandle, descriptor: CameraDescriptor) -> Result<CameraHandle>;
  pub fn create_light(&mut self, scene: SceneHandle, descriptor: LightDescriptor) -> Result<LightHandle>;
  pub fn remove_node(&mut self, node: impl Into<NodeHandle>) -> Result<()>;
  pub fn node(&self, node: impl Into<NodeHandle>) -> Result<&Node>;
  pub fn node_mut(&mut self, node: impl Into<NodeHandle>) -> Result<&mut Node>;

  // Models and meshes
  pub fn load_gltf(&mut self, scene: SceneHandle, path: impl AsRef<Path>, descriptor: ModelDescriptor) -> Result<NodeHandle>;
  pub fn load_gltf_bytes(&mut self, scene: SceneHandle, bytes: &[u8], descriptor: ModelDescriptor) -> Result<NodeHandle>;
  pub fn create_mesh(&mut self, builder: MeshBuilder) -> Result<MeshHandle>;

  // Frame lifecycle
  pub fn resize(&mut self, size: Size);
  pub fn render(&mut self, scene: SceneHandle, camera: CameraHandle) -> Result<()>;
  pub fn elapsed_time(&self) -> Duration;
}
```

Existing environment, material and texture APIs remain unchanged during this work. They can receive
their own descriptor redesign later without blocking the scene and node usability improvements.

### Node methods

```rust
impl Node {
  pub fn transform(&self) -> Transform;
  pub fn set_transform(&mut self, transform: Transform) -> &mut Self;
  pub fn set_position(&mut self, position: Vec3) -> &mut Self;
  pub fn set_rotation(&mut self, rotation: Quat) -> &mut Self;
  pub fn set_scale(&mut self, scale: Vec3) -> &mut Self;
  pub fn look_at(&mut self, target: Vec3, up: Vec3) -> &mut Self;
}
```

Camera nodes retain a world-space look-at target and up vector. Moving the node changes the eye
position while preserving its target, which supports orbit cameras without recalculating rotation
each frame. `Node::look_at` updates both the camera target and the node rotation.

### Mesh builder

```rust
let mesh = engine.create_mesh(
  MeshBuilder::new()
    .cube()
    .material(material),
)?;
```

```rust
impl MeshBuilder {
  pub fn new() -> Self;
  pub fn vertices(self, vertices: impl IntoIterator<Item = Vertex>) -> Self;
  pub fn indices(self, indices: impl IntoIterator<Item = u16>) -> Self;
  pub fn cube(self) -> Self;
  pub fn uv_sphere(self, slices: u32, stacks: u32) -> Result<Self>;
  pub fn material(self, material: MaterialHandle) -> Self;
}
```

`create_mesh` supplies the default material when none is set and validates empty geometry, index
bounds, material handles and the `u16` vertex limit. `Mesh::new` and arena insertion become private.

## Internal changes

- Rename `Node3D` to `Node` and `Node3DHandle` to `NodeHandle`.
- Validate scene membership by following a node's parent chain and comparing its root with the
  scene's root. Do not duplicate the scene handle on every node.
- Build camera view matrices from the node's world position and the camera's retained look-at target
  and up vector.
- Prevent root-node removal through the public API.
- Validate every public handle before indexing a slot map.
- Move frame-time upload from `update` into `render`.
- Keep meshes, materials and textures engine-owned when nodes or scenes are removed.

## Breaking API map

| Current                                        | Replacement                                                 |
| ---------------------------------------------- | ----------------------------------------------------------- |
| `Node3D`                                       | `Node`                                                      |
| `Node3DHandle`                                 | `NodeHandle`                                                |
| `create_scene()`                               | `create_scene(SceneDescriptor::default())`                  |
| `scene(scene).root()` before creation          | Omit the parent or use `parent: None`                       |
| `create_node(root, position, rotation, scale)` | `create_node(scene, NodeDescriptor { ... })`                |
| `create_camera_node(...)`                      | `create_camera(scene, CameraDescriptor { ... })`            |
| `create_light_node(...)`                       | `create_light(scene, LightDescriptor { ... })`              |
| `load_gltf(path, root)`                        | `load_gltf(scene, path, ModelDescriptor::default())`        |
| `load_gltf_bytes(bytes, root)`                 | `load_gltf_bytes(scene, bytes, ModelDescriptor::default())` |
| Multiple transform setter lookups              | `node_mut(node)?.set_transform(transform)`                  |
| `MeshBuilder::new(&engine).build(&mut engine)` | `engine.create_mesh(MeshBuilder::new()...)`                 |
| `update(); render(scene, camera)`              | `render(scene, camera)` using a typed `CameraHandle`        |
| `t()`                                          | `elapsed_time()`                                            |

## Implementation plan

### 1. Add tests for current behaviour

- Test local and world transform composition.
- Test recursive node removal.
- Test camera view construction.
- Test glTF transform flattening.

Complete when the tests pass without public API changes.

### 2. Add new value types

- Add `Transform` and all descriptors.
- Add typed `CameraHandle` and `LightHandle` wrappers.
- Rename `Node3D` and `Node3DHandle`.
- Export the new public types from `lib.rs`.

Complete when `luxa` compiles with the new types alongside the old behaviour.

### 3. Add scene membership validation

- Add an internal ancestry check that follows parent handles to a root node.
- Validate explicit parents and render cameras by comparing that root with the selected scene root.

Complete when cross-scene parents and cameras return errors and removal tests pass.

### 4. Replace creation and loading methods

- Replace positional node, camera and light constructors with descriptors.
- Change glTF loading to accept a scene and `ModelDescriptor`.
- Make root attachment implicit when `parent` is `None`.
- Return `Result` instead of panicking on invalid handles.

Complete when no public creation or loading path requires a root handle.

### 5. Replace camera and render behaviour

- Preserve camera look-at and up state while using the node world position as the eye.
- Change `render` to require a typed `CameraHandle`.
- Move time advancement into `render`.
- Replace `t` with `elapsed_time`.

Complete when a frame is rendered with `engine.render(scene, camera)?` and no separate `update`.

### 6. Replace mesh construction

- Remove the engine borrow from `MeshBuilder::new`.
- Add custom vertices and indices.
- Add validated `Engine::create_mesh` insertion.
- Make `Mesh::new` private and remove `mesh_mut`.

Complete when user-provided vertices and indices produce a renderable `MeshHandle` through the
public API.

### 7. Migrate the viewer and documentation

- Update `web-viewer` to the new API.
- Remove its stored root-node handle.
- Update the README example and public API list.
- Add rustdoc examples for scene creation, glTF loading and per-frame mutation.

Complete when the viewer builds for WebAssembly and displays the same models and environments as
before.

## Final acceptance criteria

- The viewer stores scene, camera and model handles, but no root handle.
- No public constructor has more than a scene handle and one descriptor argument.
- The normal frame loop contains `render(scene, camera)` and no `update()` call.
- A non-camera node cannot be passed to `render`.
- Cross-scene parents and cameras return errors.
- Removed or invalid handles do not panic in public methods.
- Moving a camera preserves its look-at target, and parent transforms affect its world-space eye
  position.
- Custom vertices and indices can be inserted through the public API.
- `cargo test -p luxa` passes.
- `cargo check -p web-viewer --target wasm32-unknown-unknown` passes.
- `cargo fmt --all -- --check` passes.
- The production viewer bundle builds and its visual behaviour is unchanged.
