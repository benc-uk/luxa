use std::sync::Arc;

use crate::camera;
use crate::models;
use crate::wgpu_helper;
use wgpu;
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

  // When rendering started, used to drive time-based animation.
  start_time: std::time::Instant,
  is_surface_configured: bool,
}

impl Renderer {
  pub async fn new(window: Arc<Window>) -> anyhow::Result<Self> {
    let size = window.inner_size();

    // Step 1 - Core instance, surface, device & queue creation
    let (surface, device, queue, surf_config) = wgpu_helper::init(size, window).await?;

    let (object_verts, object_indices) = models::example_quad();
    // Step 2 - Create the vertex buffer
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
      label: Some("Vertex Buffer"),
      contents: bytemuck::cast_slice(&object_verts),
      usage: wgpu::BufferUsages::VERTEX,
    });

    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
      label: Some("Index Buffer"),
      contents: bytemuck::cast_slice(&object_indices),
      usage: wgpu::BufferUsages::INDEX,
    });

    // Step 3 - Load the texture image and create a texture object
    let (texture_view, sampler) = wgpu_helper::create_texture_from_bytes(&device, &queue, include_bytes!("assets/texture.jpg"))?;
    // Create the bind group for the texture
    let (tex_bind_group, tex_bind_group_layout) = wgpu_helper::create_texture_bindgroup(&device, &texture_view, &sampler);

    // Add a camera for the scene; it owns its own uniform buffer and bind group.
    let camera = camera::Camera::new_default(&device, surf_config.width as f32 / surf_config.height as f32);

    // Step 4 - Create the shaders & render pipeline
    let render_pipe = wgpu_helper::create_pipeline(
      &device,
      &surf_config,
      include_str!("shaders/shader.wgsl"),
      &[crate::models::Vertex::desc()],
      &[Some(&tex_bind_group_layout), Some(camera.bind_group_layout())],
    );

    println!("Render pipeline created");

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

      start_time: std::time::Instant::now(),
      is_surface_configured: false,
    })
  }

  pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
    if new_size.width > 0 && new_size.height > 0 {
      self.surf_config.width = new_size.width;
      self.surf_config.height = new_size.height;
      self.surface.configure(&self.device, &self.surf_config);
      self.is_surface_configured = true;
    }
  }

  pub fn update(&mut self) {
    let t = self.start_time.elapsed().as_secs_f32();
    self.camera.eye.y = 1.0 + (t * 0.5).sin();
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

    let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

    // Begin rendering commands
    let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Render Encoder") });

    {
      let mut render_pass = wgpu_helper::create_render_pass(&mut encoder, &view);
      render_pass.set_pipeline(&self.render_pipe);
      render_pass.set_bind_group(0, &self.tex_bind_group, &[]);
      render_pass.set_bind_group(1, self.camera.bind_group(), &[]);
      render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
      render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
      render_pass.draw_indexed(0..self.num_indices, 0, 0..1); // Draw with the number of indices and 1 instance
    };

    // Submit the encoded commands on the queue
    self.queue.submit([encoder.finish()]);
    output.present();

    Ok(())
  }
}
