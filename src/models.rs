#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
  position: [f32; 3],
  tex_coord: [f32; 2],
  color: [f32; 3],
}

impl Vertex {
  // TODO: replace with wgpu::vertex_attr_array! macro
  pub fn desc() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
      array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
      step_mode: wgpu::VertexStepMode::Vertex,
      attributes: &[
        wgpu::VertexAttribute {
          offset: 0,
          shader_location: 0,
          format: wgpu::VertexFormat::Float32x3,
        },
        wgpu::VertexAttribute {
          offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
          shader_location: 1,
          format: wgpu::VertexFormat::Float32x2,
        },
        wgpu::VertexAttribute {
          offset: std::mem::size_of::<[f32; 5]>() as wgpu::BufferAddress,
          shader_location: 2,
          format: wgpu::VertexFormat::Float32x3,
        },
      ],
    }
  }
}

// === Example data for testing ===

pub fn example_quad() -> (&'static [Vertex], &'static [u16]) {
  let verts: &[Vertex] = &[
    Vertex {
      position: [-0.5, -0.5, 0.0],
      tex_coord: [0.0, 1.0],
      color: [0.0, 1.0, 0.0],
    },
    Vertex {
      position: [0.5, -0.5, 0.0],
      tex_coord: [1.0, 1.0],
      color: [0.0, 1.0, 0.0],
    },
    Vertex {
      position: [0.5, 0.5, 0.0],
      tex_coord: [1.0, 0.0],
      color: [1.0, 0.0, 0.0],
    },
    Vertex {
      position: [-0.5, 0.5, 0.0],
      tex_coord: [0.0, 0.0],
      color: [1.0, 0.0, 0.0],
    },
  ];

  let indices: &[u16] = &[0, 1, 2, 2, 3, 0];

  (verts, indices)
}
