use crate::Transform;
use crate::nodes::camera::CameraOrientation::NodeRotation;
use crate::nodes::{Node, NodeHandle, NodeKind};
use glam::{Mat4, Quat, Vec3};

pub(crate) struct CameraData {
  pub fovy: f32,
  pub znear: f32,
  pub zfar: f32,
  pub orientation: CameraOrientation,
}

impl CameraData {
  // This is forwarded from Node::view_proj, which is called from Engine::render_scene.
  // It needs the world position and rotation of the camera node, which is why those are passed in.
  pub(crate) fn view_proj(&self, world_pos: Vec3, world_rotation: Quat, aspect: f32) -> anyhow::Result<Mat4> {
    let view = match &self.orientation {
      CameraOrientation::NodeRotation => {
        let forward = world_rotation * Vec3::NEG_Z;
        let up = world_rotation * Vec3::Y;
        glam::camera::rh::view::look_at_mat4(world_pos, world_pos + forward, up)
      }

      CameraOrientation::LookAt { target, up } => {
        let direction = *target - world_pos;

        anyhow::ensure!(direction.length_squared() > 1e-12, "camera eye equals target");
        anyhow::ensure!(up.length_squared() > 1e-12, "camera up vector is zero");
        anyhow::ensure!(direction.cross(*up).length_squared() > 1e-12, "camera direction is parallel to up");

        glam::camera::rh::view::look_at_mat4(world_pos, *target, *up)
      }
    };

    let projection = glam::camera::rh::proj::directx::perspective(self.fovy.to_radians(), aspect, self.znear, self.zfar);

    Ok(projection * view)
  }
}

#[derive(Debug, Clone)]
pub struct CameraDescriptor {
  pub parent: Option<NodeHandle>,
  pub position: Vec3,
  pub rotation: Quat,
  pub scale: Vec3,
  pub orientation: CameraOrientation,
  pub fov_degrees: f32,
  pub near_plane: f32,
  pub far_plane: f32,
}

impl Default for CameraDescriptor {
  fn default() -> Self {
    Self {
      parent: None,
      position: Vec3::ZERO,
      rotation: Quat::IDENTITY,
      scale: Vec3::ONE,
      orientation: NodeRotation,
      fov_degrees: 60.0,
      near_plane: 0.1,
      far_plane: 1000.0,
    }
  }
}

#[derive(Debug, Clone)]
pub enum CameraOrientation {
  NodeRotation,
  LookAt { target: Vec3, up: Vec3 },
}

impl Node {
  pub(crate) fn new_camera(device: &wgpu::Device, bind_group_layout: &wgpu::BindGroupLayout, desc: CameraDescriptor) -> Self {
    let orientation = desc.orientation;
    let fov_degrees = desc.fov_degrees;
    let near_plane = desc.near_plane;
    let far_plane = desc.far_plane;
    let transform = Transform {
      position: desc.position,
      rotation: desc.rotation,
      scale: desc.scale,
    };

    let mut node = Self::new(device, bind_group_layout, transform);

    // Extra node data for camera nodes is stored in the NodeKind enum variant.
    node.kind = NodeKind::Camera(CameraData {
      fovy: fov_degrees,
      znear: near_plane,
      zfar: far_plane,
      orientation,
    });

    node
  }
}
