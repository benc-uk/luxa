mod common;
mod engine;
mod helpers;
mod models;
mod nodes;
mod parser;
mod scenes;
mod transform;

pub use common::{Aabb, Color, Size};
pub use engine::{Engine, SkyboxMode};
pub use models::{AlphaMode, Material, Mesh, MeshBuilder, Vertex};
pub use nodes::Node;
pub use transform::Transform;

// Descriptors
pub use engine::NodeDescriptor;
pub use models::ModelDescriptor;
pub use nodes::{CameraDescriptor, CameraOrientation, LightDescriptor, MeshNodeDescriptor};
pub use scenes::SceneDescriptor;

// Handles
pub use engine::{MaterialHandle, MeshHandle, NodeHandle, SceneHandle, TextureHandle};
pub use nodes::{CameraHandle, LightHandle};

// Re-exports for convenience
pub use glam;
