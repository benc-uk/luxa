// Initialization of wgpu with surface, device, queue and surface configuration
pub(crate) async fn init(
  size: crate::common::Size,
  surface_target: impl Into<wgpu::SurfaceTarget<'static>>,
) -> anyhow::Result<(wgpu::Surface<'static>, wgpu::Device, wgpu::Queue, wgpu::SurfaceConfiguration)> {
  // The instance helps us access the graphics card and create surfaces for rendering
  let instance = wgpu::Instance::default();
  let surface = instance.create_surface(surface_target).expect("surface creation failed");
  log::debug!("Surface created for window");

  // Create the adapter which is kinda like a handle to the actual graphics card
  let adapter = instance
    .request_adapter(&wgpu::RequestAdapterOptions {
      compatible_surface: Some(&surface),
      ..Default::default()
    })
    .await?;

  let info = adapter.get_info();
  log::info!("Using adapter: {} ({:?})", info.name, info.backend);

  let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default()).await?;

  let surface_caps = surface.get_capabilities(&adapter);
  log::info!("Surface capabilities: {:?}", surface_caps);

  // Use get_default_config to make life easier
  let mut surf_config = surface.get_default_config(&adapter, size.width, size.height).expect("surface config failed");
  // Add the sRGB format to the list of view formats if it's not already there, so we can use it for rendering
  let render_format = surf_config.format.add_srgb_suffix();
  if render_format != surf_config.format {
    surf_config.view_formats.push(render_format);
  }

  surface.configure(&device, &surf_config);

  Ok((surface, device, queue, surf_config))
}

// Create a depth texture for Z-buffering 3D scenes, and a view for it
pub(crate) fn create_depth_texture(device: &wgpu::Device, surf_config: &wgpu::SurfaceConfiguration) -> (wgpu::Texture, wgpu::TextureView) {
  let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
    label: Some("Depth Texture"),
    size: wgpu::Extent3d {
      width: surf_config.width,
      height: surf_config.height,
      depth_or_array_layers: 1,
    },
    mip_level_count: 1,
    sample_count: 1,
    dimension: wgpu::TextureDimension::D2,
    format: wgpu::TextureFormat::Depth32Float,
    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
    view_formats: &[],
  });

  let depth_texture_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

  (depth_texture, depth_texture_view)
}

// Create a render pass with the given encoder and texture view
pub(crate) fn create_render_pass<'a>(encoder: &'a mut wgpu::CommandEncoder, view: &wgpu::TextureView, depth_texture_view: Option<&wgpu::TextureView>) -> wgpu::RenderPass<'a> {
  // Build the optional depth attachment up front so the descriptor below stays flat and readable.
  let depth_stencil_attachment = depth_texture_view.map(|depth_view| wgpu::RenderPassDepthStencilAttachment {
    view: depth_view,
    depth_ops: Some(wgpu::Operations {
      load: wgpu::LoadOp::Clear(1.0),
      store: wgpu::StoreOp::Store,
    }),
    stencil_ops: None,
  });

  encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
    label: Some("Render Pass"),
    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
      view,
      resolve_target: None,
      depth_slice: None,
      ops: wgpu::Operations {
        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
        store: wgpu::StoreOp::Store,
      },
    })],
    depth_stencil_attachment,
    occlusion_query_set: None,
    timestamp_writes: None,
    multiview_mask: None,
  })
}
