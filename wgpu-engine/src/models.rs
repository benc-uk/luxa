mod builder;
mod material;
mod mesh_vert;
mod texture;

pub use builder::MeshBuilder;
pub use material::{Material, MaterialFallbacks};
pub use mesh_vert::{Mesh, Vertex};
pub use texture::Texture;
pub(crate) use texture::texture_or_fallback;
