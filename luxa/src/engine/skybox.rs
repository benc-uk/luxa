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
      &[Some(&bind_group_layouts.frame_cam), Some(&bind_group_layouts.env)], // group 0 = sky uniform, group 1 = env cube
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

// Axis-coded so orientation bugs are obvious: bright = +, dim = -.
const FACE_COLOURS: [wgpu::Color; 6] = [
  wgpu::Color { r: 0.8, g: 0.1, b: 0.1, a: 1.0 }, // +X bright red
  wgpu::Color { r: 0.3, g: 0.0, b: 0.0, a: 1.0 }, // -X dim red
  wgpu::Color { r: 0.1, g: 0.8, b: 0.1, a: 1.0 }, // +Y bright green
  wgpu::Color { r: 0.0, g: 0.3, b: 0.0, a: 1.0 }, // -Y dim green
  wgpu::Color { r: 0.1, g: 0.1, b: 0.8, a: 1.0 }, // +Z bright blue
  wgpu::Color { r: 0.0, g: 0.0, b: 0.3, a: 1.0 }, // -Z dim blue
];

// Debug function to fill the 6 faces of a cubemap with solid colours, so you can see which face is which. This is useful for debugging orientation issues when baking cubemaps.
pub(crate) fn fill_faces(device: &wgpu::Device, queue: &wgpu::Queue, cube: &crate::models::Cubemap) {
  let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Cube face fill") });
  for face in 0..6usize {
    // The pass is dropped at the end of each iteration; dropping records the clear.
    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
      label: Some("face clear"),
      color_attachments: &[Some(wgpu::RenderPassColorAttachment {
        view: cube.face_view(0, face),
        resolve_target: None,
        depth_slice: None,
        ops: wgpu::Operations {
          load: wgpu::LoadOp::Clear(FACE_COLOURS[face]),
          store: wgpu::StoreOp::Store,
        },
      })],
      depth_stencil_attachment: None,
      occlusion_query_set: None,
      timestamp_writes: None,
      multiview_mask: None,
    });
  }
  queue.submit([encoder.finish()]);
}
