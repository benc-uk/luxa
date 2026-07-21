pub(crate) struct Pipelines {
  opaque: wgpu::RenderPipeline,        // cull Back,  blend REPLACE
  opaque_double: wgpu::RenderPipeline, // cull None,  blend REPLACE
  blend: wgpu::RenderPipeline,         // cull Back,  ALPHA_BLENDING
  blend_double: wgpu::RenderPipeline,  // cull None,  ALPHA_BLENDING
}

impl Pipelines {
  pub(crate) fn new(
    device: &wgpu::Device,
    shader_str: &str,
    target_format: wgpu::TextureFormat,
    vertex_layouts: &[wgpu::VertexBufferLayout<'static>],
    bind_group_layouts: &[Option<&wgpu::BindGroupLayout>],
  ) -> Self {
    let shader_module = &device.create_shader_module(wgpu::ShaderModuleDescriptor {
      label: Some("Shader"),
      source: wgpu::ShaderSource::Wgsl(shader_str.into()),
    });

    let opaque = create_pipeline(
      device,
      shader_module,
      target_format,
      vertex_layouts,
      bind_group_layouts,
      true, // enable depth
      Some(wgpu::Face::Back),
      None, // no blending
    );

    let opaque_double = create_pipeline(
      device,
      shader_module,
      target_format,
      vertex_layouts,
      bind_group_layouts,
      true, // enable depth
      None, // cull none
      None, // no blending
    );

    let blend = create_pipeline(
      device,
      shader_module,
      target_format,
      vertex_layouts,
      bind_group_layouts,
      true, // enable depth
      Some(wgpu::Face::Back),
      Some(wgpu::BlendState::ALPHA_BLENDING),
    );

    let blend_double = create_pipeline(
      device,
      shader_module,
      target_format,
      vertex_layouts,
      bind_group_layouts,
      true, // enable depth
      None, // cull none
      Some(wgpu::BlendState::ALPHA_BLENDING),
    );

    log::info!("Render pipelines created");

    Self {
      opaque,
      opaque_double,
      blend,
      blend_double,
    }
  }

  pub(crate) fn select(&self, blended: bool, double_sided: bool) -> &wgpu::RenderPipeline {
    match (blended, double_sided) {
      (false, false) => &self.opaque,
      (false, true) => &self.opaque_double,
      (true, false) => &self.blend,
      (true, true) => &self.blend_double,
    }
  }
}

// Create a "standard" render pipeline from the given shader string
// Lots of defaults are used here, we could provide more options in the future if needed
pub(crate) fn create_pipeline(
  device: &wgpu::Device,
  shader_module: &wgpu::ShaderModule,
  target_format: wgpu::TextureFormat,
  vertex_layouts: &[wgpu::VertexBufferLayout<'static>],
  bind_group_layouts: &[Option<&wgpu::BindGroupLayout>],
  enable_depth: bool,
  cull_mode: Option<wgpu::Face>,
  blend: Option<wgpu::BlendState>,
) -> wgpu::RenderPipeline {
  let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
    label: Some("Render Pipeline Layout"),
    bind_group_layouts,
    immediate_size: 0,
  });

  let depth_write_enabled = match blend {
    Some(_) => Some(false),
    None => Some(true),
  };

  let pipeline_desc = &wgpu::RenderPipelineDescriptor {
    label: Some("Render Pipeline"),
    layout: Some(&render_pipeline_layout),
    vertex: wgpu::VertexState {
      module: shader_module,
      entry_point: Some("vert_main"), // Hard coded for now
      buffers: vertex_layouts,
      compilation_options: wgpu::PipelineCompilationOptions::default(),
    },

    fragment: Some(wgpu::FragmentState {
      module: shader_module,
      entry_point: Some("frag_main"), // Hard coded for now
      targets: &[Some(wgpu::ColorTargetState {
        format: target_format,
        blend,
        write_mask: wgpu::ColorWrites::ALL,
      })],
      compilation_options: wgpu::PipelineCompilationOptions::default(),
    }),

    primitive: wgpu::PrimitiveState {
      topology: wgpu::PrimitiveTopology::TriangleList,
      strip_index_format: None,
      front_face: wgpu::FrontFace::Ccw,
      cull_mode,
      polygon_mode: wgpu::PolygonMode::Fill,
      unclipped_depth: false,
      conservative: false,
    },

    depth_stencil: if enable_depth {
      Some(wgpu::DepthStencilState {
        format: wgpu::TextureFormat::Depth32Float,
        depth_write_enabled,
        depth_compare: Some(wgpu::CompareFunction::LessEqual),
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
