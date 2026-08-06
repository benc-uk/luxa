use crate::engine;
use crate::engine::BindGroupLayouts;
use crate::engine::pipelines;
use crate::helpers;
use wgpu::util::DeviceExt;

const SHADER_SKYBOX: &str = include_str!("../../shaders/skybox.wgsl");

pub enum SkyboxMode {
  None,
  EnvironmentMap,
  IrradianceMap,
  PrefilteredMap,
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct MipUniform {
  mip_level: f32,
  _padding: [f32; 3], // pad to 16 bytes for uniform
}

pub(crate) struct Skybox {
  skybox_pipeline: wgpu::RenderPipeline,
  mode: SkyboxMode,
  mip_bind_group: wgpu::BindGroup,
  mip_uniform: MipUniform,
  mip_uniform_buffer: wgpu::Buffer,
}

impl Skybox {
  pub(crate) fn new(device: &wgpu::Device, bind_group_layouts: &BindGroupLayouts, target_format: wgpu::TextureFormat) -> Skybox {
    let sky_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
      label: Some("Skybox Shader"),
      source: wgpu::ShaderSource::Wgsl(SHADER_SKYBOX.into()),
    });

    let mip_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
      label: Some("Skybox Mip Bind Group Layout"),
      entries: &[helpers::uniform_entry(0, wgpu::ShaderStages::FRAGMENT)],
    });

    let mip_uniform = MipUniform {
      mip_level: 0.0,
      _padding: [0.0; 3],
    };

    let mip_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
      label: Some("Skybox Mip Uniform Buffer"),
      contents: bytemuck::bytes_of(&mip_uniform),
      usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let mip_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
      label: Some("Skybox Mip Bind Group"),
      layout: &mip_bind_group_layout,
      entries: &[wgpu::BindGroupEntry {
        binding: 0,
        resource: mip_uniform_buffer.as_entire_binding(),
      }],
    });

    let skybox_pipeline = pipelines::create_pipeline(
      &device,
      &sky_module,
      target_format,                                                                                       // same sRGB surface format used for the main pipeline
      &[],                                                                                                 // no vertex buffers
      &[Some(&bind_group_layouts.frame_cam), Some(&bind_group_layouts.env), Some(&mip_bind_group_layout)], // group 0 = frame/camera, group 1 = env map, group 2 = MIP level
      true,                                                                                                // enable depth (matches the pass's Depth32Float attachment)
      None,                                                                                                // cull none (fullscreen tri winding is irrelevant)
      None,                                                                                                // no blend -> depth_write true, harmless at far plane
    );

    Skybox {
      skybox_pipeline,
      mode: SkyboxMode::None,
      mip_bind_group,
      mip_uniform,
      mip_uniform_buffer,
    }
  }

  pub(crate) fn set_mode(&mut self, mode: SkyboxMode) {
    self.mode = mode;
  }

  pub(crate) fn set_mip_level(&mut self, queue: &wgpu::Queue, mip_level: f32) {
    self.mip_uniform.mip_level = mip_level;
    queue.write_buffer(&self.mip_uniform_buffer, 0, bytemuck::bytes_of(&self.mip_uniform));
  }

  pub(crate) fn render(&self, render_pass: &mut wgpu::RenderPass, engine: &engine::Engine) {
    render_pass.set_pipeline(&self.skybox_pipeline);
    render_pass.set_bind_group(0, &engine.frame_cam_bind_group, &[]);
    let env_bind_group = match self.mode {
      SkyboxMode::None => return, // no skybox to render
      SkyboxMode::EnvironmentMap => engine.ibl.env_bind_group(),
      SkyboxMode::IrradianceMap => engine.ibl.irradiance_bind_group(),
      SkyboxMode::PrefilteredMap => engine.ibl.prefilter_bind_group(),
    };
    render_pass.set_bind_group(1, env_bind_group, &[]);
    render_pass.set_bind_group(2, &self.mip_bind_group, &[]);

    render_pass.draw(0..3, 0..1);
  }
}
