// ======================================================================================
// Structs and functions for describing vertex data and example models to render.
// ======================================================================================

use crate::engine::{Engine, MaterialHandle};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
// The vertex data for a 3D model. This is the data that will be sent to the GPU for rendering.
pub struct Vertex {
  pub position: [f32; 3],
  pub tex_coord: [f32; 2],
  pub normal: [f32; 3],
}

impl Vertex {
  const ATTRIBUTES: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x3];

  // Get a description of the vertex buffer layout for this vertex type. This is used when creating the render pipeline.
  pub(crate) fn desc() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
      array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
      step_mode: wgpu::VertexStepMode::Vertex,
      attributes: &Self::ATTRIBUTES,
    }
  }
}

// ===== Mesh ===============================================================================

pub struct Mesh {
  material: MaterialHandle,
  indices_count: u32,
  vertex_buffer: wgpu::Buffer,
  index_buffer: wgpu::Buffer,
}

impl Mesh {
  pub fn new(engine: &Engine, vertices: Vec<Vertex>, indices: Vec<u16>, material: MaterialHandle) -> Self {
    let vertex_buffer = engine.get_device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
      label: Some("Vertex Buffer"),
      contents: bytemuck::cast_slice(&vertices),
      usage: wgpu::BufferUsages::VERTEX,
    });

    let index_buffer = engine.get_device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
      label: Some("Index Buffer"),
      contents: bytemuck::cast_slice(&indices),
      usage: wgpu::BufferUsages::INDEX,
    });

    Mesh {
      material,
      indices_count: indices.len() as u32,
      vertex_buffer,
      index_buffer,
    }
  }

  pub(crate) fn vertex_buffer(&self) -> &wgpu::Buffer {
    &self.vertex_buffer
  }

  pub(crate) fn index_buffer(&self) -> &wgpu::Buffer {
    &self.index_buffer
  }

  pub(crate) fn num_indices(&self) -> u32 {
    self.indices_count
  }

  pub(crate) fn material_handle(&self) -> MaterialHandle {
    self.material
  }
}
