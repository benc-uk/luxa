// ============================================================================================
// IBL: image-based lighting resources baking, this is kinda funky and hard, about 60% AI
// ============================================================================================

use crate::engine::BindGroupLayouts;
use crate::engine::pipelines;
use crate::helpers;
use crate::models::{Cubemap, Texture};
use anyhow::Result;
use glam::Vec3;
use wgpu::util::DeviceExt;

const SHADER_IBL: &str = include_str!("../../shaders/bake_ibl.wgsl");

// Consts for controlling the overall IBL
const ENV_SIZE: u32 = 1024;
// Full mip chain for the env cube (1024 -> 1 = 11 mips). The prefilter reads progressively
// blurrier mips per sample to kill specular sparkle, so the env needs mips, not just mip 0.
const ENV_MIPS: u32 = ENV_SIZE.ilog2() + 1;
const IRRADIANCE_SIZE: u32 = 32;
const BRDF_LUT_SIZE: u32 = 512;
const PREFILTER_SIZE: u32 = 512;
const PREFILTER_MIPS: u32 = 5;

// Standard cubemap capture orientations in layer order: +X, -X, +Y, -Y, +Z, -Z.
const CUBE_FACE_ORIENTATIONS: [(Vec3, Vec3); 6] = [
  (Vec3::X, Vec3::NEG_Y),
  (Vec3::NEG_X, Vec3::NEG_Y),
  (Vec3::Y, Vec3::Z),
  (Vec3::NEG_Y, Vec3::NEG_Z),
  (Vec3::Z, Vec3::NEG_Y),
  (Vec3::NEG_Z, Vec3::NEG_Y),
];

// Per-face uniform for the bake: the inverse view-proj that turns a face's NDC into a
// world-space sample direction.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct FaceUniform {
  inv_view_proj: [f32; 16],
  roughness: f32, // only the prefilter bake reads this; others write 0.0
  _pad: [f32; 3],
}

// Environment-dependent textures and bind groups used by scene and skybox rendering.
struct IblResources {
  pub(crate) env_bind_group: wgpu::BindGroup,

  pub(crate) irradiance: Cubemap,
  pub(crate) irradiance_bind_group: wgpu::BindGroup,

  pub(crate) prefilter: Cubemap,
  pub(crate) prefilter_bind_group: wgpu::BindGroup,
}

impl IblResources {
  fn black(device: &wgpu::Device, queue: &wgpu::Queue, layouts: &BindGroupLayouts) -> Self {
    let env = Cubemap::new_render_target(device, 1, 1, wgpu::TextureFormat::Rgba16Float, "Black Environment Cube");
    let irradiance = Cubemap::new_render_target(device, 1, 1, wgpu::TextureFormat::Rgba16Float, "Black Irradiance Cube");
    let prefilter = Cubemap::new_render_target(device, 1, 1, wgpu::TextureFormat::Rgba16Float, "Black Prefilter Cube");

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Black IBL Clear") });
    for cubemap in [&env, &irradiance, &prefilter] {
      for face in 0..6 {
        fill_face(&mut encoder, cubemap, face, wgpu::Color::BLACK);
      }
    }
    queue.submit([encoder.finish()]);

    Self {
      env_bind_group: create_cubemap_bind_group(device, &layouts.env, &env.view, &env.sampler, "Black Environment Bind Group"),
      irradiance_bind_group: create_cubemap_bind_group(device, &layouts.env, &irradiance.view, &irradiance.sampler, "Black Irradiance Bind Group"),
      prefilter_bind_group: create_cubemap_bind_group(device, &layouts.env, &prefilter.view, &prefilter.sampler, "Black Prefilter Bind Group"),
      irradiance,
      prefilter,
    }
  }
}

// Device-owned bake infrastructure shared by every environment loaded by the engine.
struct IblBaker {
  face_layout: wgpu::BindGroupLayout,
  equirect_layout: wgpu::BindGroupLayout,
  equirect_pipeline: wgpu::RenderPipeline,
  irradiance_pipeline: wgpu::RenderPipeline,
  env_mip_pipeline: wgpu::RenderPipeline,
  prefilter_pipeline: wgpu::RenderPipeline,
  brdf_lut: Texture,
}

// Owns both the reusable bake infrastructure and the currently active IBL resources.
pub(crate) struct Ibl {
  baker: IblBaker,
  resources: IblResources,
}

impl Ibl {
  pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue, layouts: &BindGroupLayouts) -> Result<Self> {
    let baker = IblBaker::new(device, queue, layouts);
    let resources = IblResources::black(device, queue, layouts);
    Ok(Self { baker, resources })
  }

  pub(crate) fn set_environment(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, hdr_bytes: &[u8], layouts: &BindGroupLayouts) -> Result<()> {
    self.resources = self.baker.bake(device, queue, hdr_bytes, layouts)?;
    Ok(())
  }

  pub(crate) fn clear_environment(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, layouts: &BindGroupLayouts) {
    self.resources = IblResources::black(device, queue, layouts);
  }

  pub(crate) fn env_bind_group(&self) -> &wgpu::BindGroup {
    &self.resources.env_bind_group
  }

  pub(crate) fn irradiance_bind_group(&self) -> &wgpu::BindGroup {
    &self.resources.irradiance_bind_group
  }

  pub(crate) fn prefilter_bind_group(&self) -> &wgpu::BindGroup {
    &self.resources.prefilter_bind_group
  }

  pub(crate) fn irradiance(&self) -> &Cubemap {
    &self.resources.irradiance
  }

  pub(crate) fn prefilter(&self) -> &Cubemap {
    &self.resources.prefilter
  }

  pub(crate) fn brdf_lut(&self) -> &Texture {
    &self.baker.brdf_lut
  }
}

impl IblBaker {
  pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue, layouts: &BindGroupLayouts) -> Self {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
      label: Some("IBL Bake Shader"),
      source: wgpu::ShaderSource::Wgsl(SHADER_IBL.into()),
    });

    let face_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
      label: Some("IBL Face Layout"),
      entries: &[helpers::uniform_entry(0, wgpu::ShaderStages::FRAGMENT)],
    });

    let equirect_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
      label: Some("Equirect Source Layout"),
      entries: &[helpers::texture_entry(0), helpers::sampler_entry(1)],
    });

    let equirect_pipeline = create_bake_pipeline(
      device,
      &module,
      "frag_equirect",
      wgpu::TextureFormat::Rgba16Float,
      &[Some(&face_layout), None, Some(&equirect_layout)],
    );

    let irradiance_pipeline = create_bake_pipeline(
      device,
      &module,
      "frag_irradiance",
      wgpu::TextureFormat::Rgba16Float,
      &[Some(&face_layout), Some(&layouts.env)],
    );

    let env_mip_pipeline = create_bake_pipeline(
      device,
      &module,
      "frag_env_mips",
      wgpu::TextureFormat::Rgba16Float,
      &[Some(&face_layout), Some(&layouts.env)],
    );

    let prefilter_pipeline = create_bake_pipeline(
      device,
      &module,
      "frag_prefilter",
      wgpu::TextureFormat::Rgba16Float,
      &[Some(&face_layout), Some(&layouts.env)],
    );

    let brdf_lut_pipeline = create_bake_pipeline(device, &module, "frag_brdf_lut", wgpu::TextureFormat::Rg16Float, &[]);
    let brdf_lut = bake_brdf_lut(device, queue, &brdf_lut_pipeline);

    Self {
      face_layout,
      equirect_layout,
      equirect_pipeline,
      irradiance_pipeline,
      env_mip_pipeline,
      prefilter_pipeline,
      brdf_lut,
    }
  }

  // Bake an equirectangular HDR into the six faces of a fresh environment cube.
  fn bake(&self, device: &wgpu::Device, queue: &wgpu::Queue, hdr_bytes: &[u8], layouts: &BindGroupLayouts) -> Result<IblResources> {
    log::info!("Creating IBL from equirect HDR");

    // Destination cube we render the six faces into.
    let env = Cubemap::new_render_target(device, ENV_SIZE, ENV_MIPS, wgpu::TextureFormat::Rgba16Float, "Env Cube");

    // Transient equirect source: sampled during the bake, dropped when this fn returns.
    let src = Texture::new_equirect_hdr(device, queue, hdr_bytes, "Equirect HDR Source")?;

    // group 2 is the same equirect source for all six faces.
    let src_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
      label: Some("Equirect Source Bind Group"),
      layout: &self.equirect_layout,
      entries: &[helpers::bind_texture(0, &src), helpers::bind_sampler(1, &src)],
    });

    log::info!("Baking equirect HDR into env cube...");
    bake_faces(device, queue, &self.equirect_pipeline, &src_bind_group, 2, &self.face_layout, &env);

    Ok(self.bake_from_environment_cubemap(device, queue, env, layouts))
  }

  // Complete the environment-dependent bake after mip 0 of the environment cube has
  // been populated, then assemble the resources sampled by the renderer.
  fn bake_from_environment_cubemap(&self, device: &wgpu::Device, queue: &wgpu::Queue, env: Cubemap, layouts: &BindGroupLayouts) -> IblResources {
    // Give the env cube its full mip chain before anything samples it. The prefilter reads
    // progressively blurrier mips per sample (Krivanek/Colbert), which is what removes the
    // grainy specular speckle a single-mip env leaves behind.
    log::info!("Baking env cube mip chain...");
    bake_env_mips(device, queue, &self.env_mip_pipeline, &self.face_layout, &layouts.env, &env);

    let env_bind_group = create_cubemap_bind_group(device, &layouts.env, &env.view, &env.sampler, "Env Cube Bind Group");

    // --- 5b: bake the diffuse irradiance cube by convolving env over the hemisphere. ---
    let irradiance = Cubemap::new_render_target(device, IRRADIANCE_SIZE, 1, wgpu::TextureFormat::Rgba16Float, "Irradiance Cube");

    log::info!("Baking irradiance from env cube...");
    bake_faces(device, queue, &self.irradiance_pipeline, &env_bind_group, 1, &self.face_layout, &irradiance);

    let irradiance_bind_group = create_cubemap_bind_group(device, &layouts.env, &irradiance.view, &irradiance.sampler, "Irradiance Cube Bind Group");

    let prefilter = Cubemap::new_render_target(device, PREFILTER_SIZE, PREFILTER_MIPS, wgpu::TextureFormat::Rgba16Float, "Prefilter Cube");
    log::info!("Baking prefiltered specular env cube...");
    bake_prefilter(device, queue, &self.prefilter_pipeline, &self.face_layout, &env_bind_group, &prefilter);

    let prefilter_bind_group = create_cubemap_bind_group(device, &layouts.env, &prefilter.view, &prefilter.sampler, "Prefilter Cube Bind Group");

    IblResources {
      env_bind_group,
      irradiance,
      irradiance_bind_group,
      prefilter,
      prefilter_bind_group,
    }
  }
}

fn create_bake_pipeline(
  device: &wgpu::Device,
  module: &wgpu::ShaderModule,
  fragment_entry_point: &str,
  target_format: wgpu::TextureFormat,
  bind_group_layouts: &[Option<&wgpu::BindGroupLayout>],
) -> wgpu::RenderPipeline {
  pipelines::create_pipeline_with_entry_points(device, module, "vert_main", fragment_entry_point, target_format, &[], bind_group_layouts, false, None, None)
}

fn create_cubemap_bind_group(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, view: &wgpu::TextureView, sampler: &wgpu::Sampler, label: &str) -> wgpu::BindGroup {
  device.create_bind_group(&wgpu::BindGroupDescriptor {
    label: Some(label),
    layout,
    entries: &[
      wgpu::BindGroupEntry {
        binding: 0,
        resource: wgpu::BindingResource::TextureView(view),
      },
      wgpu::BindGroupEntry {
        binding: 1,
        resource: wgpu::BindingResource::Sampler(sampler),
      },
    ],
  })
}

fn face_uniforms(roughness: f32) -> [FaceUniform; 6] {
  let projection = glam::camera::rh::proj::directx::perspective(std::f32::consts::FRAC_PI_2, 1.0, 0.1, 10.0);

  CUBE_FACE_ORIENTATIONS.map(|(forward, up)| {
    let view = glam::camera::rh::view::look_to_mat4(Vec3::ZERO, forward, up);
    FaceUniform {
      inv_view_proj: (projection * view).inverse().to_cols_array(),
      roughness,
      _pad: [0.0; 3],
    }
  })
}

// Fill one cubemap face with a solid colour.
fn fill_face(encoder: &mut wgpu::CommandEncoder, cube: &crate::models::Cubemap, face: usize, color: wgpu::Color) {
  // The pass is dropped at the end of each iteration; dropping records the clear.
  let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
    label: Some("Cubemap Face Clear"),
    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
      view: cube.face_view(0, face),
      resolve_target: None,
      depth_slice: None,
      ops: wgpu::Operations {
        load: wgpu::LoadOp::Clear(color),
        store: wgpu::StoreOp::Store,
      },
    })],
    depth_stencil_attachment: None,
    occlusion_query_set: None,
    timestamp_writes: None,
    multiview_mask: None,
  });
}

// Render a fullscreen-triangle pass into all six faces of mip 0. The caller chooses the
// source bind-group index because equirect and cubemap sources use different WGSL groups.
fn bake_faces(
  device: &wgpu::Device,
  queue: &wgpu::Queue,
  pipeline: &wgpu::RenderPipeline,
  src_bind_group: &wgpu::BindGroup,
  src_bind_group_index: u32,
  face_layout: &wgpu::BindGroupLayout,
  dst: &Cubemap,
) {
  let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Cube Face Bake") });

  for (face, face_uniform) in face_uniforms(0.0).into_iter().enumerate() {
    // One buffer per face on purpose: reusing a single buffer + queue.write_buffer
    // across passes in one submit would leave every pass reading the LAST matrix.
    // create_buffer_init writes at creation, so each face gets its own value.
    let face_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
      label: Some("Bake Face Uniform"),
      contents: bytemuck::cast_slice(&[face_uniform]),
      usage: wgpu::BufferUsages::UNIFORM,
    });

    let face_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
      label: Some("Bake Face Bind Group"),
      layout: face_layout,
      entries: &[helpers::bind_buffer(0, &face_buffer)],
    });

    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
      label: Some("Bake Face Pass"),
      color_attachments: &[Some(wgpu::RenderPassColorAttachment {
        view: dst.face_view(0, face),
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
    });

    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, &face_bind_group, &[]);
    pass.set_bind_group(src_bind_group_index, src_bind_group, &[]);
    pass.draw(0..3, 0..1);
  }

  queue.submit([encoder.finish()]);
}

// Bake the BRDF integration LUT (5d): one fullscreen pass into a 2D Rg16Float texture,
// no inputs (pure maths). x = NdotV, y = roughness; texel = (scale, bias) for F0.
// View- and environment-independent, so this is the same table for every scene.
fn bake_brdf_lut(device: &wgpu::Device, queue: &wgpu::Queue, pipeline: &wgpu::RenderPipeline) -> Texture {
  let lut = Texture::new_render_target(device, BRDF_LUT_SIZE, BRDF_LUT_SIZE, wgpu::TextureFormat::Rg16Float, "BRDF LUT");

  let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("BRDF LUT Bake") });
  {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
      label: Some("BRDF LUT Pass"),
      color_attachments: &[Some(wgpu::RenderPassColorAttachment {
        view: &lut.view,
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
    });
    pass.set_pipeline(pipeline);
    pass.draw(0..3, 0..1);
  }
  queue.submit([encoder.finish()]);

  lut
}

// Fill the env cube's mip chain (mips 1..N) by downsampling each parent mip in turn. wgpu
// has no generateMipmap, so we do it by hand: for every destination mip we bind a Cube view
// of just the parent mip and re-sample it per face through the capture projection. Rendering
// mip N while sampling mip N-1 of the same texture is safe - they're disjoint subresources,
// and wgpu barriers the read-after-write between passes in this one encoder.
fn bake_env_mips(
  device: &wgpu::Device,
  queue: &wgpu::Queue,
  pipeline: &wgpu::RenderPipeline,
  face_layout: &wgpu::BindGroupLayout,
  env_layout: &wgpu::BindGroupLayout,
  env: &Cubemap,
) {
  if env.mip_level_count <= 1 {
    return;
  }

  let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
    label: Some("Env Mip Chain Bake"),
  });
  let face_uniforms = face_uniforms(0.0);

  for dst_mip in 1..env.mip_level_count {
    let src_mip = dst_mip - 1;

    // A Cube view of only the parent mip, so the sampled subresource never overlaps the mip
    // we're writing into this pass.
    let src_view = env.texture.create_view(&wgpu::TextureViewDescriptor {
      label: Some("Env Mip Source Cube View"),
      dimension: Some(wgpu::TextureViewDimension::Cube),
      base_mip_level: src_mip,
      mip_level_count: Some(1),
      ..Default::default()
    });

    let src_bind_group = create_cubemap_bind_group(device, env_layout, &src_view, &env.sampler, "Env Mip Source Bind Group");

    for (face, uniform) in face_uniforms.iter().enumerate() {
      let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Env Mip Face Uniform"),
        contents: bytemuck::bytes_of(uniform),
        usage: wgpu::BufferUsages::UNIFORM,
      });
      let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Env Mip Face Bind Group"),
        layout: face_layout,
        entries: &[helpers::bind_buffer(0, &buffer)],
      });

      let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("Env Mip Face Pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
          view: env.face_view(dst_mip as usize, face),
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
      });

      pass.set_pipeline(pipeline);
      pass.set_bind_group(0, &bind_group, &[]);
      pass.set_bind_group(1, &src_bind_group, &[]);
      pass.draw(0..3, 0..1);
    }
  }

  queue.submit([encoder.finish()]);
}

// Prefilter bake (5c): convolve the env cube with the GGX lobe once per roughness level,
// each into a mip of dst. Mip m uses roughness = m / (mips - 1): mip 0 mirror-sharp, last
// mip broad blur. Loops mip THEN face - the extra mip loop and per-mip roughness are why
// this can't reuse bake_faces (single-mip, inv_view_proj only). Source is the env cube.
fn bake_prefilter(
  device: &wgpu::Device,
  queue: &wgpu::Queue,
  pipeline: &wgpu::RenderPipeline,
  face_layout: &wgpu::BindGroupLayout,
  src_bind_group: &wgpu::BindGroup,
  dst: &Cubemap,
) {
  let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Prefilter Bake") });

  // Nested loop: mip first, then face. Each mip has a different roughness, so we can't reuse bake_faces.
  for mip in 0..PREFILTER_MIPS {
    // roughness sweeps 0..1 across the mip chain.
    let roughness = mip as f32 / (PREFILTER_MIPS - 1) as f32;

    for (face, uniform) in face_uniforms(roughness).into_iter().enumerate() {
      // Fresh buffer per (mip, face): one shared buffer across passes in a single submit
      // would leave every pass reading the LAST value written.
      let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Prefilter Face Uniform"),
        contents: bytemuck::cast_slice(&[uniform]),
        usage: wgpu::BufferUsages::UNIFORM,
      });

      let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Prefilter Face Bind Group"),
        layout: face_layout,
        entries: &[helpers::bind_buffer(0, &buffer)],
      });

      let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("Prefilter Face Pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
          // The target is THIS mip's face view; wgpu renders at that mip's resolution.
          view: dst.face_view(mip as usize, face),
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
      });

      pass.set_pipeline(pipeline);
      pass.set_bind_group(0, &bind_group, &[]);
      pass.set_bind_group(1, src_bind_group, &[]);
      pass.draw(0..3, 0..1);
    }
  }

  queue.submit([encoder.finish()]);
}
