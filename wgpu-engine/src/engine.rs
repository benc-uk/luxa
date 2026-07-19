mod gpu;
mod lighting;
mod render;
mod resources;

use glam::Mat4;
use slotmap::SlotMap;
use web_time::Instant;

use crate::common::Size;
use crate::models::{Material, MaterialFallbacks, Mesh, Texture, Vertex};
use crate::nodes::Node3D;
use gpu::{create_depth_texture, create_pipeline, init};
pub(crate) use lighting::{LightUniform, LightsUniform};

use render::BindGroupLayouts;
pub use resources::{MaterialHandle, MeshHandle, Node3DHandle, SceneHandle, TextureHandle};
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
  pos: [f32; 3],
  _padding: f32, // pad to 16 bytes for alignment
}

impl CameraUniform {
  fn new() -> Self {
    Self {
      view_proj: Mat4::IDENTITY.to_cols_array(),
      pos: [0.0, 0.0, 0.0],
      _padding: 0.0,
    }
  }
}

const BRDF: &str = include_str!("../shaders/pbr.wgsl");
// const COMMON: &str = include_str!("../shaders/common.wgsl");
const MAIN: &str = include_str!("../shaders/shader.wgsl");

pub struct Engine {
  surface: wgpu::Surface<'static>,
  aspect: f32,
  device: wgpu::Device,
  queue: wgpu::Queue,
  surf_config: wgpu::SurfaceConfiguration,
  render_pipe: wgpu::RenderPipeline,

  // Depth buffer for 3D rendering
  depth_texture_view: wgpu::TextureView,
  material_fallbacks: MaterialFallbacks,
  default_material: MaterialHandle,

  // When rendering started, used to drive time-based animation.
  start_time: Instant,
  is_surface_configured: bool,

  // BG bind group for frame-level uniforms (camera, frame time, etc)
  bind_group_layouts: BindGroupLayouts,
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
    let bind_group_layouts = Self::init_bind_group_layouts(&device);

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
      layout: &bind_group_layouts.frame_cam,
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
      layout: &bind_group_layouts.lights,
      entries: &[wgpu::BindGroupEntry {
        binding: 0,
        resource: lights_uniform_buffer.as_entire_binding(),
      }],
    });

    // Step 5 - Create the shaders & render pipeline
    let render_pipe = create_pipeline(
      &device,
      surf_config.format.add_srgb_suffix(),
      &format!("{BRDF}\n{MAIN}").as_str(),
      &[Vertex::desc()],
      &[
        Some(&bind_group_layouts.frame_cam),
        Some(&bind_group_layouts.material),
        Some(&bind_group_layouts.node),
        Some(&bind_group_layouts.lights),
      ],
      true,
    );

    // Step 6 - Create a depth texture for Z-buffering 3D scenes, and a view for it
    let (_depth_texture, depth_texture_view) = create_depth_texture(&device, &surf_config);

    // Step 7 - Create a default texture and material
    let mut textures = SlotMap::with_key();
    let mut materials = SlotMap::with_key();
    let material_fallbacks = MaterialFallbacks::new(&device, &queue, &mut textures)?;
    let default_material = materials.insert(Material::new(&device, &bind_group_layouts.material, &material_fallbacks, &textures));

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
      material_fallbacks,
      default_material,

      start_time: Instant::now(),
      is_surface_configured: true,

      bind_group_layouts,
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
      textures,
    })
  }

  pub(crate) fn get_device(&self) -> &wgpu::Device {
    &self.device
  }

  pub fn default_material(&self) -> MaterialHandle {
    self.default_material
  }
}
