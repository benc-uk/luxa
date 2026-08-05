use std::vec;

use super::{Engine, Node3DHandle, SceneHandle, gpu};
use crate::helpers;
use glam::Mat4;

pub(crate) struct BindGroupLayouts {
  pub(crate) frame_cam: wgpu::BindGroupLayout,
  pub(crate) material: wgpu::BindGroupLayout,
  pub(crate) node: wgpu::BindGroupLayout,
  pub(crate) lights: wgpu::BindGroupLayout,
  pub(crate) env: wgpu::BindGroupLayout, // Note that this is not used in the main render pass, but is used in the IBL baking passes
}

impl Engine {
  pub fn resize(&mut self, new_size: (u32, u32)) {
    let w = new_size.0;
    let h = new_size.1;

    if w > 0 && h > 0 {
      self.surf_config.width = w;
      self.surf_config.height = h;
      self.surface.configure(&self.device, &self.surf_config);
      let (_depth_texture, depth_texture_view) = gpu::create_depth_texture(&self.device, &self.surf_config);
      self.depth_texture_view = depth_texture_view;
      self.aspect = w as f32 / h as f32;
      self.is_surface_configured = true;
    }

    log::info!("Resized surface to {w}x{h}, aspect ratio is now {}", self.aspect);
  }

  // TODO: This is all hard coded crap while weare figuring out how to do a scene graph and testing crap
  pub fn update(&mut self) {
    self.frame_uniform.time = self.start_time.elapsed().as_secs_f32();
    self.queue.write_buffer(&self.frame_uniform_buffer, 0, bytemuck::cast_slice(&[self.frame_uniform]));
  }

  pub fn t(&self) -> f32 {
    self.frame_uniform.time
  }

  pub fn render(&mut self, scene: SceneHandle, camera_node: Node3DHandle) -> anyhow::Result<()> {
    // We can't render unless the surface is configured
    if !self.is_surface_configured {
      return Ok(());
    }

    for material in self.materials.values_mut() {
      material.upload_gpu(&self.device, &self.queue, &self.textures);
    }

    // Root node for rendering all nodes in this scene
    let root = *self.scenes.get(scene).expect("invalid scene");

    self.lights_uniform.count = 0;
    // List of nodes to render, with their world matrices. We will fill this by traversing the scene graph.
    let mut render_list: Vec<Node3DHandle> = Vec::new();
    // Stack starts with the root node and identity world matrix
    let mut stack = vec![(root, Mat4::IDENTITY)];
    let mut camera_found = false;

    // This stack approach is a depth-first traversal of the scene graph, without the need for recursion.
    while let Some((node_hdl, parent_world)) = stack.pop() {
      // Calculate this node's world matrix and set/cache it in the node
      let world = parent_world * self.nodes[node_hdl].local_matrix();
      self.nodes[node_hdl].set_world_matrix(world);
      let node = &self.nodes[node_hdl];

      // Handle camera node specially, have to do this here after world matrix is set
      if node_hdl == camera_node {
        if let Some(vp) = node.view_proj(self.aspect) {
          self.camera_uniform.view_proj = vp.to_cols_array();
          self.camera_uniform.pos = node.world_position().to_array();
          self.camera_uniform.inv_view_proj = vp.inverse().to_cols_array();
          self.queue.write_buffer(&self.camera_uniform_buffer, 0, bytemuck::cast_slice(&[self.camera_uniform]));

          camera_found = true;
        }
      }

      // Light nodes are added to the lights uniform
      if node.is_light() {
        if let Some(light_data) = node.light_data() {
          self.lights_uniform.add_light(light_data, world.w_axis.truncate());
        }
      }

      // Output list of nodes to render is just all nodes that have meshes, in depth-first order
      if node.is_mesh() {
        render_list.push(node_hdl);
      }

      for &child in node.children() {
        stack.push((child, world));
      }
    }

    if !camera_found {
      log::warn!("camera node {camera_node:?} not found in scene (or node is not a camera); skipping frame");
      return Ok(());
    }

    // Really important to upload the world matrices for all nodes before rendering
    // This also uploads the world matrix & position for the camera node
    for &node_hdl in &render_list {
      self.nodes[node_hdl].upload_world_mat(&self.queue);
    }

    self.queue.write_buffer(&self.lights_uniform_buffer, 0, bytemuck::cast_slice(&[self.lights_uniform]));

    // Get the current texture from the surface and create a view for rendering
    let output = match self.surface.get_current_texture() {
      // Both hand back a usable texture.
      wgpu::CurrentSurfaceTexture::Success(surface_texture) | wgpu::CurrentSurfaceTexture::Suboptimal(surface_texture) => surface_texture,

      wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded | wgpu::CurrentSurfaceTexture::Validation => {
        return Ok(());
      }

      wgpu::CurrentSurfaceTexture::Outdated => {
        self.surface.configure(&self.device, &self.surf_config);
        return Ok(());
      }

      wgpu::CurrentSurfaceTexture::Lost => {
        anyhow::bail!("Lost device");
      }
    };

    let view = output.texture.create_view(&wgpu::TextureViewDescriptor {
      format: Some(self.surf_config.format.add_srgb_suffix()),
      ..Default::default()
    });

    // Begin rendering commands
    let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Render Encoder") });

    // Whole render pass is contained in this block, so it is dropped (releasing
    // its &mut borrow of the encoder) before we call encoder.finish() below.
    {
      let mut render_pass = gpu::create_render_pass(&mut encoder, &view, Some(&self.depth_texture_view));
      render_pass.set_bind_group(0, &self.frame_cam_bind_group, &[]);
      render_pass.set_bind_group(3, &self.lights_bind_group, &[]);
      // render_pass.set_bind_group(4, &self.ibl.render_bind_group, &[]);

      // Place to store all blended meshes, which we will render after all opaque meshes
      let mut blended_meshes = vec![];

      // Walk the scene graph: each mesh-carrying node draws its meshes, looking up each
      // mesh's material by handle from the engine's arenas.
      for node in render_list.iter().map(|hdl| &self.nodes[*hdl]) {
        render_pass.set_bind_group(2, node.get_bind_group(), &[]);

        for &mesh_handle in node.mesh_handles() {
          let mesh = self.meshes.get(mesh_handle).expect("Invalid mesh handle");
          let material = self.materials.get(mesh.material_handle()).expect("Invalid material handle");

          if material.is_blended() {
            blended_meshes.push((node, mesh, material));
          } else {
            render_mesh(&mut render_pass, mesh, material, &self.pipelines);
          }
        }
      }

      // Renderer skybox after opaque meshes, but before blended meshes
      self.skybox.render(&mut render_pass, &self);

      // 🔥 TODO: Sort blended meshes by depth from camera

      //log::error!("Rendering {} blended meshes", blended_meshes.len());

      // Render all blended meshes after all opaque meshes, so they are drawn on top of the opaque ones.
      for (node, mesh, material) in blended_meshes {
        render_pass.set_bind_group(2, node.get_bind_group(), &[]);
        render_mesh(&mut render_pass, mesh, material, &self.pipelines);
      }
    }

    // Submit the encoded commands on the queue & present the output texture to the surface
    self.queue.submit([encoder.finish()]);
    output.present();

    Ok(())
  }

  pub(crate) fn init_bind_group_layouts(device: &wgpu::Device) -> BindGroupLayouts {
    BindGroupLayouts {
      frame_cam: device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Uniform Bind Group Layout"),
        entries: &[
          helpers::uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT), // camera uniform
          helpers::uniform_entry(1, wgpu::ShaderStages::VERTEX_FRAGMENT), // time uniform
        ],
      }),

      material: device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Material Bind Group Layout"),
        entries: &[
          helpers::uniform_entry(0, wgpu::ShaderStages::FRAGMENT),
          helpers::texture_entry(1),
          helpers::sampler_entry(2),
          helpers::texture_entry(3),
          helpers::sampler_entry(4),
          helpers::texture_entry(5),
          helpers::sampler_entry(6),
          helpers::texture_entry(7),
          helpers::sampler_entry(8),
          helpers::texture_entry(9),
          helpers::sampler_entry(10),
        ],
      }),

      node: device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Node Bind Group Layout"),
        entries: &[helpers::uniform_entry(0, wgpu::ShaderStages::VERTEX)],
      }),

      lights: device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Lights Bind Group Layout"),
        entries: &[
          helpers::uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT),
          helpers::texture_cube_entry(1), // irradiance
          helpers::sampler_entry(2),
          helpers::texture_cube_entry(3), // prefilter
          helpers::sampler_entry(4),
          helpers::texture_entry(5), // brdf lut, not a cube
          helpers::sampler_entry(6),
        ],
      }),

      env: device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Environment Bind Group Layout"),
        entries: &[helpers::texture_cube_entry(0), helpers::sampler_entry(1)],
      }),
    }
  }
}

// Free function to render a mesh with a material, using the given render pass and pipelines. This is called from Engine::render()
fn render_mesh(render_pass: &mut wgpu::RenderPass, mesh: &super::Mesh, material: &super::Material, pipelines: &super::Pipelines) {
  let pipeline = pipelines.select(material.is_blended(), material.is_double_sided());
  render_pass.set_pipeline(&pipeline);

  render_pass.set_bind_group(1, material.get_bind_group(), &[]);
  render_pass.set_vertex_buffer(0, mesh.vertex_buffer().slice(..));
  render_pass.set_index_buffer(mesh.index_buffer().slice(..), wgpu::IndexFormat::Uint16);
  render_pass.draw_indexed(0..mesh.num_indices(), 0, 0..1);
}
