// ======================================================================================
// Core renderer for the application. All WGPU rendering is done here.
// ======================================================================================

use std::sync::Arc;

use crate::camera;
use crate::models;
use crate::platform::Instant;
use crate::wgpu_helper;
use wgpu::util::DeviceExt;
use winit::window::Window;

pub struct Renderer {
  surface: wgpu::Surface<'static>,
  device: wgpu::Device,
  queue: wgpu::Queue,
  surf_config: wgpu::SurfaceConfiguration,
  render_pipe: wgpu::RenderPipeline,

  // Objects, remove this to a separate struct later, but for now we can keep it here
  vertex_buffer: wgpu::Buffer,
  index_buffer: wgpu::Buffer,
  num_indices: u32,
  tex_bind_group: wgpu::BindGroup,

  // Add a camera for the scene
  camera: camera::Camera,

  // Depth buffer for 3D rendering
  depth_texture_view: wgpu::TextureView,

  // When rendering started, used to drive time-based animation.
  start_time: Instant,
  is_surface_configured: bool,
}

impl Renderer {
  pub async fn new(window: Arc<Window>, size: winit::dpi::PhysicalSize<u32>) -> anyhow::Result<Self> {
    // Step 1 - Core instance, surface, device & queue creation
    let (surface, device, queue, surf_config) = wgpu_helper::init(size, window).await?;

    // Load object data
    let (object_verts, object_indices) = models::primitive_cube();

    // Step 2 - Create the vertex buffer
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
      label: Some("Vertex Buffer"),
      contents: bytemuck::cast_slice(object_verts),
      usage: wgpu::BufferUsages::VERTEX,
    });

    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
      label: Some("Index Buffer"),
      contents: bytemuck::cast_slice(object_indices),
      usage: wgpu::BufferUsages::INDEX,
    });

    // Step 3 - Load the texture image and create a texture object
    let (texture_view, sampler) = wgpu_helper::create_texture_from_bytes(&device, &queue, include_bytes!("../assets/crate_wood.jpg"))?;
    // Create the bind group for the texture
    let (tex_bind_group, tex_bind_group_layout) = wgpu_helper::create_texture_bindgroup(&device, &texture_view, &sampler);

    // Step 4 - Add a camera for the scene; it owns its own uniform buffer and bind group.
    let aspect = surf_config.width as f32 / surf_config.height as f32;
    let camera = camera::Camera::new(&device, glam::vec3(0.0, 1.5, 2.5), glam::vec3(0.0, 0.0, 0.0), aspect);

    // Step 5 - Create a depth texture for 3D rendering
    let (_depth_texture, depth_texture_view) = wgpu_helper::create_depth_texture(&device, &surf_config);

    // Step 6 - Create the shaders & render pipeline
    let render_pipe = wgpu_helper::create_pipeline(
      &device,
      surf_config.format.add_srgb_suffix(),
      include_str!("../shaders/shader.wgsl"),
      &[crate::models::Vertex::desc()],
      &[Some(&tex_bind_group_layout), Some(camera.bind_group_layout())],
      true,
    );

    log::info!("Render pipeline created");

    Ok(Self {
      surface,
      device,
      queue,
      surf_config,
      render_pipe,

      vertex_buffer,
      index_buffer,
      num_indices: object_indices.len() as u32,
      tex_bind_group,
      camera,
      depth_texture_view,

      start_time: Instant::now(),
      is_surface_configured: true,
    })
  }

  pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
    if new_size.width > 0 && new_size.height > 0 {
      self.surf_config.width = new_size.width;
      self.surf_config.height = new_size.height;
      self.surface.configure(&self.device, &self.surf_config);
      let (_depth_texture, depth_texture_view) = wgpu_helper::create_depth_texture(&self.device, &self.surf_config);
      self.depth_texture_view = depth_texture_view;
      self.camera.set_aspect(new_size.width as f32 / new_size.height as f32);
      self.is_surface_configured = true;
    }
  }

  pub fn update(&mut self) {
    let t = self.start_time.elapsed().as_secs_f32();

    // rotate the camera around the origin in a circle
    let radius = 2.5;
    let cam_x = radius * t.cos();
    let cam_z = radius * t.sin();
    self.camera.set_position([cam_x, 1.5, cam_z]);

    self.camera.update(&self.queue);
  }

  pub fn render(&mut self) -> anyhow::Result<()> {
    // We can't render unless the surface is configured
    if !self.is_surface_configured {
      return Ok(());
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

    // Begin rendering commands
    let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Render Encoder") });

    {
      // Whole render pass is contained in this block
      let mut render_pass = wgpu_helper::create_render_pass(&mut encoder, &view, Some(&self.depth_texture_view));
      render_pass.set_pipeline(&self.render_pipe);
      render_pass.set_bind_group(0, &self.tex_bind_group, &[]);
      render_pass.set_bind_group(1, self.camera.bind_group(), &[]);
      render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
      render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
      render_pass.draw_indexed(0..self.num_indices, 0, 0..1);
    };

    // Submit the encoded commands on the queue & present the output texture to the surface
    self.queue.submit([encoder.finish()]);
    output.present();

    Ok(())
  }
}
