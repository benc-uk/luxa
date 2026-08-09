use super::Engine;
use crate::CameraDescriptor;
use crate::Transform;
use crate::models::{Material, Mesh, Texture};
use crate::nodes::LightDescriptor;
use crate::nodes::MeshNodeDescriptor;
use crate::nodes::Node;
use crate::scenes::{Scene, SceneDescriptor};
use glam::{Quat, Vec3};
use slotmap::new_key_type;

new_key_type! {
  pub struct MeshHandle;
  pub struct MaterialHandle;
  pub struct TextureHandle;
  pub struct NodeHandle;
  pub struct SceneHandle;
}

#[derive(Debug, Clone, Default)]
pub struct NodeDescriptor {
  pub parent: Option<NodeHandle>,
  pub transform: Transform,
}

impl Engine {
  pub fn create_scene(&mut self, desc: SceneDescriptor) -> SceneHandle {
    let root_node = Node::new(&self.device, &self.bind_group_layouts.node, crate::Transform::default());

    let root_handle = self.nodes.insert(root_node);
    self.scenes.insert(Scene::new(root_handle, desc))
  }

  pub fn scene(&self, handle: SceneHandle) -> &Scene {
    self.scenes.get(handle).expect("Invalid scene handle")
  }

  pub fn scene_mut(&mut self, handle: SceneHandle) -> &mut Scene {
    self.scenes.get_mut(handle).expect("Invalid scene handle")
  }

  pub fn create_texture(&mut self, path: &str) -> anyhow::Result<TextureHandle> {
    let texture = Texture::from_file(&self.device, &self.queue, path)?;
    let handle = self.textures.insert(texture);
    log::info!("Loaded texture from {} with handle {:?}", path, handle);
    Ok(handle)
  }

  pub(crate) fn create_texture_from_image(&mut self, image: &image::DynamicImage, format: wgpu::TextureFormat, label: &str) -> anyhow::Result<TextureHandle> {
    let texture = Texture::from_image(&self.device, &self.queue, image, format, Some(label))?;
    let handle = self.textures.insert(texture);
    log::info!("Created texture {} with handle {:?}", label, handle);
    Ok(handle)
  }

  pub fn create_material(&mut self, texture: Option<TextureHandle>) -> MaterialHandle {
    let mut material = Material::new(&self.device, &self.bind_group_layouts.material, &self.material_fallbacks, &self.textures);

    if let Some(texture) = texture {
      self.textures.get(texture).expect("Invalid texture handle");
      material.set_base_color_texture(texture);
    }

    let handle = self.materials.insert(material);

    log::info!("Created material with handle {:?}", handle);
    handle
  }

  pub(crate) fn store_mesh(&mut self, mesh: Mesh) -> MeshHandle {
    let handle = self.meshes.insert(mesh);
    log::info!("Added mesh to cache with handle {:?}", handle);
    handle
  }

  pub fn create_node(&mut self, scene: SceneHandle, desc: NodeDescriptor) -> NodeHandle {
    let parent = desc.parent.unwrap_or_else(|| self.scenes[scene].root());

    let node = Node::new(&self.device, &self.bind_group_layouts.node, desc.transform);

    self.attach(node, parent)
  }

  pub fn remove_node(&mut self, handle: NodeHandle) {
    if let Some(node) = self.nodes.remove(handle) {
      if let Some(parent_handle) = node.parent() {
        if let Some(parent_node) = self.nodes.get_mut(parent_handle) {
          parent_node.remove_child(handle);
        }
      }

      for &child_handle in node.children() {
        self.remove_node(child_handle);
      }
    } else {
      log::warn!("Attempted to remove non-existent node with handle {:?}", handle);
    }
  }

  pub fn create_mesh(&mut self, scene: SceneHandle, desc: MeshNodeDescriptor) -> NodeHandle {
    let parent = desc.parent.unwrap_or_else(|| self.scenes[scene].root());

    let node = Node::new_mesh(&self.device, &self.bind_group_layouts.node, &self.meshes, desc);
    self.attach(node, parent)
  }

  pub fn create_camera(&mut self, scene: SceneHandle, desc: CameraDescriptor) -> anyhow::Result<NodeHandle> {
    // If no parent is specified, attach the camera to the root node of the scene.
    let parent = desc.parent.unwrap_or_else(|| self.scenes[scene].root());

    anyhow::ensure!(
      desc.fov_degrees.is_finite() && desc.fov_degrees > 0.0 && desc.fov_degrees < 180.0,
      "camera FOV must be between 0 and 180 degrees"
    );
    anyhow::ensure!(desc.near_plane.is_finite() && desc.near_plane > 0.0, "camera near plane must be positive");
    anyhow::ensure!(
      desc.far_plane.is_finite() && desc.far_plane > desc.near_plane,
      "camera far plane must exceed its near plane"
    );

    let node = Node::new_camera(&self.device, &self.bind_group_layouts.node, desc);

    Ok(self.attach(node, parent))
  }

  pub fn create_light(&mut self, scene: SceneHandle, desc: LightDescriptor) -> NodeHandle {
    let parent = desc.parent.unwrap_or_else(|| self.scenes[scene].root());

    let node = Node::new_light(&self.device, &self.bind_group_layouts.node, desc);

    self.attach(node, parent)
  }

  // Private helper function to attach a node to a parent node and return the handle of the newly created node.
  fn attach(&mut self, node: Node, parent: NodeHandle) -> NodeHandle {
    let handle = self.nodes.insert(node);
    self.nodes[parent].add_child(handle);
    self.nodes[handle].set_parent(parent);
    handle
  }

  pub fn material(&self, handle: MaterialHandle) -> &Material {
    self.materials.get(handle).expect("Invalid material handle")
  }

  pub fn material_mut(&mut self, handle: MaterialHandle) -> &mut Material {
    self.materials.get_mut(handle).expect("Invalid material handle")
  }

  pub fn mesh(&self, handle: MeshHandle) -> &Mesh {
    self.meshes.get(handle).expect("Invalid mesh handle")
  }

  pub fn mesh_mut(&mut self, handle: MeshHandle) -> &mut Mesh {
    self.meshes.get_mut(handle).expect("Invalid mesh handle")
  }

  pub fn node(&self, handle: NodeHandle) -> &Node {
    self.nodes.get(handle).expect("Invalid node handle")
  }

  pub fn node_mut(&mut self, handle: NodeHandle) -> &mut Node {
    self.nodes.get_mut(handle).expect("Invalid node handle")
  }
}
