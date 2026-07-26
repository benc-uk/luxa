// ======================================================================================
// IBL: image-based lighting resources baked from an equirectangular HDR environment.
//
// This grows one milestone at a time. Right now it holds only 5a: the environment cube
// baked from an equirect HDR, plus the bind group the skybox samples. Irradiance,
// prefilter and the BRDF LUT arrive with 5b / 5c / 5d.
// ======================================================================================

use crate::engine::BindGroupLayouts;
use crate::engine::pipelines;
use crate::helpers;
use crate::models::{Cubemap, Texture};
use anyhow::Result;
use glam::Vec3;
use wgpu::util::DeviceExt;

const SHADER_EQUIRECT: &str = include_str!("../../shaders/equirect.wgsl");
const SHADER_IRRADIANCE: &str = include_str!("../../shaders/irradiance.wgsl");
const SHADER_BRDF_LUT: &str = include_str!("../../shaders/brdf_lut.wgsl");

// Consts for controlling the overall IBL
const ENV_SIZE: u32 = 1024;
const IRRADIANCE_SIZE: u32 = 32;
const BRDF_LUT_SIZE: u32 = 512;

// Per-face uniform for the bake: the inverse view-proj that turns a face's NDC into a
// world-space sample direction.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct FaceUniform {
  inv_view_proj: [f32; 16],
}

#[allow(dead_code)]
pub(crate) struct Ibl {
  // The environment cube, baked from the HDR. Sampled by the skybox and, later, the
  // source for the irradiance / prefilter convolutions.
  pub(crate) env: Cubemap,
  // Runtime bind group over the env cube (cube view + sampler), consumed by the skybox.
  pub(crate) env_bind_group: wgpu::BindGroup,
  pub(crate) irradiance: Cubemap,
  pub(crate) irradiance_bind_group: wgpu::BindGroup,
  pub(crate) brdf_lut: Texture,
}

impl Ibl {
  // Bake an equirectangular HDR into the six faces of a fresh environment cube.
  pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue, hdr_bytes: &[u8], layouts: &BindGroupLayouts) -> Result<Self> {
    log::info!("Creating IBL from equirect HDR");

    // Destination cube we render the six faces into.
    let env = Cubemap::new_render_target(device, ENV_SIZE, 1, wgpu::TextureFormat::Rgba16Float, "Env Cube");

    // Transient equirect source: sampled during the bake, dropped when this fn returns.
    let src = Texture::new_equirect_hdr(device, queue, hdr_bytes, "Equirect HDR Source")?;

    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
      label: Some("Equirect Bake Shader"),
      source: wgpu::ShaderSource::Wgsl(SHADER_EQUIRECT.into()),
    });

    // Bake-only layouts (not part of the shared BindGroupLayouts): group 0 = per-face
    // inverse view-proj uniform, group 1 = the 2D equirect source.
    let face_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
      label: Some("Equirect Face Layout"),
      entries: &[helpers::uniform_entry(0, wgpu::ShaderStages::FRAGMENT)],
    });

    let src_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
      label: Some("Equirect Source Layout"),
      entries: &[helpers::texture_entry(0), helpers::sampler_entry(1)],
    });

    // Fullscreen-triangle bake: no depth, no cull, no blend; target matches the cube.
    let pipeline = pipelines::create_pipeline(
      device,
      &module,
      wgpu::TextureFormat::Rgba16Float,
      &[], // no vertex buffers
      &[Some(&face_layout), Some(&src_layout)],
      false, // no depth
      None,  // cull none
      None,  // no blend
    );

    // group 1 is the same equirect source for all six faces.
    let src_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
      label: Some("Equirect Source Bind Group"),
      layout: &src_layout,
      entries: &[helpers::bind_texture(0, &src), helpers::bind_sampler(1, &src)],
    });

    log::info!("Baking equirect HDR into env cube...");
    bake_faces(device, queue, &pipeline, &src_bind_group, &face_layout, &env);

    Self::from_env(device, queue, env, layouts)
  }

  // Create a new IBL with a solid color for the environment cube. This is useful for testing and debugging only
  pub(crate) fn new_debug(device: &wgpu::Device, queue: &wgpu::Queue, layouts: &BindGroupLayouts) -> Result<Self> {
    let env = Cubemap::new_render_target(device, ENV_SIZE, 1, wgpu::TextureFormat::Rgba16Float, "Env Cube");

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
    let env = Cubemap::new_render_target(device, ENV_SIZE, 1, wgpu::TextureFormat::Rgba16Float, "Env Cube");

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
      source: wgpu::ShaderSource::Wgsl(SHADER_IRRADIANCE.into()),
    });

    // group 0 = per-face FaceUniform (group 1 = the env cube, via layouts.env).
    let face_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
      label: Some("Irradiance Face Layout"),
      entries: &[helpers::uniform_entry(0, wgpu::ShaderStages::FRAGMENT)],
    });

    // Same create_pipeline shape as the equirect bake; source layout is layouts.env (cube).
    let pipeline = pipelines::create_pipeline(
      device,
      &module,
      wgpu::TextureFormat::Rgba16Float,
      &[],                                       // no vertex buffers
      &[Some(&face_layout), Some(&layouts.env)], // group 0 = face uniform, group 1 = env cube
      false,                                     // no depth
      None,                                      // cull none
      None,                                      // no blend
    );

    // bake_faces caller #2: convolve env (env_bind_group as group 1) into irradiance.
    log::info!("Baking irradiance from env cube...");
    bake_faces(device, queue, &pipeline, &env_bind_group, &face_layout, &irradiance);
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

    Ok(Self {
      env,
      env_bind_group,
      irradiance,
      irradiance_bind_group,
      brdf_lut,
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
fn bake_faces(device: &wgpu::Device, queue: &wgpu::Queue, pipeline: &wgpu::RenderPipeline, src_bind_group: &wgpu::BindGroup, face_layout: &wgpu::BindGroupLayout, dst: &Cubemap) {
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
    pass.set_bind_group(1, src_bind_group, &[]);
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
    source: wgpu::ShaderSource::Wgsl(SHADER_BRDF_LUT.into()),
  });

  // No bind group layouts: the LUT shader takes no inputs.
  let pipeline = pipelines::create_pipeline(
    device,
    &module,
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
