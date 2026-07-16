use glam::{Mat4, Quat, Vec3, camera};
use slotmap::{SlotMap, new_key_type};
use web_time::Instant;

use crate::common::Size;
use crate::models::{Material, Mesh, Texture, Vertex};
use crate::{Node3D, wgpu_helper};

use wgpu::util::DeviceExt;

new_key_type! {
  pub struct MeshHandle;
  pub struct MaterialHandle;
  pub struct TextureHandle;
  pub struct Node3DHandle;
  pub struct SceneHandle;
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct FrameUniform {
  time: f32,
}

impl FrameUniform {
  fn new() -> Self {
    Self { time: 0.0 }
  }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
  view_proj: [f32; 16],
}

impl CameraUniform {
  fn new() -> Self {
    Self {
      view_proj: Mat4::IDENTITY.to_cols_array(),
    }
  }
}

pub struct Engine {
  surface: wgpu::Surface<'static>,
  aspect: f32,
  device: wgpu::Device,
  queue: wgpu::Queue,
  surf_config: wgpu::SurfaceConfiguration,
  render_pipe: wgpu::RenderPipeline,

  // Depth buffer for 3D rendering
  depth_texture_view: wgpu::TextureView,
  default_texture: Texture,
  default_material: MaterialHandle,

  // When rendering started, used to drive time-based animation.
  start_time: Instant,
  is_surface_configured: bool,

  // BG bind group for frame-level uniforms (camera, time, etc.)
  frame_uniform: FrameUniform,
  frame_uniform_buffer: wgpu::Buffer,
  frame_cam_bind_group: wgpu::BindGroup,
  camera_uniform: CameraUniform,
  camera_uniform_buffer: wgpu::Buffer,

  // Arenas for storing resources, so we can return handles to them.
  scenes: SlotMap<SceneHandle, Node3DHandle>,
  nodes: SlotMap<Node3DHandle, Node3D>,
  meshes: SlotMap<MeshHandle, Mesh>,
  materials: SlotMap<MaterialHandle, Material>,
  textures: SlotMap<TextureHandle, Texture>,
}

impl Engine {
  pub async fn new(surface_target: impl Into<wgpu::SurfaceTarget<'static>>, size: Size) -> anyhow::Result<Self> {
    let aspect = size.width as f32 / size.height as f32;

    // Step 1 - Core instance, surface, device & queue creation
    let (surface, device, queue, surf_config) = wgpu_helper::init(size, surface_target).await?;

    // Step 5 - Create a depth texture for 3D rendering
    let (_depth_texture, depth_texture_view) = wgpu_helper::create_depth_texture(&device, &surf_config);

    let frame_cam_bg_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
      label: Some("Uniform Bind Group Layout"),
      entries: &[
        wgpu_helper::uniform_entry(0, wgpu::ShaderStages::VERTEX),          // camera uniform
        wgpu_helper::uniform_entry(1, wgpu::ShaderStages::VERTEX_FRAGMENT), // time uniform
      ],
    });

    let frame_uniform = FrameUniform::new();
    let frame_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
      label: Some("Frame Uniform Buffer"),
      contents: bytemuck::cast_slice(&[frame_uniform]),
      usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let camera_uniform = CameraUniform::new();
    let camera_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
      label: Some("Camera Uniform Buffer"),
      contents: bytemuck::cast_slice(&[camera_uniform]),
      usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let frame_cam_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
      label: Some("Frame Bind Group"),
      layout: &frame_cam_bg_layout,
      entries: &[
        wgpu::BindGroupEntry {
          binding: 0,
          resource: camera_uniform_buffer.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
          binding: 1,
          resource: frame_uniform_buffer.as_entire_binding(),
        },
      ],
    });

    let mat_bg_layout = Material::get_bind_group_layout(&device);
    let node_bg_layout = Node3D::get_bind_group_layout(&device);

    // Step 6 - Create the shaders & render pipeline
    let render_pipe = wgpu_helper::create_pipeline(
      &device,
      surf_config.format.add_srgb_suffix(),
      include_str!("../shaders/shader.wgsl"),
      &[Vertex::desc()],
      &[Some(&frame_cam_bg_layout), Some(&mat_bg_layout), Some(&node_bg_layout)],
      true,
    );

    let default_texture = Texture::new_flat_color(&device, &queue, [255, 255, 255, 255], "default_texture")?;
    let mut materials = SlotMap::with_key();
    let dm = Material::new(&device, &default_texture);
    let default_material = materials.insert(dm);

    log::info!("Render pipeline created");

    Ok(Self {
      surface,
      aspect,
      device,
      queue,
      surf_config,
      render_pipe,

      depth_texture_view,
      default_texture,
      default_material,

      start_time: Instant::now(),
      is_surface_configured: true,
      frame_uniform,
      frame_uniform_buffer,
      frame_cam_bind_group,
      camera_uniform,
      camera_uniform_buffer,

      nodes: SlotMap::with_key(),
      scenes: SlotMap::with_key(),
      meshes: SlotMap::with_key(),
      materials,
      textures: SlotMap::with_key(),
    })
  }

  pub(crate) fn get_device(&self) -> &wgpu::Device {
    &self.device
  }

  pub(crate) fn get_queue(&self) -> &wgpu::Queue {
    &self.queue
  }

  pub fn default_texture(&self) -> &Texture {
    &self.default_texture
  }

  pub fn default_material(&self) -> MaterialHandle {
    self.default_material
  }

  pub fn resize(&mut self, new_size: crate::common::Size) {
    if new_size.width > 0 && new_size.height > 0 {
      self.surf_config.width = new_size.width;
      self.surf_config.height = new_size.height;
      self.surface.configure(&self.device, &self.surf_config);
      let (_depth_texture, depth_texture_view) = wgpu_helper::create_depth_texture(&self.device, &self.surf_config);
      self.depth_texture_view = depth_texture_view;
      self.aspect = new_size.width as f32 / new_size.height as f32;
      self.is_surface_configured = true;
    }
  }

  // TODO: This is all hard coded crap while weare figuring out how to do a scene graph and testing crap
  pub fn update(&mut self) {
    self.frame_uniform.time = self.start_time.elapsed().as_secs_f32();
    self.queue.write_buffer(&self.frame_uniform_buffer, 0, bytemuck::cast_slice(&[self.frame_uniform]));
  }

  pub fn t(&self) -> f32 {
    self.frame_uniform.time
  }

  pub fn render(&mut self, scene: SceneHandle, camera_node: Node3DHandle) -> anyhow::Result<()> {
    // We can't render unless the surface is configured
    if !self.is_surface_configured {
      return Ok(());
    }

    for material in self.materials.values_mut() {
      material.upload_gpu(&self.queue);
    }

    // Begin rendering commands
    let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Render Encoder") });

    // Root node for rendering all nodes in this scene
    let root = *self.scenes.get(scene).expect("invalid scene");

    // List of nodes to render, with their world matrices. We will fill this by traversing the scene graph.
    let mut render_list: Vec<Node3DHandle> = Vec::new();
    // Stack starts with the root node and identity world matrix
    let mut stack = vec![(root, Mat4::IDENTITY)];
    let mut camera_found = false;

    // This stack approach is a depth-first traversal of the scene graph, without the need for recursion.
    while let Some((node_hdl, parent_world)) = stack.pop() {
      // Calculate this node's world matrix and set/cache it in the node
      let world = parent_world * self.nodes[node_hdl].local_matrix();
      self.nodes[node_hdl].set_world_matrix(world);
      let node = &self.nodes[node_hdl];

      // Handle camera node specially, have to do this here after world matrix is set
      if node_hdl == camera_node {
        if let Some(vp) = node.view_proj(self.aspect) {
          self.camera_uniform.view_proj = vp.to_cols_array();
          self.queue.write_buffer(&self.camera_uniform_buffer, 0, bytemuck::cast_slice(&[self.camera_uniform]));
          camera_found = true;
        }
      }

      render_list.push(node_hdl);

      for &child in node.children() {
        stack.push((child, world));
      }
    }

    if !camera_found {
      log::warn!("camera node {camera_node:?} not found in scene (or node is not a camera); skipping frame");
      return Ok(());
    }

    for &node_hdl in &render_list {
      self.nodes[node_hdl].upload_world_mat(&self.queue);
    }

    // Get the current texture from the surface and create a view for rendering
    let output = match self.surface.get_current_texture() {
      // Both hand back a usable texture.
      wgpu::CurrentSurfaceTexture::Success(surface_texture) | wgpu::CurrentSurfaceTexture::Suboptimal(surface_texture) => surface_texture,

      wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded | wgpu::CurrentSurfaceTexture::Validation => {
        return Ok(());
      }

      wgpu::CurrentSurfaceTexture::Outdated => {
        self.surface.configure(&self.device, &self.surf_config);
        return Ok(());
      }

      wgpu::CurrentSurfaceTexture::Lost => {
        anyhow::bail!("Lost device");
      }
    };

    let view = output.texture.create_view(&wgpu::TextureViewDescriptor {
      format: Some(self.surf_config.format.add_srgb_suffix()),
      ..Default::default()
    });

    // Whole render pass is contained in this block, so it is dropped (releasing
    // its &mut borrow of the encoder) before we call encoder.finish() below.
    {
      let mut render_pass = wgpu_helper::create_render_pass(&mut encoder, &view, Some(&self.depth_texture_view));
      render_pass.set_pipeline(&self.render_pipe);
      render_pass.set_bind_group(0, &self.frame_cam_bind_group, &[]);

      // Walk the scene graph: each mesh-carrying node draws its meshes, looking up each
      // mesh's material by handle from the engine's arenas.
      for node in render_list.iter().map(|hdl| &self.nodes[*hdl]) {
        render_pass.set_bind_group(2, node.get_bind_group(), &[]);

        for &mesh_handle in node.mesh_handles() {
          let mesh = self.meshes.get(mesh_handle).expect("Invalid mesh handle");
          let material = self.materials.get(mesh.material_handle()).expect("Invalid material handle");

          render_pass.set_bind_group(1, material.get_bind_group(), &[]);
          render_pass.set_vertex_buffer(0, mesh.vertex_buffer().slice(..));
          render_pass.set_index_buffer(mesh.index_buffer().slice(..), wgpu::IndexFormat::Uint16);
          render_pass.draw_indexed(0..mesh.num_indices(), 0, 0..1);
        }
      }
    }

    // Submit the encoded commands on the queue & present the output texture to the surface
    self.queue.submit([encoder.finish()]);
    output.present();

    Ok(())
  }

  // ===== Resource management =====================================================================

  pub fn create_scene(&mut self) -> (SceneHandle, Node3DHandle) {
    let root_node = Node3D::new(&self.device, glam::Vec3::ZERO, glam::Quat::IDENTITY, glam::Vec3::new(1.0, 1.0, 1.0));
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

  pub fn create_material(&mut self, texture: Option<TextureHandle>) -> MaterialHandle {
    if let Some(texture_handle) = texture {
      let texture = self.textures.get(texture_handle).expect("Invalid texture handle");
      let material = Material::new(&self.device, texture);
      let handle = self.materials.insert(material);
      log::info!("Created material with handle {:?}", handle);
      handle
    } else {
      let material = Material::new(&self.device, &self.default_texture);
      let handle = self.materials.insert(material);
      log::info!("Created default material with handle {:?}", handle);
      handle
    }
  }

  pub(crate) fn add_mesh(&mut self, mesh: Mesh) -> MeshHandle {
    let handle = self.meshes.insert(mesh);
    log::info!("Added mesh with handle {:?}", handle);
    handle
  }

  pub fn create_node(&mut self, parent: Node3DHandle, position: Vec3, rotation: Quat, scale: Vec3) -> Node3DHandle {
    let node = Node3D::new(&self.device, position, rotation, scale);
    self.attach(node, parent)
  }

  pub fn create_mesh_node(&mut self, parent: Node3DHandle, meshes: Vec<MeshHandle>, position: Vec3, rotation: Quat, scale: Vec3) -> Node3DHandle {
    let node = Node3D::new_mesh(&self.device, meshes, position, rotation, scale);
    self.attach(node, parent)
  }

  pub fn create_camera_node(&mut self, parent: Node3DHandle, position: Vec3, look_at: Vec3, scale: Vec3, fovy: f32, znear: f32, zfar: f32) -> Node3DHandle {
    let node = Node3D::new_camera(&self.device, position, look_at, scale, fovy, znear, zfar);
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
