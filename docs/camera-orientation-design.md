# Camera orientation modes

Status: Proposed design  
Target: Luxa `0.1`

## Purpose

Luxa cameras need to support two common behaviours:

- Free cameras controlled by position and rotation.
- Target cameras that continue looking at a world-space point when moved, such as orbit cameras.

A look-at-only camera ignores node rotation, making `set_rotation` ineffective. A rotation-only
camera requires an orbit controller to recalculate its orientation after every movement. Two
explicit orientation modes support both behaviours without making either one a special case.

## Decision

Camera orientation has two modes:

```rust
pub enum CameraOrientation {
  NodeRotation,
  LookAt { target: Vec3, up: Vec3 },
}
```

`NodeRotation` derives the view direction from the camera node's world rotation. `LookAt` derives
it from the camera node's world position and a retained world-space target and up vector.

`CameraDescriptor` specifies the initial mode:

```rust
pub struct CameraDescriptor {
  pub parent: Option<NodeHandle>,
  pub transform: Transform,
  pub orientation: CameraOrientation,
  pub fov_degrees: f32,
  pub near_plane: f32,
  pub far_plane: f32,
}
```

The default is `NodeRotation`. With an identity rotation, the camera looks along negative Z with
positive Y as up. This also avoids requiring an arbitrary default target or handling a default
eye and target at the same position.

## Behaviour

| Operation             | Result                                                                 |
| --------------------- | ---------------------------------------------------------------------- |
| `set_position`        | Moves the eye without changing orientation mode.                       |
| `set_rotation`        | Updates rotation and switches the camera to `NodeRotation`.            |
| `set_transform`       | Updates the node transform and switches the camera to `NodeRotation`.  |
| `look_at(target, up)` | Switches the camera to `LookAt` with world-space target and up values. |
| `set_scale`           | Does not affect the camera view.                                       |

Moving a camera in `LookAt` mode preserves its target, which supports orbit cameras. Moving a
camera in `NodeRotation` mode preserves its orientation, which supports free and first-person
cameras.

## View construction

Both modes use the camera node's world position as the eye.

`NodeRotation` uses the node's world rotation:

```rust
let forward = world_rotation * Vec3::NEG_Z;
let up = world_rotation * Vec3::Y;
let view = look_at_mat4(eye, eye + forward, up);
```

`LookAt` uses the retained world-space target and up vector:

```rust
let view = look_at_mat4(eye, target, up);
```

Scene traversal should track world rotation independently from the world matrix:

```rust
let world_rotation = parent_world_rotation * node.rotation();
```

Passing world position and world rotation to camera view construction avoids recovering rotation
from a matrix that may contain scale.

## Scale

Camera scale has no optical meaning. Field of view controls magnification, while the near and far
planes control the visible depth range.

Scale remains part of `Transform` because a camera is still a scene node, but camera view
construction ignores it. Camera nodes and their ancestors should normally use unit scale. Tracking
world rotation separately ensures accidental scale does not distort camera orientation.

## Validation and degeneracy

Camera creation and updates should reject invalid projection values:

- Field of view must be greater than 0 and less than 180 degrees.
- The near plane must be greater than 0.
- The far plane must be greater than the near plane.

`LookAt` also needs defined behaviour for invalid orientation values:

- `up` must not have zero length.
- The eye and target must not occupy the same position.
- The view direction and up vector must not be parallel.

Where these conditions depend on the final world transform, rendering should return an error or
retain the last valid camera orientation rather than producing a matrix containing non-finite
values.

## Acceptance criteria

- An identity `NodeRotation` camera looks along negative Z with positive Y up.
- `set_rotation` visibly changes a camera previously in `LookAt` mode.
- Moving a `NodeRotation` camera preserves its world orientation.
- Moving a `LookAt` camera preserves its world-space target.
- Parent rotation affects a `NodeRotation` camera.
- Parent translation affects the eye position in both modes.
- Node and parent scale do not affect the camera view orientation.
- Invalid look-at and projection values do not produce non-finite view-projection matrices.
