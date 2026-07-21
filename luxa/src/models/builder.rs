use crate::engine::{Engine, MaterialHandle, MeshHandle};
use crate::models::{Mesh, Vertex};

// Returns a cube model with 24 vertices and 36 indices (6 faces, 2 triangles per face)
pub(crate) fn primitive_cube() -> (Vec<Vertex>, Vec<u16>) {
  // A cube needs 24 vertices (4 per face)
  let vertices: Vec<Vertex> = vec![
    // Front face (+z)
    Vertex {
      position: [-0.5, -0.5, 0.5],
      tex_coord: [0.0, 1.0],
      normal: [0.0, 0.0, 1.0],
    },
    Vertex {
      position: [0.5, -0.5, 0.5],
      tex_coord: [1.0, 1.0],
      normal: [0.0, 0.0, 1.0],
    },
    Vertex {
      position: [0.5, 0.5, 0.5],
      tex_coord: [1.0, 0.0],
      normal: [0.0, 0.0, 1.0],
    },
    Vertex {
      position: [-0.5, 0.5, 0.5],
      tex_coord: [0.0, 0.0],
      normal: [0.0, 0.0, 1.0],
    },
    // Back face (-z)
    Vertex {
      position: [0.5, -0.5, -0.5],
      tex_coord: [0.0, 1.0],
      normal: [0.0, 0.0, -1.0],
    },
    Vertex {
      position: [-0.5, -0.5, -0.5],
      tex_coord: [1.0, 1.0],
      normal: [0.0, 0.0, -1.0],
    },
    Vertex {
      position: [-0.5, 0.5, -0.5],
      tex_coord: [1.0, 0.0],
      normal: [0.0, 0.0, -1.0],
    },
    Vertex {
      position: [0.5, 0.5, -0.5],
      tex_coord: [0.0, 0.0],
      normal: [0.0, 0.0, -1.0],
    },
    // Right face (+x)
    Vertex {
      position: [0.5, -0.5, 0.5],
      tex_coord: [0.0, 1.0],
      normal: [1.0, 0.0, 0.0],
    },
    Vertex {
      position: [0.5, -0.5, -0.5],
      tex_coord: [1.0, 1.0],
      normal: [1.0, 0.0, 0.0],
    },
    Vertex {
      position: [0.5, 0.5, -0.5],
      tex_coord: [1.0, 0.0],
      normal: [1.0, 0.0, 0.0],
    },
    Vertex {
      position: [0.5, 0.5, 0.5],
      tex_coord: [0.0, 0.0],
      normal: [1.0, 0.0, 0.0],
    },
    // Left face (-x)
    Vertex {
      position: [-0.5, -0.5, -0.5],
      tex_coord: [0.0, 1.0],
      normal: [-1.0, 0.0, 0.0],
    },
    Vertex {
      position: [-0.5, -0.5, 0.5],
      tex_coord: [1.0, 1.0],
      normal: [-1.0, 0.0, 0.0],
    },
    Vertex {
      position: [-0.5, 0.5, 0.5],
      tex_coord: [1.0, 0.0],
      normal: [-1.0, 0.0, 0.0],
    },
    Vertex {
      position: [-0.5, 0.5, -0.5],
      tex_coord: [0.0, 0.0],
      normal: [-1.0, 0.0, 0.0],
    },
    // Top face (+y)
    Vertex {
      position: [-0.5, 0.5, 0.5],
      tex_coord: [0.0, 1.0],
      normal: [0.0, 1.0, 0.0],
    },
    Vertex {
      position: [0.5, 0.5, 0.5],
      tex_coord: [1.0, 1.0],
      normal: [0.0, 1.0, 0.0],
    },
    Vertex {
      position: [0.5, 0.5, -0.5],
      tex_coord: [1.0, 0.0],
      normal: [0.0, 1.0, 0.0],
    },
    Vertex {
      position: [-0.5, 0.5, -0.5],
      tex_coord: [0.0, 0.0],
      normal: [0.0, 1.0, 0.0],
    },
    // Bottom face (-y)
    Vertex {
      position: [-0.5, -0.5, -0.5],
      tex_coord: [0.0, 1.0],
      normal: [0.0, -1.0, 0.0],
    },
    Vertex {
      position: [0.5, -0.5, -0.5],
      tex_coord: [1.0, 1.0],
      normal: [0.0, -1.0, 0.0],
    },
    Vertex {
      position: [0.5, -0.5, 0.5],
      tex_coord: [1.0, 0.0],
      normal: [0.0, -1.0, 0.0],
    },
    Vertex {
      position: [-0.5, -0.5, 0.5],
      tex_coord: [0.0, 0.0],
      normal: [0.0, -1.0, 0.0],
    },
  ];

  // A cube needs 36 indices (6 faces, 2 triangles per face, 3 indices per triangle)
  let indices: Vec<u16> = vec![
    0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7, 8, 9, 10, 8, 10, 11, 12, 13, 14, 12, 14, 15, 16, 17, 18, 16, 18, 19, 20, 21, 22, 20, 22, 23,
  ];

  (vertices, indices)
}

// Returns a UV sphere of radius 0.5 centred on the origin.
// - `slices`: number of vertical segments (longitude, going around the equator). Clamped to a minimum of 3.
// - `stacks`: number of horizontal bands (latitude, pole to pole). Clamped to a minimum of 2.
// Higher values mean more polygons and a smoother sphere.
pub(crate) fn primitive_sphere(slices: u32, stacks: u32) -> (Vec<Vertex>, Vec<u16>) {
  use std::f32::consts::PI;

  let slices = slices.max(3);
  let stacks = stacks.max(2);
  let radius = 0.5;

  let mut vertices: Vec<Vertex> = Vec::with_capacity(((stacks + 1) * (slices + 1)) as usize);
  let mut indices: Vec<u16> = Vec::with_capacity((stacks * slices * 6) as usize);

  // Build a grid of vertices. We generate `slices + 1` columns so the seam has
  // duplicated vertices with tex_coord u = 0.0 and u = 1.0, giving clean UVs.
  for i in 0..=stacks {
    // phi runs from 0 (top pole, +y) to PI (bottom pole, -y).
    let phi = PI * i as f32 / stacks as f32;
    let (sin_phi, cos_phi) = phi.sin_cos();

    for j in 0..=slices {
      // theta runs around the vertical axis, 0 to 2*PI.
      let theta = 2.0 * PI * j as f32 / slices as f32;
      let (sin_theta, cos_theta) = theta.sin_cos();

      // Point on a unit sphere; the position is also the (already normalised) normal.
      let nx = sin_phi * cos_theta;
      let ny = cos_phi;
      let nz = sin_phi * sin_theta;

      vertices.push(Vertex {
        position: [nx * radius, ny * radius, nz * radius],
        tex_coord: [j as f32 / slices as f32, i as f32 / stacks as f32],
        normal: [nx, ny, nz],
      });
    }
  }

  // Two triangles per grid cell. Winding is counter-clockwise when viewed from
  // outside so faces point outwards (matches the engine's Ccw front face).
  let stride = slices + 1;
  for i in 0..stacks {
    for j in 0..slices {
      let a = (i * stride + j) as u16;
      let a_right = a + 1;
      let b = a + stride as u16;
      let b_right = b + 1;

      indices.extend_from_slice(&[a, a_right, b, a_right, b_right, b]);
    }
  }

  (vertices, indices)
}

// Mesh builder is a chainable helper to build primative and other meshes and provide materials/textures to them
pub struct MeshBuilder {
  verts: Vec<Vertex>,
  indices: Vec<u16>,
  material: MaterialHandle,
}

impl MeshBuilder {
  pub fn new(engine: &Engine) -> Self {
    Self {
      verts: Vec::new(),
      indices: Vec::new(),
      material: engine.default_material(), // Default material is a white texture
    }
  }

  pub fn add_primitive_cube(mut self) -> Self {
    let (verts, indices) = primitive_cube();
    self.verts.extend(verts);
    self.indices.extend(indices);
    self
  }

  // Add a UV sphere of radius 0.5. `slices` controls the vertical segments (longitude)
  // and `stacks` the horizontal bands (latitude); more of each means a smoother sphere.
  pub fn add_primitive_sphere(mut self, slices: u32, stacks: u32) -> Self {
    let base = self.verts.len() as u16;
    let (verts, indices) = primitive_sphere(slices, stacks);
    self.verts.extend(verts);
    // Offset indices by the vertices already present so this primitive can be combined with others.
    self.indices.extend(indices.into_iter().map(|idx| idx + base));
    self
  }

  pub fn set_material(mut self, material: MaterialHandle) -> Self {
    self.material = material;
    self
  }

  pub fn build(self, engine: &mut Engine) -> MeshHandle {
    let mesh = Mesh::new(engine, self.verts, self.indices, self.material);

    log::info!("MeshBuilder built mesh with blah and blah");

    engine.add_mesh(mesh)
  }
}
