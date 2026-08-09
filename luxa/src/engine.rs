mod gpu;
mod ibl;
mod lighting;
mod pipelines;
mod render;
mod resources;
mod skybox;

use glam::Mat4;
use slotmap::SlotMap;
use web_time::Instant;

use crate::models::{Material, MaterialFallbacks, Mesh, Texture, Vertex};
use crate::nodes::Node;
use crate::scenes::Scene;
use gpu::{create_depth_texture, init};
use ibl::Ibl;
pub(crate) use lighting::LightsUniform;
pub use skybox::SkyboxMode;

use pipelines::Pipelines;
use render::BindGroupLayouts;
pub use resources::{MaterialHandle, MeshHandle, NodeHandle, SceneHandle, TextureHandle};
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
  inv_view_proj: [f32; 16],
  pos: [f32; 3],
  _padding: f32,
}

impl CameraUniform {
  fn new() -> Self {
    Self {
      view_proj: Mat4::IDENTITY.to_cols_array(),
      pos: [0.0, 0.0, 0.0],
      inv_view_proj: Mat4::IDENTITY.to_cols_array(),
      _padding: 0.0,
    }
  }
}

const SHADER_MAIN: &str = include_str!("../shaders/render_main.wgsl");

pub struct Engine {
  surface: wgpu::Surface<'static>,
  aspect: f32,
  device: wgpu::Device,
  queue: wgpu::Queue,
  surf_config: wgpu::SurfaceConfiguration,
  pipelines: Pipelines,

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

  // Image-based lighting resources and environment bake state
  ibl: Ibl,
  skybox: skybox::Skybox,

  // Arenas for storing resources, so we can return handles to them.
  scenes: SlotMap<SceneHandle, Scene>,
  nodes: SlotMap<NodeHandle, Node>,
  meshes: SlotMap<MeshHandle, Mesh>,
  materials: SlotMap<MaterialHandle, Material>,
  textures: SlotMap<TextureHandle, Texture>,
}

impl Engine {
  pub async fn new(surface_target: impl Into<wgpu::SurfaceTarget<'static>>, size: (u32, u32)) -> anyhow::Result<Self> {
    log::info!("Creating new Luxa engine...");
    let aspect = size.0 as f32 / size.1 as f32;

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

    // Step 5 - Create the IBL resources with their default environment.
    let ibl = Ibl::new(&device, &queue, &bind_group_layouts)?;

    let lights_bind_group = create_lights_bind_group(&device, &bind_group_layouts.lights, &lights_uniform_buffer, &ibl);

    // Step 6 - Create the shaders & render pipelines
    let target_format = surf_config.format.add_srgb_suffix(); // add_srgb_suffix is v important, otherwise it will not work on some platforms like web
    let pipelines = Pipelines::new(
      &device,
      SHADER_MAIN,
      target_format,
      &[Vertex::desc()],
      &[
        Some(&bind_group_layouts.frame_cam), // group 0
        Some(&bind_group_layouts.material),  // group 1
        Some(&bind_group_layouts.node),      // group 2
        Some(&bind_group_layouts.lights),    // group 3
      ],
    );

    // Step 7 - Create a depth texture for Z-buffering 3D scenes, and a view for it
    let (_depth_texture, depth_texture_view) = create_depth_texture(&device, &surf_config);

    // Step 8 - Create a default texture and material
    let mut textures = SlotMap::with_key();
    let mut materials = SlotMap::with_key();
    let material_fallbacks = MaterialFallbacks::new(&device, &queue, &mut textures)?;
    let default_material = materials.insert(Material::new(&device, &bind_group_layouts.material, &material_fallbacks, &textures));

    // Step 9 - Create the skybox renderer
    let skybox = skybox::Skybox::new(&device, &bind_group_layouts, target_format);

    log::info!("Luxa engine created successfully");

    // Return the engine with all the resources created and ready to go
    Ok(Self {
      surface,
      aspect,
      device,
      queue,
      surf_config,
      pipelines,

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

      // Env map stuff
      ibl,
      skybox,

      nodes: SlotMap::with_key(),
      scenes: SlotMap::with_key(),
      meshes: SlotMap::with_key(),
      materials,
      textures,
    })
  }

  #[cfg(target_arch = "wasm32")]
  pub async fn new_from_canvas(canvas: web_sys::HtmlCanvasElement, size: (u32, u32)) -> anyhow::Result<Self> {
    Self::new(wgpu::SurfaceTarget::Canvas(canvas), size).await
  }

  pub(crate) fn get_device(&self) -> &wgpu::Device {
    &self.device
  }

  pub fn default_material(&self) -> MaterialHandle {
    self.default_material
  }

  pub fn set_environment(&mut self, hdr_bytes: &[u8]) -> anyhow::Result<()> {
    self.ibl.set_environment(&self.device, &self.queue, hdr_bytes, &self.bind_group_layouts)?;
    self.lights_bind_group = create_lights_bind_group(&self.device, &self.bind_group_layouts.lights, &self.lights_uniform_buffer, &self.ibl);

    Ok(())
  }

  pub fn clear_environment(&mut self) {
    self.ibl.clear_environment(&self.device, &self.queue, &self.bind_group_layouts);
    self.lights_bind_group = create_lights_bind_group(&self.device, &self.bind_group_layouts.lights, &self.lights_uniform_buffer, &self.ibl);
  }

  pub fn skybox_set_mode(&mut self, mode: SkyboxMode, mip_level: f32) {
    self.skybox.set_mode(mode);
    self.skybox.set_mip_level(&self.queue, mip_level);
  }
}

fn create_lights_bind_group(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, lights_uniform_buffer: &wgpu::Buffer, ibl: &Ibl) -> wgpu::BindGroup {
  device.create_bind_group(&wgpu::BindGroupDescriptor {
    label: Some("Lights Bind Group"),
    layout,
    entries: &[
      wgpu::BindGroupEntry {
        binding: 0,
        resource: lights_uniform_buffer.as_entire_binding(),
      },
      wgpu::BindGroupEntry {
        binding: 1,
        resource: wgpu::BindingResource::TextureView(&ibl.irradiance().view),
      },
      wgpu::BindGroupEntry {
        binding: 2,
        resource: wgpu::BindingResource::Sampler(&ibl.irradiance().sampler),
      },
      wgpu::BindGroupEntry {
        binding: 3,
        resource: wgpu::BindingResource::TextureView(&ibl.prefilter().view),
      },
      wgpu::BindGroupEntry {
        binding: 4,
        resource: wgpu::BindingResource::Sampler(&ibl.prefilter().sampler),
      },
      wgpu::BindGroupEntry {
        binding: 5,
        resource: wgpu::BindingResource::TextureView(&ibl.brdf_lut().view),
      },
      wgpu::BindGroupEntry {
        binding: 6,
        resource: wgpu::BindingResource::Sampler(&ibl.brdf_lut().sampler),
      },
    ],
  })
}
