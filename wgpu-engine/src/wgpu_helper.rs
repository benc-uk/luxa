// ======================================================================================
// Helper and wrapper functions for wgpu
// ======================================================================================

// Initialization of wgpu with surface, device, queue and surface configuration
pub async fn init(
  size: crate::common::Size,
  surface_target: impl Into<wgpu::SurfaceTarget<'static>>,
) -> anyhow::Result<(wgpu::Surface<'static>, wgpu::Device, wgpu::Queue, wgpu::SurfaceConfiguration)> {
  // // Web canvases might report zero while layout settles, but GPU textures cannot be empty.
  // let width = size.width.max(1);
  // let height = size.height.max(1);

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

// Create a "standard" render pipeline from the given shader string
// Lots of defaults are used here, we could provide more options in the future if needed
pub fn create_pipeline(
  device: &wgpu::Device,
  target_format: wgpu::TextureFormat,
  shader_str: &str,
  vertex_layouts: &[wgpu::VertexBufferLayout<'static>],
  bind_group_layouts: &[Option<&wgpu::BindGroupLayout>],
  enable_depth: bool,
) -> wgpu::RenderPipeline {
  let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
    label: Some("Shader"),
    source: wgpu::ShaderSource::Wgsl(shader_str.into()),
  });

  let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
    label: Some("Render Pipeline Layout"),
    bind_group_layouts,
    immediate_size: 0,
  });

  let pipeline_desc = &wgpu::RenderPipelineDescriptor {
    label: Some("Render Pipeline"),
    layout: Some(&render_pipeline_layout),
    vertex: wgpu::VertexState {
      module: &shader,
      entry_point: Some("vert_main"), // Hard coded for now
      buffers: vertex_layouts,
      compilation_options: wgpu::PipelineCompilationOptions::default(),
    },
    fragment: Some(wgpu::FragmentState {
      module: &shader,
      entry_point: Some("frag_main"), // Hard coded for now
      targets: &[Some(wgpu::ColorTargetState {
        format: target_format,
        blend: Some(wgpu::BlendState::REPLACE),
        write_mask: wgpu::ColorWrites::ALL,
      })],
      compilation_options: wgpu::PipelineCompilationOptions::default(),
    }),
    primitive: wgpu::PrimitiveState {
      topology: wgpu::PrimitiveTopology::TriangleList,
      strip_index_format: None,
      front_face: wgpu::FrontFace::Ccw,
      cull_mode: Some(wgpu::Face::Back),
      polygon_mode: wgpu::PolygonMode::Fill,
      unclipped_depth: false,
      conservative: false,
    },
    depth_stencil: if enable_depth {
      Some(wgpu::DepthStencilState {
        format: wgpu::TextureFormat::Depth32Float,
        depth_write_enabled: Some(true),
        depth_compare: Some(wgpu::CompareFunction::Less),
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
      })
    } else {
      None
    },
    multisample: wgpu::MultisampleState {
      count: 1,
      mask: !0,
      alpha_to_coverage_enabled: false,
    },
    multiview_mask: None,
    cache: None,
  };

  device.create_render_pipeline(pipeline_desc)
}

// Create a render pass with the given encoder and texture view
pub fn create_render_pass<'a>(encoder: &'a mut wgpu::CommandEncoder, view: &wgpu::TextureView, depth_texture_view: Option<&wgpu::TextureView>) -> wgpu::RenderPass<'a> {
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

// Common bind group layout entry constructors — the boring fields are filled in for you.

pub fn uniform_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
  wgpu::BindGroupLayoutEntry {
    binding,
    visibility,
    ty: wgpu::BindingType::Buffer {
      ty: wgpu::BufferBindingType::Uniform,
      has_dynamic_offset: false,
      min_binding_size: None,
    },
    count: None,
  }
}

pub fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
  wgpu::BindGroupLayoutEntry {
    binding,
    visibility: wgpu::ShaderStages::FRAGMENT,
    ty: wgpu::BindingType::Texture {
      multisampled: false,
      view_dimension: wgpu::TextureViewDimension::D2,
      sample_type: wgpu::TextureSampleType::Float { filterable: true },
    },
    count: None,
  }
}

pub fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
  wgpu::BindGroupLayoutEntry {
    binding,
    visibility: wgpu::ShaderStages::FRAGMENT,
    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
    count: None,
  }
}

pub fn create_depth_texture(device: &wgpu::Device, surf_config: &wgpu::SurfaceConfiguration) -> (wgpu::Texture, wgpu::TextureView) {
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
