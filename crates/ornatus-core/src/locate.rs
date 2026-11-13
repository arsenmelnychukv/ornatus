//! Finding where a UV coordinate sits on the surface, the reverse of picking.
//! Used to draw sticker handles on the model.

use glam::{Vec2, Vec3};

use crate::model::MeshData;

const CELLS: usize = 128;

/// A grid over the UV square that lists the triangles near each cell.
#[derive(Debug, Clone, Default)]
pub struct UvLocator {
    cells: Vec<Vec<u32>>,
}

impl UvLocator {
    /// Index a mesh. Meshes without UVs give a locator that finds nothing.
    pub fn new(mesh: &MeshData) -> Self {
        let Some(uvs) = &mesh.uvs else {
            return Self::default();
        };
        let mut cells = vec![Vec::new(); CELLS * CELLS];
        for (t, tri) in mesh.indices.as_chunks::<3>().0.iter().enumerate() {
            let uv = tri.map(|i| Vec2::from(uvs[i as usize]));
            let min = uv[0].min(uv[1]).min(uv[2]).clamp(Vec2::ZERO, Vec2::ONE);
            let max = uv[0].max(uv[1]).max(uv[2]).clamp(Vec2::ZERO, Vec2::ONE);
            let (x0, y0) = cell(min);
            let (x1, y1) = cell(max);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    cells[y * CELLS + x].push(t as u32);
                }
            }
        }
        Self { cells }
    }

    /// The world position of a UV coordinate, if it lies on a triangle.
    pub fn world_point(&self, mesh: &MeshData, uv: Vec2) -> Option<Vec3> {
        let uvs = mesh.uvs.as_ref()?;
        if self.cells.is_empty() || !(0.0..=1.0).contains(&uv.x) || !(0.0..=1.0).contains(&uv.y) {
            return None;
        }
        let (x, y) = cell(uv);
        for &t in &self.cells[y * CELLS + x] {
            let i = &mesh.indices[t as usize * 3..t as usize * 3 + 3];
            let i = [i[0] as usize, i[1] as usize, i[2] as usize];
            let [a, b, c] = i.map(|i| Vec2::from(uvs[i]));
            let det = (b - a).perp_dot(c - a);
            if det.abs() < 1e-12 {
                continue;
            }
            let (v, w) = (
                (uv - a).perp_dot(c - a) / det,
                (b - a).perp_dot(uv - a) / det,
            );
            let u = 1.0 - v - w;
            if u >= -1e-5 && v >= -1e-5 && w >= -1e-5 {
                let [pa, pb, pc] = i.map(|i| mesh.positions[i]);
                return Some(pa * u + pb * v + pc * w);
            }
        }
        None
    }
}

fn cell(uv: Vec2) -> (usize, usize) {
    let to = |v: f32| ((v * CELLS as f32) as usize).min(CELLS - 1);
    (to(uv.x), to(uv.y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::load_glb;

    #[test]
    fn uv_points_map_back_to_the_surface() {
        let model = load_glb(&ornatus_fixtures::laptop_glb()).unwrap().model;
        let lid = model.meshes.iter().find(|m| m.name == "Lid_Outer").unwrap();
        let locator = UvLocator::new(lid);
        let center = lid.positions.iter().sum::<Vec3>() / lid.positions.len() as f32;
        let found = locator
            .world_point(lid, Vec2::splat(0.5))
            .expect("on the lid");
        assert!((found - center).length() < 1e-4);
        // Each corner maps to the vertex that carries that UV.
        for (position, uv) in lid.positions.iter().zip(lid.uvs.as_ref().unwrap()) {
            let uv = Vec2::from(*uv).clamp(Vec2::splat(0.001), Vec2::splat(0.999));
            let found = locator.world_point(lid, uv).expect("near a corner");
            assert!((found - *position).length() < 0.01, "{found} vs {position}");
        }
    }

    #[test]
    fn outside_the_uv_square_or_without_uvs_finds_nothing() {
        let model = load_glb(&ornatus_fixtures::laptop_glb()).unwrap().model;
        let lid = &model.meshes[4];
        let locator = UvLocator::new(lid);
        assert!(locator.world_point(lid, Vec2::new(1.5, 0.5)).is_none());
        let flawed = load_glb(&ornatus_fixtures::flawed_glb()).unwrap().model;
        let no_uv = flawed
            .meshes
            .iter()
            .find(|m| m.name == "NoUV_Quad")
            .unwrap();
        assert!(
            UvLocator::new(no_uv)
                .world_point(no_uv, Vec2::splat(0.5))
                .is_none()
        );
    }

    #[test]
    fn gaps_between_islands_find_nothing() {
        let mut model = load_glb(&ornatus_fixtures::grid_glb(4)).unwrap().model;
        // Shrink the UVs into one corner, leaving most of the square empty.
        for uv in model.meshes[0].uvs.as_mut().unwrap() {
            *uv = [uv[0] * 0.25, uv[1] * 0.25];
        }
        let mesh = &model.meshes[0];
        let locator = UvLocator::new(mesh);
        assert!(locator.world_point(mesh, Vec2::splat(0.1)).is_some());
        assert!(locator.world_point(mesh, Vec2::splat(0.8)).is_none());
    }
}
