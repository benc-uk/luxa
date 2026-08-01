use super::Engine;
use crate::models::{Material, Mesh, Texture};
use crate::nodes::Node3D;
use glam::{Quat, Vec3};
use slotmap::new_key_type;

new_key_type! {
  pub struct MeshHandle;
  pub struct MaterialHandle;
  pub struct TextureHandle;
  pub struct Node3DHandle;
  pub struct SceneHandle;
}

impl Engine {
  pub fn create_scene(&mut self) -> (SceneHandle, Node3DHandle) {
    let root_node = Node3D::new(
      &self.device,
      &self.bind_group_layouts.node,
      glam::Vec3::ZERO,
      glam::Quat::IDENTITY,
      glam::Vec3::new(1.0, 1.0, 1.0),
    );
    let root_handle = self.nodes.insert(root_node);
    let scene_handle = self.scenes.insert(root_handle);
    log::info!("Created scene with handle {:?}", scene_handle);

    (scene_handle, root_handle)
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

  pub(crate) fn add_mesh(&mut self, mesh: Mesh) -> MeshHandle {
    let handle = self.meshes.insert(mesh);
    log::info!("Added mesh to cache with handle {:?}", handle);
    handle
  }

  pub fn create_node(&mut self, parent: Node3DHandle, position: Vec3, rotation: Quat, scale: Vec3) -> Node3DHandle {
    let node = Node3D::new(&self.device, &self.bind_group_layouts.node, position, rotation, scale);
    self.attach(node, parent)
  }

  pub fn remove_node(&mut self, handle: Node3DHandle) {
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

  pub fn create_mesh_node(&mut self, parent: Node3DHandle, meshes: Vec<MeshHandle>, position: Vec3, rotation: Quat, scale: Vec3) -> Node3DHandle {
    let node = Node3D::new_mesh(&self.device, &self.bind_group_layouts.node, &self.meshes, meshes, position, rotation, scale);
    self.attach(node, parent)
  }

  pub fn create_camera_node(&mut self, parent: Node3DHandle, position: Vec3, look_at: Vec3, scale: Vec3, fovy: f32, znear: f32, zfar: f32) -> Node3DHandle {
    let node = Node3D::new_camera(&self.device, &self.bind_group_layouts.node, position, look_at, scale, fovy, znear, zfar);
    self.attach(node, parent)
  }

  pub fn create_light_node(&mut self, parent: Node3DHandle, position: Vec3, color: Vec3, intensity: f32) -> Node3DHandle {
    let node = Node3D::new_light(&self.device, &self.bind_group_layouts.node, position, color, intensity);
    self.attach(node, parent)
  }

  fn attach(&mut self, node: Node3D, parent: Node3DHandle) -> Node3DHandle {
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

  pub fn node(&self, handle: Node3DHandle) -> &Node3D {
    self.nodes.get(handle).expect("Invalid node handle")
  }

  pub fn node_mut(&mut self, handle: Node3DHandle) -> &mut Node3D {
    self.nodes.get_mut(handle).expect("Invalid node handle")
  }
}
