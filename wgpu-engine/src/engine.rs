mod gpu;
mod render;
mod resources;

use glam::{Mat4, Quat, Vec3};
use slotmap::{SlotMap, new_key_type};
use web_time::Instant;

use crate::Node3D;
use crate::common::Size;
use crate::helpers;
use crate::models::{Material, Mesh, Texture, Vertex};
use gpu::{create_depth_texture, create_pipeline, init};

use bytemuck::Zeroable;
pub use resources::*;
use wgpu::util::DeviceExt;

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

const MAX_LIGHTS: usize = 16;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct LightUniform {
  position: [f32; 3],
  intensity: f32, // fills the w slot so position+intensity = one 16-byte vec4
  color: [f32; 3],
  _padding: f32, // pads color up to a full 16-byte vec4
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct LightsUniform {
  count: u32,
  _padding: [u32; 3], // push the array to offset 16 (array align = 16)
  lights: [LightUniform; MAX_LIGHTS],
}

impl LightsUniform {
  fn new() -> Self {
    Self {
      count: 0,
      _padding: [0; 3],
      lights: [LightUniform::zeroed(); MAX_LIGHTS],
    }
  }

  fn add_light(&mut self, light_data: &crate::nodes::LightData, world_pos: Vec3) {
    if self.count as usize >= MAX_LIGHTS {
      log::warn!("Maximum number of lights ({}) exceeded, ignoring additional lights", MAX_LIGHTS);
      return;
    }

    let idx = self.count as usize;
    self.lights[idx] = LightUniform {
      position: [world_pos.x, world_pos.y, world_pos.z],
      intensity: light_data.intensity,
      color: [light_data.color.x, light_data.color.y, light_data.color.z],
      _padding: 0.0,
    };
    self.count += 1;
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

  // BG bind group for frame-level uniforms (camera, frame time, etc)
  frame_uniform: FrameUniform,
  frame_uniform_buffer: wgpu::Buffer,
  camera_uniform: CameraUniform,
  camera_uniform_buffer: wgpu::Buffer,
  frame_cam_bind_group: wgpu::BindGroup,
  lights_uniform: LightsUniform,
  lights_uniform_buffer: wgpu::Buffer,
  lights_bind_group: wgpu::BindGroup,

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
    let (surface, device, queue, surf_config) = init(size, surface_target).await?;

    // Step 2 - Create bind group layouts for frame-camera uniforms, lights, materials and node (model)
    let frame_cam_bg_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
      label: Some("Uniform Bind Group Layout"),
      entries: &[
        helpers::uniform_entry(0, wgpu::ShaderStages::VERTEX),          // camera uniform
        helpers::uniform_entry(1, wgpu::ShaderStages::VERTEX_FRAGMENT), // time uniform
      ],
    });
    let lights_bg_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
      label: Some("Lights Bind Group Layout"),
      entries: &[helpers::uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
    });
    let mat_bg_layout = Material::get_bind_group_layout(&device);
    let node_bg_layout = Node3D::get_bind_group_layout(&device);

    // Step 3 - Create the uniforms & buffers for frame and camera, and lights
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

    // Step 4 - Create the lights uniform, buffer and bind group
    let lights_uniform = LightsUniform::new();
    let lights_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
      label: Some("Lights Uniform Buffer"),
      contents: bytemuck::cast_slice(&[lights_uniform]),
      usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let lights_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
      label: Some("Lights Bind Group"),
      layout: &lights_bg_layout,
      entries: &[wgpu::BindGroupEntry {
        binding: 0,
        resource: lights_uniform_buffer.as_entire_binding(),
      }],
    });

    // Step 5 - Create the shaders & render pipeline
    let render_pipe = create_pipeline(
      &device,
      surf_config.format.add_srgb_suffix(),
      include_str!("../shaders/shader.wgsl"),
      &[Vertex::desc()],
      &[Some(&frame_cam_bg_layout), Some(&mat_bg_layout), Some(&node_bg_layout), Some(&lights_bg_layout)],
      true,
    );

    // Step 6 - Create a depth texture for Z-buffering 3D scenes, and a view for it
    let (_depth_texture, depth_texture_view) = create_depth_texture(&device, &surf_config);

    // Step 7 - Create a default texture and material
    let default_texture = Texture::new_flat_color(&device, &queue, [255, 255, 255, 255], "default_texture")?;
    let mut materials = SlotMap::with_key();
    let default_material = materials.insert(Material::new(&device, &default_texture));

    log::info!("Render pipeline created");

    // Return the engine with all the resources created and ready to go
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
      camera_uniform,
      camera_uniform_buffer,
      frame_cam_bind_group,
      lights_uniform,
      lights_uniform_buffer,
      lights_bind_group,

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
}
