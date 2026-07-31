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

// Per-face uniform for the bake: the inverse view-proj that turns a face's NDC into a
// world-space sample direction.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct FaceUniform {
  inv_view_proj: [f32; 16],
  roughness: f32, // only the prefilter bake reads this; others write 0.0
  _pad: [f32; 3],
}

#[allow(dead_code)]
pub(crate) struct Ibl {
  pub(crate) env: Cubemap,
  pub(crate) env_bind_group: wgpu::BindGroup,

  pub(crate) irradiance: Cubemap,
  pub(crate) irradiance_bind_group: wgpu::BindGroup,

  pub(crate) brdf_lut: Texture,

  pub(crate) prefilter: Cubemap,
  pub(crate) prefilter_bind_group: wgpu::BindGroup,
}

impl Ibl {
  // Bake an equirectangular HDR into the six faces of a fresh environment cube.
  pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue, hdr_bytes: &[u8], layouts: &BindGroupLayouts) -> Result<Self> {
    log::info!("Creating IBL from equirect HDR");

    // Destination cube we render the six faces into.
    let env = Cubemap::new_render_target(device, ENV_SIZE, ENV_MIPS, wgpu::TextureFormat::Rgba16Float, "Env Cube");

    // Transient equirect source: sampled during the bake, dropped when this fn returns.
    let src = Texture::new_equirect_hdr(device, queue, hdr_bytes, "Equirect HDR Source")?;

    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
      label: Some("Equirect Bake Shader"),
      source: wgpu::ShaderSource::Wgsl(SHADER_IBL.into()),
    });

    // Bake-only layouts (not part of the shared BindGroupLayouts): group 0 = per-face
    // inverse view-proj uniform, group 2 = the 2D equirect source. Group 1 is reserved
    // for the cubemap source used by the other entry points in the combined module.
    let face_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
      label: Some("Equirect Face Layout"),
      entries: &[helpers::uniform_entry(0, wgpu::ShaderStages::FRAGMENT)],
    });

    let src_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
      label: Some("Equirect Source Layout"),
      entries: &[helpers::texture_entry(0), helpers::sampler_entry(1)],
    });

    // Fullscreen-triangle bake: no depth, no cull, no blend; target matches the cube.
    let pipeline = pipelines::create_pipeline_with_entry_points(
      device,
      &module,
      "vert_main",
      "frag_equirect",
      wgpu::TextureFormat::Rgba16Float,
      &[], // no vertex buffers
      &[Some(&face_layout), None, Some(&src_layout)],
      false, // no depth
      None,  // cull none
      None,  // no blend
    );

    // group 2 is the same equirect source for all six faces.
    let src_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
      label: Some("Equirect Source Bind Group"),
      layout: &src_layout,
      entries: &[helpers::bind_texture(0, &src), helpers::bind_sampler(1, &src)],
    });

    log::info!("Baking equirect HDR into env cube...");
    bake_faces(device, queue, &pipeline, &src_bind_group, 2, &face_layout, &env);

    Self::from_env(device, queue, env, layouts)
  }

  // Create a new IBL with a solid color for the environment cube. This is useful for testing and debugging only
  pub(crate) fn new_debug(device: &wgpu::Device, queue: &wgpu::Queue, layouts: &BindGroupLayouts) -> Result<Self> {
    let env = Cubemap::new_render_target(device, ENV_SIZE, ENV_MIPS, wgpu::TextureFormat::Rgba16Float, "Env Cube");

    // Fill each face with a different color so we can see the orientation of the cube in the skybox.
    // Axis-coded so orientation bugs are obvious: bright = +, dim = -.
    const FACE_COLOURS: [wgpu::Color; 6] = [
      wgpu::Color { r: 0.8, g: 0.1, b: 0.1, a: 1.0 }, // +X bright red
      wgpu::Color { r: 0.3, g: 0.0, b: 0.0, a: 1.0 }, // -X dim red
      wgpu::Color { r: 0.1, g: 0.8, b: 0.1, a: 1.0 }, // +Y bright green
      wgpu::Color { r: 0.0, g: 0.3, b: 0.0, a: 1.0 }, // -Y dim green
      wgpu::Color { r: 0.1, g: 0.1, b: 0.8, a: 1.0 }, // +Z bright blue
      wgpu::Color { r: 0.0, g: 0.0, b: 0.3, a: 1.0 }, // -Z dim blue
    ];

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Cube face fill") });
    for face in 0..6usize {
      let color = FACE_COLOURS[face];
      fill_face(&mut encoder, &env, face, color);
    }
    queue.submit([encoder.finish()]);

    Self::from_env(device, queue, env, layouts)
  }

  // Create an "empty" IBL with a solid color for the environment cube. This is useful for having a default IBL when no HDR is provided
  pub(crate) fn new_solid_color(device: &wgpu::Device, queue: &wgpu::Queue, layouts: &BindGroupLayouts, color: wgpu::Color) -> Result<Self> {
    let env = Cubemap::new_render_target(device, ENV_SIZE, ENV_MIPS, wgpu::TextureFormat::Rgba16Float, "Env Cube");

    // Fill each face with the same color.
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Env Cube Fill") });
    for face in 0..6 {
      fill_face(&mut encoder, &env, face, color);
    }

    queue.submit([encoder.finish()]);

    Self::from_env(device, queue, env, layouts)
  }

  // Shared tail for all three constructors: once env's six faces are filled, build the
  // runtime bind group the skybox samples and assemble the Ibl. Step 5b will grow this
  // to also bake the irradiance cube from env.
  fn from_env(device: &wgpu::Device, queue: &wgpu::Queue, env: Cubemap, layouts: &BindGroupLayouts) -> Result<Self> {
    // Give the env cube its full mip chain before anything samples it. The prefilter reads
    // progressively blurrier mips per sample (Krivanek/Colbert), which is what removes the
    // grainy specular speckle a single-mip env leaves behind.
    log::info!("Baking env cube mip chain...");
    bake_env_mips(device, queue, layouts, &env);

    let env_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
      label: Some("Env Cube Bind Group"),
      layout: &layouts.env,
      entries: &[
        wgpu::BindGroupEntry {
          binding: 0,
          resource: wgpu::BindingResource::TextureView(&env.view),
        },
        wgpu::BindGroupEntry {
          binding: 1,
          resource: wgpu::BindingResource::Sampler(&env.sampler),
        },
      ],
    });

    // --- 5b: bake the diffuse irradiance cube by convolving env over the hemisphere. ---
    let irradiance = Cubemap::new_render_target(device, IRRADIANCE_SIZE, 1, wgpu::TextureFormat::Rgba16Float, "Irradiance Cube");

    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
      label: Some("Irradiance Bake Shader"),
      source: wgpu::ShaderSource::Wgsl(SHADER_IBL.into()),
    });

    // group 0 = per-face FaceUniform (group 1 = the env cube, via layouts.env).
    let face_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
      label: Some("Irradiance Face Layout"),
      entries: &[helpers::uniform_entry(0, wgpu::ShaderStages::FRAGMENT)],
    });

    // Same create_pipeline shape as the equirect bake; source layout is layouts.env (cube).
    let pipeline = pipelines::create_pipeline_with_entry_points(
      device,
      &module,
      "vert_main",
      "frag_irradiance",
      wgpu::TextureFormat::Rgba16Float,
      &[],                                       // no vertex buffers
      &[Some(&face_layout), Some(&layouts.env)], // group 0 = face uniform, group 1 = env cube
      false,                                     // no depth
      None,                                      // cull none
      None,                                      // no blend
    );

    // bake_faces caller #2: convolve env (env_bind_group as group 1) into irradiance.
    log::info!("Baking irradiance from env cube...");
    bake_faces(device, queue, &pipeline, &env_bind_group, 1, &face_layout, &irradiance);
    let brdf_lut = bake_brdf_lut(device, queue);

    let irradiance_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
      label: Some("Irradiance Cube Bind Group"),
      layout: &layouts.env,
      entries: &[
        wgpu::BindGroupEntry {
          binding: 0,
          resource: wgpu::BindingResource::TextureView(&irradiance.view),
        },
        wgpu::BindGroupEntry {
          binding: 1,
          resource: wgpu::BindingResource::Sampler(&irradiance.sampler),
        },
      ],
    });

    let prefilter = Cubemap::new_render_target(device, PREFILTER_SIZE, PREFILTER_MIPS, wgpu::TextureFormat::Rgba16Float, "Prefilter Cube");
    log::info!("Baking prefiltered specular env cube...");
    bake_prefilter(device, queue, layouts, &env_bind_group, &prefilter);

    let prefilter_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
      label: Some("Prefilter Cube Bind Group"),
      layout: &layouts.env,
      entries: &[
        wgpu::BindGroupEntry {
          binding: 0,
          resource: wgpu::BindingResource::TextureView(&prefilter.view),
        },
        wgpu::BindGroupEntry {
          binding: 1,
          resource: wgpu::BindingResource::Sampler(&prefilter.sampler),
        },
      ],
    });

    Ok(Self {
      env,
      env_bind_group,
      irradiance,
      irradiance_bind_group,
      brdf_lut,
      prefilter,
      prefilter_bind_group,
    })
  }
}

// Simple helper to fill a cube face with a solid color. Used for debugging and testing.
fn fill_face(encoder: &mut wgpu::CommandEncoder, cube: &crate::models::Cubemap, face: usize, color: wgpu::Color) {
  // The pass is dropped at the end of each iteration; dropping records the clear.
  let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
    label: Some("face clear"),
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

// Render a fullscreen-triangle pass into all six faces of a single-mip cube. The caller supplies the pipeline (which shader),
// the source it samples at group 1, the group-0 FaceUniform layout, and the destination cube.
fn bake_faces(
  device: &wgpu::Device,
  queue: &wgpu::Queue,
  pipeline: &wgpu::RenderPipeline,
  src_bind_group: &wgpu::BindGroup,
  src_bind_group_index: u32,
  face_layout: &wgpu::BindGroupLayout,
  dst: &Cubemap,
) {
  // 90-degree FOV so each face exactly covers one cube side; eye at the origin.
  // directx::perspective gives the [0,1] depth convention the bake shader's ndc.z = 1.0 assumes.
  let proj = glam::camera::rh::proj::directx::perspective(std::f32::consts::FRAC_PI_2, 1.0, 0.1, 10.0);

  // Standard cubemap capture set (forward, up). See the bake orientation notes; the
  // ndc.y write-flip lives in the shader, this table is where flips/rotations get fixed.
  let faces: [(Vec3, Vec3); 6] = [
    (Vec3::X, Vec3::NEG_Y),     // +X
    (Vec3::NEG_X, Vec3::NEG_Y), // -X
    (Vec3::Y, Vec3::Z),         // +Y
    (Vec3::NEG_Y, Vec3::NEG_Z), // -Y
    (Vec3::Z, Vec3::NEG_Y),     // +Z
    (Vec3::NEG_Z, Vec3::NEG_Y), // -Z
  ];

  let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Cube Face Bake") });

  for (face, (fwd, up)) in faces.iter().enumerate() {
    let view = glam::camera::rh::view::look_to_mat4(Vec3::ZERO, *fwd, *up);
    let inv_view_proj = (proj * view).inverse();

    // One buffer per face on purpose: reusing a single buffer + queue.write_buffer
    // across passes in one submit would leave every pass reading the LAST matrix.
    // create_buffer_init writes at creation, so each face gets its own value.
    let face_uniform = FaceUniform {
      inv_view_proj: inv_view_proj.to_cols_array(),
      roughness: 0.0, // only the prefilter bake reads this; others write 0.0
      _pad: [0.0; 3],
    };

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
fn bake_brdf_lut(device: &wgpu::Device, queue: &wgpu::Queue) -> Texture {
  let lut = Texture::new_render_target(device, BRDF_LUT_SIZE, BRDF_LUT_SIZE, wgpu::TextureFormat::Rg16Float, "BRDF LUT");

  let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
    label: Some("BRDF LUT Shader"),
    source: wgpu::ShaderSource::Wgsl(SHADER_IBL.into()),
  });

  // No bind group layouts: the LUT shader takes no inputs.
  let pipeline = pipelines::create_pipeline_with_entry_points(
    device,
    &module,
    "vert_main",
    "frag_brdf_lut",
    wgpu::TextureFormat::Rg16Float,
    &[],   // no vertex buffers
    &[],   // no bind groups
    false, // no depth
    None,  // cull none
    None,  // no blend
  );

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
    pass.set_pipeline(&pipeline);
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
fn bake_env_mips(device: &wgpu::Device, queue: &wgpu::Queue, layouts: &BindGroupLayouts, env: &Cubemap) {
  if env.mip_level_count <= 1 {
    return;
  }

  let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
    label: Some("Env Mip Bake Shader"),
    source: wgpu::ShaderSource::Wgsl(SHADER_IBL.into()),
  });

  // group 0 = per-face FaceUniform (inv_view_proj); group 1 = the parent-mip env cube view.
  let face_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
    label: Some("Env Mip Face Layout"),
    entries: &[helpers::uniform_entry(0, wgpu::ShaderStages::FRAGMENT)],
  });

  let pipeline = pipelines::create_pipeline_with_entry_points(
    device,
    &module,
    "vert_main",
    "frag_env_mips",
    wgpu::TextureFormat::Rgba16Float,
    &[],
    &[Some(&face_layout), Some(&layouts.env)],
    false,
    None,
    None,
  );

  let proj = glam::camera::rh::proj::directx::perspective(std::f32::consts::FRAC_PI_2, 1.0, 0.1, 10.0);
  let faces: [(Vec3, Vec3); 6] = [
    (Vec3::X, Vec3::NEG_Y),
    (Vec3::NEG_X, Vec3::NEG_Y),
    (Vec3::Y, Vec3::Z),
    (Vec3::NEG_Y, Vec3::NEG_Z),
    (Vec3::Z, Vec3::NEG_Y),
    (Vec3::NEG_Z, Vec3::NEG_Y),
  ];

  let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
    label: Some("Env Mip Chain Bake"),
  });

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

    let src_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
      label: Some("Env Mip Source Bind Group"),
      layout: &layouts.env,
      entries: &[
        wgpu::BindGroupEntry {
          binding: 0,
          resource: wgpu::BindingResource::TextureView(&src_view),
        },
        wgpu::BindGroupEntry {
          binding: 1,
          resource: wgpu::BindingResource::Sampler(&env.sampler),
        },
      ],
    });

    for (face, (fwd, up)) in faces.iter().enumerate() {
      let view = glam::camera::rh::view::look_to_mat4(Vec3::ZERO, *fwd, *up);
      let inv_view_proj = (proj * view).inverse();

      let uniform = FaceUniform {
        inv_view_proj: inv_view_proj.to_cols_array(),
        roughness: 0.0,
        _pad: [0.0; 3],
      };
      let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Env Mip Face Uniform"),
        contents: bytemuck::cast_slice(&[uniform]),
        usage: wgpu::BufferUsages::UNIFORM,
      });
      let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Env Mip Face Bind Group"),
        layout: &face_layout,
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

      pass.set_pipeline(&pipeline);
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
fn bake_prefilter(device: &wgpu::Device, queue: &wgpu::Queue, layouts: &BindGroupLayouts, src_bind_group: &wgpu::BindGroup, dst: &Cubemap) {
  let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
    label: Some("Prefilter Bake Shader"),
    source: wgpu::ShaderSource::Wgsl(SHADER_IBL.into()),
  });

  // group 0 = per-face PrefilterUniform (inv_view_proj + roughness); group 1 = env cube.
  let face_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
    label: Some("Prefilter Face Layout"),
    entries: &[helpers::uniform_entry(0, wgpu::ShaderStages::FRAGMENT)],
  });

  let pipeline = pipelines::create_pipeline_with_entry_points(
    device,
    &module,
    "vert_main",
    "frag_prefilter",
    wgpu::TextureFormat::Rgba16Float,
    &[],
    &[Some(&face_layout), Some(&layouts.env)],
    false,
    None,
    None,
  );

  let proj = glam::camera::rh::proj::directx::perspective(std::f32::consts::FRAC_PI_2, 1.0, 0.1, 10.0);
  let faces: [(Vec3, Vec3); 6] = [
    (Vec3::X, Vec3::NEG_Y),
    (Vec3::NEG_X, Vec3::NEG_Y),
    (Vec3::Y, Vec3::Z),
    (Vec3::NEG_Y, Vec3::NEG_Z),
    (Vec3::Z, Vec3::NEG_Y),
    (Vec3::NEG_Z, Vec3::NEG_Y),
  ];

  let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Prefilter Bake") });

  // Nested loop: mip first, then face. Each mip has a different roughness, so we can't reuse bake_faces.
  for mip in 0..PREFILTER_MIPS {
    // roughness sweeps 0..1 across the mip chain.
    let roughness = mip as f32 / (PREFILTER_MIPS - 1) as f32;

    for (face, (fwd, up)) in faces.iter().enumerate() {
      let view = glam::camera::rh::view::look_to_mat4(Vec3::ZERO, *fwd, *up);
      let inv_view_proj = (proj * view).inverse();

      // Fresh buffer per (mip, face): one shared buffer across passes in a single submit
      // would leave every pass reading the LAST value written.
      let uniform = FaceUniform {
        inv_view_proj: inv_view_proj.to_cols_array(),
        roughness,
        _pad: [0.0; 3],
      };
      let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Prefilter Face Uniform"),
        contents: bytemuck::cast_slice(&[uniform]),
        usage: wgpu::BufferUsages::UNIFORM,
      });
      let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Prefilter Face Bind Group"),
        layout: &face_layout,
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

      pass.set_pipeline(&pipeline);
      pass.set_bind_group(0, &bind_group, &[]);
      pass.set_bind_group(1, src_bind_group, &[]);
      pass.draw(0..3, 0..1);
    }
  }

  queue.submit([encoder.finish()]);
}
