use crate::engine::BindGroupLayouts;
use crate::engine::pipelines;

const SHADER_SKYBOX: &str = include_str!("../../shaders/skybox.wgsl");

pub(crate) struct Skybox {
  skybox_pipeline: wgpu::RenderPipeline,
}

impl Skybox {
  pub(crate) fn new(device: &wgpu::Device, bind_group_layouts: &BindGroupLayouts, target_format: wgpu::TextureFormat) -> Skybox {
    let sky_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
      label: Some("Skybox Shader"),
      source: wgpu::ShaderSource::Wgsl(SHADER_SKYBOX.into()),
    });

    let skybox_pipeline = pipelines::create_pipeline(
      &device,
      &sky_module,
      target_format,                                                         // same sRGB surface format used for the main pipeline
      &[],                                                                   // no vertex buffers
      &[Some(&bind_group_layouts.frame_cam), Some(&bind_group_layouts.env)], // group 0 = frame/camera, group 1 = env map
      true,                                                                  // enable depth (matches the pass's Depth32Float attachment)
      None,                                                                  // cull none (fullscreen tri winding is irrelevant)
      None,                                                                  // no blend -> depth_write true, harmless at far plane
    );

    Skybox { skybox_pipeline }
  }

  pub(crate) fn render(&self, render_pass: &mut wgpu::RenderPass, env_bind_group: &wgpu::BindGroup, camera_bind_group: &wgpu::BindGroup) {
    render_pass.set_pipeline(&self.skybox_pipeline);
    render_pass.set_bind_group(0, camera_bind_group, &[]);
    render_pass.set_bind_group(1, env_bind_group, &[]);
    render_pass.draw(0..3, 0..1);
  }
}
