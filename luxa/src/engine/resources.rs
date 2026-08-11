use super::Engine;
use crate::models::{Material, Mesh, Texture};
use crate::nodes::{CameraDescriptor, CameraHandle, LightDescriptor, MeshNodeDescriptor, Node};
use crate::scenes::{Scene, SceneDescriptor};
use crate::{LightHandle, Transform};
use slotmap::new_key_type;

new_key_type! {
  pub struct MeshHandle;
  pub struct MaterialHandle;
  pub struct TextureHandle;
  pub struct NodeHandle;
  pub struct SceneHandle;
}

#[derive(Debug, Clone)]
pub struct NodeDescriptor {
  pub parent: Option<NodeHandle>,
  pub position: glam::Vec3,
  pub rotation: glam::Quat,
  pub scale: glam::Vec3,
}

impl Default for NodeDescriptor {
  fn default() -> Self {
    Self {
      parent: None,
      position: glam::Vec3::ZERO,
      rotation: glam::Quat::IDENTITY,
      scale: glam::Vec3::ONE,
    }
  }
}

impl Engine {
  pub fn create_scene(&mut self, desc: SceneDescriptor) -> SceneHandle {
    let root_handle = self.nodes.insert(Node::new(&self.device, &self.bind_group_layouts.node, crate::Transform::default()));
    let scene = self.scenes.insert(Scene::new(root_handle, desc));
    self.nodes[root_handle].set_scene(scene); // root has no parent to inherit from
    scene
  }

  pub fn scene(&self, handle: SceneHandle) -> anyhow::Result<&Scene> {
    self.scenes.get(handle).ok_or_else(|| anyhow::anyhow!("Invalid scene handle"))
  }

  pub fn scene_mut(&mut self, handle: SceneHandle) -> anyhow::Result<&mut Scene> {
    self.scenes.get_mut(handle).ok_or_else(|| anyhow::anyhow!("Invalid scene handle"))
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

  pub fn create_material(&mut self, desc: crate::models::MaterialDescriptor) -> anyhow::Result<MaterialHandle> {
    // Validate any supplied texture handles up front.
    for texture in [
      desc.base_color_texture,
      desc.metallic_roughness_texture,
      desc.normal_texture,
      desc.occlusion_texture,
      desc.emissive_texture,
    ]
    .into_iter()
    .flatten()
    {
      anyhow::ensure!(self.textures.contains_key(texture), "Invalid texture handle");
    }

    let mut material = Material::new(&self.device, &self.bind_group_layouts.material, &self.material_fallbacks, &self.textures);

    material.set_base_color_factor(desc.base_color_factor);
    material.set_metallic_factor(desc.metallic_factor);
    material.set_roughness_factor(desc.roughness_factor);
    material.set_emissive_factor(desc.emissive_factor);
    material.set_normal_scale(desc.normal_scale);
    material.set_occlusion_strength(desc.occlusion_strength);
    material.set_alpha_mode(desc.alpha_mode);
    material.set_alpha_cutoff(desc.alpha_cutoff);
    material.set_double_sided(desc.double_sided);

    if let Some(texture) = desc.base_color_texture {
      material.set_base_color_texture(texture);
    }
    if let Some(texture) = desc.metallic_roughness_texture {
      material.set_metallic_roughness_texture(texture);
    }
    if let Some(texture) = desc.normal_texture {
      material.set_normal_texture(texture);
    }
    if let Some(texture) = desc.occlusion_texture {
      material.set_occlusion_texture(texture);
    }
    if let Some(texture) = desc.emissive_texture {
      material.set_emissive_texture(texture);
    }

    let handle = self.materials.insert(material);

    log::info!("Created material with handle {:?}", handle);
    Ok(handle)
  }

  pub(crate) fn store_mesh(&mut self, mesh: Mesh) -> MeshHandle {
    let handle = self.meshes.insert(mesh);
    log::info!("Added mesh to cache with handle {:?}", handle);
    handle
  }

  pub fn create_node(&mut self, scene: SceneHandle, desc: NodeDescriptor) -> anyhow::Result<NodeHandle> {
    let parent = self.resolve_parent(scene, desc.parent)?;

    let node = Node::new(
      &self.device,
      &self.bind_group_layouts.node,
      Transform {
        position: desc.position,
        rotation: desc.rotation,
        scale: desc.scale,
      },
    );

    Ok(self.attach(node, parent)?)
  }

  pub fn remove_node(&mut self, handle: impl Into<NodeHandle>) -> anyhow::Result<()> {
    let handle = handle.into();
    anyhow::ensure!(self.node(handle)?.parent().is_some(), "cannot remove the scene root node");
    self.remove_node_recursive(handle);
    Ok(())
  }

  fn remove_node_recursive(&mut self, handle: NodeHandle) {
    if let Some(node) = self.nodes.remove(handle) {
      if let Some(parent_handle) = node.parent()
        && let Some(parent_node) = self.nodes.get_mut(parent_handle)
      {
        parent_node.remove_child(handle);
      }

      for &child_handle in node.children() {
        self.remove_node_recursive(child_handle);
      }
    }
  }

  pub fn remove_scene(&mut self, scene: SceneHandle) -> anyhow::Result<()> {
    anyhow::ensure!(self.scenes.contains_key(scene), "Invalid scene handle");
    self.nodes.retain(|_, node| node.scene() != scene);
    self.scenes.remove(scene);
    Ok(())
  }

  pub fn create_mesh_node(&mut self, scene: SceneHandle, desc: MeshNodeDescriptor) -> anyhow::Result<NodeHandle> {
    let parent = self.resolve_parent(scene, desc.parent)?;

    let node = Node::new_mesh(&self.device, &self.bind_group_layouts.node, &self.meshes, desc);
    Ok(self.attach(node, parent)?)
  }

  // Validates and inserts a mesh resource built by a `MeshBuilder`, returning its handle.
  pub fn create_mesh(&mut self, builder: crate::models::MeshBuilder) -> anyhow::Result<MeshHandle> {
    let (vertices, indices, material) = builder.into_parts();

    anyhow::ensure!(!vertices.is_empty(), "mesh has no vertices");
    anyhow::ensure!(!indices.is_empty(), "mesh has no indices");
    anyhow::ensure!(
      vertices.len() <= u16::MAX as usize + 1,
      "mesh has too many vertices for u16 indices (max {})",
      u16::MAX as usize + 1
    );
    let vertex_count = vertices.len() as u32;
    anyhow::ensure!(indices.iter().all(|&i| (i as u32) < vertex_count), "mesh index out of bounds");

    let material = match material {
      Some(material) => {
        anyhow::ensure!(self.materials.contains_key(material), "Invalid material handle");
        material
      }
      None => self.default_material(),
    };

    let mesh = Mesh::new(self, vertices, indices, material);
    Ok(self.store_mesh(mesh))
  }

  pub fn create_camera(&mut self, scene: SceneHandle, desc: CameraDescriptor) -> anyhow::Result<CameraHandle> {
    let parent = self.resolve_parent(scene, desc.parent)?;

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

    Ok(CameraHandle(self.attach(node, parent)?))
  }

  pub fn create_light(&mut self, scene: SceneHandle, desc: LightDescriptor) -> anyhow::Result<LightHandle> {
    let parent = self.resolve_parent(scene, desc.parent)?;

    let node = Node::new_light(&self.device, &self.bind_group_layouts.node, desc);

    Ok(LightHandle(self.attach(node, parent)?))
  }

  // Private helper function to attach a node to a parent node and return the handle of the newly created node.
  fn attach(&mut self, node: Node, parent: NodeHandle) -> anyhow::Result<NodeHandle> {
    let handle = self.nodes.insert(node);
    self.nodes[parent].add_child(handle);
    self.nodes[handle].set_parent(parent);

    // Assign the scene handle to the newly created node. This is important for ensuring that nodes are aware of which scene they belong to.
    let scene_handle = self.nodes[parent].scene();
    self.nodes[handle].set_scene(scene_handle);

    Ok(handle)
  }

  pub fn material(&self, handle: MaterialHandle) -> anyhow::Result<&Material> {
    self.materials.get(handle).ok_or_else(|| anyhow::anyhow!("Invalid material handle"))
  }

  pub fn material_mut(&mut self, handle: MaterialHandle) -> anyhow::Result<&mut Material> {
    self.materials.get_mut(handle).ok_or_else(|| anyhow::anyhow!("Invalid material handle"))
  }

  pub fn mesh(&self, handle: MeshHandle) -> anyhow::Result<&Mesh> {
    self.meshes.get(handle).ok_or_else(|| anyhow::anyhow!("Invalid mesh handle"))
  }

  pub fn node(&self, handle: impl Into<NodeHandle>) -> anyhow::Result<&Node> {
    self.nodes.get(handle.into()).ok_or_else(|| anyhow::anyhow!("Invalid node handle"))
  }

  pub fn node_mut(&mut self, handle: impl Into<NodeHandle>) -> anyhow::Result<&mut Node> {
    self.nodes.get_mut(handle.into()).ok_or_else(|| anyhow::anyhow!("Invalid node handle"))
  }

  fn resolve_parent(&self, scene: SceneHandle, parent: Option<NodeHandle>) -> anyhow::Result<NodeHandle> {
    let root = self.scene(scene)?.root();
    match parent {
      None => Ok(root),
      Some(parent) => {
        let parent_scene = self.node(parent)?.scene(); // errors if the handle is stale
        anyhow::ensure!(parent_scene == scene, "Parent node does not belong to the specified scene");
        Ok(parent)
      }
    }
  }
}
