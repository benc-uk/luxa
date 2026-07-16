// ======================================================================================
// Structs and functions for describing vertex data and example models to render.
// ======================================================================================

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
// The vertex data for a 3D model. This is the data that will be sent to the GPU for rendering.
pub struct Vertex {
  position: [f32; 3],
  tex_coord: [f32; 2],
  color: [f32; 3],
}

impl Vertex {
  const ATTRIBUTES: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x3];

  // Get a description of the vertex buffer layout for this vertex type. This is used when creating the render pipeline.
  pub fn desc() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
      array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
      step_mode: wgpu::VertexStepMode::Vertex,
      attributes: &Self::ATTRIBUTES,
    }
  }
}

// Returns a cube model with 24 vertices and 36 indices (6 faces, 2 triangles per face)
pub fn primitive_cube() -> (&'static [Vertex], &'static [u16]) {
  // A cube needs 24 vertices (4 per face)
  let verts: &[Vertex] = &[
    // Front face (+z)
    Vertex {
      position: [-0.5, -0.5, 0.5],
      tex_coord: [0.0, 1.0],
      color: [1.0, 1.0, 1.0],
    },
    Vertex {
      position: [0.5, -0.5, 0.5],
      tex_coord: [1.0, 1.0],
      color: [1.0, 1.0, 1.0],
    },
    Vertex {
      position: [0.5, 0.5, 0.5],
      tex_coord: [1.0, 0.0],
      color: [1.0, 1.0, 1.0],
    },
    Vertex {
      position: [-0.5, 0.5, 0.5],
      tex_coord: [0.0, 0.0],
      color: [1.0, 1.0, 1.0],
    },
    // Back face (-z)
    Vertex {
      position: [0.5, -0.5, -0.5],
      tex_coord: [0.0, 1.0],
      color: [1.0, 1.0, 1.0],
    },
    Vertex {
      position: [-0.5, -0.5, -0.5],
      tex_coord: [1.0, 1.0],
      color: [1.0, 1.0, 1.0],
    },
    Vertex {
      position: [-0.5, 0.5, -0.5],
      tex_coord: [1.0, 0.0],
      color: [1.0, 1.0, 1.0],
    },
    Vertex {
      position: [0.5, 0.5, -0.5],
      tex_coord: [0.0, 0.0],
      color: [1.0, 1.0, 1.0],
    },
    // Right face (+x)
    Vertex {
      position: [0.5, -0.5, 0.5],
      tex_coord: [0.0, 1.0],
      color: [1.0, 1.0, 1.0],
    },
    Vertex {
      position: [0.5, -0.5, -0.5],
      tex_coord: [1.0, 1.0],
      color: [1.0, 1.0, 1.0],
    },
    Vertex {
      position: [0.5, 0.5, -0.5],
      tex_coord: [1.0, 0.0],
      color: [1.0, 1.0, 1.0],
    },
    Vertex {
      position: [0.5, 0.5, 0.5],
      tex_coord: [0.0, 0.0],
      color: [1.0, 1.0, 1.0],
    },
    // Left face (-x)
    Vertex {
      position: [-0.5, -0.5, -0.5],
      tex_coord: [0.0, 1.0],
      color: [1.0, 1.0, 1.0],
    },
    Vertex {
      position: [-0.5, -0.5, 0.5],
      tex_coord: [1.0, 1.0],
      color: [1.0, 1.0, 1.0],
    },
    Vertex {
      position: [-0.5, 0.5, 0.5],
      tex_coord: [1.0, 0.0],
      color: [1.0, 1.0, 1.0],
    },
    Vertex {
      position: [-0.5, 0.5, -0.5],
      tex_coord: [0.0, 0.0],
      color: [1.0, 1.0, 1.0],
    },
    // Top face (+y)
    Vertex {
      position: [-0.5, 0.5, 0.5],
      tex_coord: [0.0, 1.0],
      color: [1.0, 1.0, 1.0],
    },
    Vertex {
      position: [0.5, 0.5, 0.5],
      tex_coord: [1.0, 1.0],
      color: [1.0, 1.0, 1.0],
    },
    Vertex {
      position: [0.5, 0.5, -0.5],
      tex_coord: [1.0, 0.0],
      color: [1.0, 1.0, 1.0],
    },
    Vertex {
      position: [-0.5, 0.5, -0.5],
      tex_coord: [0.0, 0.0],
      color: [1.0, 1.0, 1.0],
    },
    // Bottom face (-y)
    Vertex {
      position: [-0.5, -0.5, -0.5],
      tex_coord: [0.0, 1.0],
      color: [1.0, 1.0, 1.0],
    },
    Vertex {
      position: [0.5, -0.5, -0.5],
      tex_coord: [1.0, 1.0],
      color: [1.0, 1.0, 1.0],
    },
    Vertex {
      position: [0.5, -0.5, 0.5],
      tex_coord: [1.0, 0.0],
      color: [1.0, 1.0, 1.0],
    },
    Vertex {
      position: [-0.5, -0.5, 0.5],
      tex_coord: [0.0, 0.0],
      color: [1.0, 1.0, 1.0],
    },
  ];

  let indices: &[u16] = &[
    0, 1, 2, 2, 3, 0, // front
    4, 5, 6, 6, 7, 4, // back
    8, 9, 10, 10, 11, 8, // right
    12, 13, 14, 14, 15, 12, // left
    16, 17, 18, 18, 19, 16, // top
    20, 21, 22, 22, 23, 20, // bottom
  ];

  (verts, indices)
}
