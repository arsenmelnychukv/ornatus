//! Generates ornatus test fixtures and the default laptop model as `.glb`
//! bytes, from simple shapes with clean UVs (decision 0004).

mod glb;
mod laptop;

pub use glb::{MaterialSpec, MeshSpec, NodeSpec, SceneSpec};
pub use laptop::{grid_glb, laptop_glb, triangle_glb};
