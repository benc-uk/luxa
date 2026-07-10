// ======================================================================================
// Helper and wrapper functions for wgpu
// ======================================================================================

use image::GenericImageView;
use std::sync::Arc;

// Initialization of wgpu with surface, device, queue and surface configuration
pub async fn init(
  size: winit::dpi::PhysicalSize<u32>,
  window: Arc<winit::window::Window>,
) -> anyhow::Result<(wgpu::Surface<'static>, wgpu::Device, wgpu::Queue, wgpu::SurfaceConfiguration)> {
  // The instance helps us access the graphics card and create surfaces for rendering
  let instance = wgpu::Instance::default();
  let surface = instance.create_surface(window.clone()).unwrap();
  println!("Surface created for window");

  // Create the adapter which is kinda like a handle to the actual graphics card
  let adapter = instance
    .request_adapter(&wgpu::RequestAdapterOptions {
      compatible_surface: Some(&surface),
      ..Default::default()
    })
    .await?;

  let info = adapter.get_info();
  println!("Using adapter: {} ({:?})", info.name, info.backend);

  let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default()).await?;

  let surface_caps = surface.get_capabilities(&adapter);
  println!("Surface capabilities: {:?}", surface_caps);

  // Use get_default_config to make life easier
  let surf_config = surface.get_default_config(&adapter, size.width, size.height).unwrap();
  surface.configure(&device, &surf_config);

  Ok((surface, device, queue, surf_config))
}

// Create a "standard" render pipeline from the given shader string
// Lots of defaults are used here, we could provide more options in the future if needed
pub fn create_pipeline(
  device: &wgpu::Device,
  surf_config: &wgpu::SurfaceConfiguration,
  shader_str: &str,
  vertex_layouts: &[wgpu::VertexBufferLayout<'static>],
  bind_groups: &[Option<&wgpu::BindGroupLayout>],
) -> wgpu::RenderPipeline {
  let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
    label: Some("Shader"),
    source: wgpu::ShaderSource::Wgsl(shader_str.into()),
  });

  let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
    label: Some("Render Pipeline Layout"),
    bind_group_layouts: bind_groups,
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
        format: surf_config.format,
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
    depth_stencil: None,
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
pub fn create_render_pass<'a>(encoder: &'a mut wgpu::CommandEncoder, view: &wgpu::TextureView) -> wgpu::RenderPass<'a> {
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
    depth_stencil_attachment: None,
    occlusion_query_set: None,
    timestamp_writes: None,
    multiview_mask: None,
  })
}

pub fn create_texture_from_bytes(device: &wgpu::Device, queue: &wgpu::Queue, bytes: &[u8]) -> anyhow::Result<(wgpu::TextureView, wgpu::Sampler)> {
  let img = image::load_from_memory(bytes)?;
  let rgba = img.to_rgba8();
  let bytes_rgba = rgba.as_raw();
  let dimensions = img.dimensions();

  let size = wgpu::Extent3d {
    width: dimensions.0,
    height: dimensions.1,
    depth_or_array_layers: 1,
  };

  let texture = device.create_texture(&wgpu::TextureDescriptor {
    label: Some("Texture"),
    size,
    mip_level_count: 1,
    sample_count: 1,
    dimension: wgpu::TextureDimension::D2,
    format: wgpu::TextureFormat::Rgba8UnormSrgb,
    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
    view_formats: &[],
  });

  queue.write_texture(
    wgpu::TexelCopyTextureInfo {
      texture: &texture,
      mip_level: 0,
      origin: wgpu::Origin3d::ZERO,
      aspect: wgpu::TextureAspect::All,
    },
    &bytes_rgba,
    wgpu::TexelCopyBufferLayout {
      offset: 0,
      bytes_per_row: Some(4 * dimensions.0),
      rows_per_image: Some(dimensions.1),
    },
    size,
  );

  let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
  let diffuse_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
    address_mode_u: wgpu::AddressMode::ClampToEdge,
    address_mode_v: wgpu::AddressMode::ClampToEdge,
    address_mode_w: wgpu::AddressMode::ClampToEdge,
    mag_filter: wgpu::FilterMode::Linear,
    min_filter: wgpu::FilterMode::Nearest,
    mipmap_filter: wgpu::MipmapFilterMode::Nearest,
    ..Default::default()
  });

  Ok((texture_view, diffuse_sampler))
}

pub fn create_texture_bindgroup(device: &wgpu::Device, texture_view: &wgpu::TextureView, sampler: &wgpu::Sampler) -> (wgpu::BindGroup, wgpu::BindGroupLayout) {
  let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
    label: Some("Texture Bind Group Layout"),
    entries: &[
      wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
          multisampled: false,
          view_dimension: wgpu::TextureViewDimension::D2,
          sample_type: wgpu::TextureSampleType::Float { filterable: true },
        },
        count: None,
      },
      wgpu::BindGroupLayoutEntry {
        binding: 1,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
      },
    ],
  });

  let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
    label: Some("Texture Bind Group"),
    layout: &bind_group_layout,
    entries: &[
      wgpu::BindGroupEntry {
        binding: 0,
        resource: wgpu::BindingResource::TextureView(&texture_view),
      },
      wgpu::BindGroupEntry {
        binding: 1,
        resource: wgpu::BindingResource::Sampler(&sampler),
      },
    ],
  });

  (bind_group, bind_group_layout)
}

pub fn create_uniform_bindgroup(device: &wgpu::Device, buffer: &wgpu::Buffer) -> (wgpu::BindGroup, wgpu::BindGroupLayout) {
  let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
    label: Some("Uniform Bind Group Layout"),
    entries: &[wgpu::BindGroupLayoutEntry {
      binding: 0,
      visibility: wgpu::ShaderStages::VERTEX,
      ty: wgpu::BindingType::Buffer {
        ty: wgpu::BufferBindingType::Uniform,
        has_dynamic_offset: false,
        min_binding_size: None,
      },
      count: None,
    }],
  });

  let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
    label: Some("Uniform Bind Group"),
    layout: &bind_group_layout,
    entries: &[wgpu::BindGroupEntry {
      binding: 0,
      resource: buffer.as_entire_binding(),
    }],
  });

  (bind_group, bind_group_layout)
}
