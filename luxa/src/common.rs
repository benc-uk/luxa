use crate::models::Vertex;
use glam::Vec3;

#[derive(Copy, Clone, Debug)]
pub struct Size {
  pub width: u32,
  pub height: u32,
}

pub type Color = [f32; 4];

// An axis-aligned bounding box: the smallest box, aligned to the world axes, that encloses a mesh.
#[derive(Copy, Clone, Debug)]
pub struct Aabb {
  pub min: Vec3,
  pub max: Vec3,
}

impl Aabb {
  // Computes the AABB enclosing all vertex positions. Returns a zero-sized box at the origin when `vertices` is empty.
  pub(crate) fn from_vertices(vertices: &[Vertex]) -> Self {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);

    for vertex in vertices {
      let position = Vec3::from(vertex.position);
      min = min.min(position);
      max = max.max(position);
    }

    if vertices.is_empty() {
      min = Vec3::ZERO;
      max = Vec3::ZERO;
    }

    Aabb { min, max }
  }

  pub fn empty() -> Self {
    Aabb { min: Vec3::ZERO, max: Vec3::ZERO }
  }

  pub fn is_empty(&self) -> bool {
    self.min == self.max
  }

  pub fn contains_point(&self, point: Vec3) -> bool {
    point.x >= self.min.x && point.x <= self.max.x && point.y >= self.min.y && point.y <= self.max.y && point.z >= self.min.z && point.z <= self.max.z
  }

  pub fn union(&self, other: &Aabb) -> Aabb {
    Aabb {
      min: self.min.min(other.min),
      max: self.max.max(other.max),
    }
  }

  // The point at the centre of the box.
  pub fn center(&self) -> Vec3 {
    (self.min + self.max) * 0.5
  }

  // The full size of the box along each axis (`max - min`).
  pub fn size(&self) -> Vec3 {
    self.max - self.min
  }

  // The half-extents of the box, measured from its centre to a face along each axis.
  pub fn extents(&self) -> Vec3 {
    self.size() * 0.5
  }
}
