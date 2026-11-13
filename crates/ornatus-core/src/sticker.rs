//! Stickers as data (decision 0003): where an image sits on the paint
//! surface, how big it is, and how it is turned.
//!
//! A sticker is described in a frame that is undistorted on the surface.
//! UV space is usually stretched (a 0.30 by 0.21 panel maps onto a square), so
//! positions and sizes are measured in units of the UV width, and the
//! vertical direction is scaled by the surface's UV aspect `k` when the sticker
//! is turned into UV coordinates. That keeps a square image square on the model.

use glam::Vec2;

use crate::model::MeshData;

/// Identifies an imported image.
pub type ImageId = u32;

/// An image placed on the surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sticker {
    /// Stable identity, so a sticker can be found again after edits.
    pub id: u32,
    /// The image it shows.
    pub image: ImageId,
    /// Center in UV space.
    pub center: Vec2,
    /// Half width and half height, in units of the UV width.
    pub half: Vec2,
    /// Turn in radians. In UV space, where y runs down, positive turns clockwise.
    pub rotation: f32,
    /// 0 to 1.
    pub opacity: f32,
    /// Mirror the image on purpose (decision 0005: never mirrored by accident).
    pub flip_x: bool,
}

impl Sticker {
    /// The sticker's two half axes in UV space, for surface aspect `k`.
    pub fn axes(&self, k: f32) -> (Vec2, Vec2) {
        let (s, c) = self.rotation.sin_cos();
        (
            Vec2::new(c, s * k) * self.half.x,
            Vec2::new(-s, c * k) * self.half.y,
        )
    }

    /// UV position of a point given in the sticker's own frame, where
    /// (-1, -1) is the top left corner and (1, 1) the bottom right.
    pub fn uv_at(&self, local: Vec2, k: f32) -> Vec2 {
        let (ax, ay) = self.axes(k);
        self.center + ax * local.x + ay * local.y
    }

    /// The four corners in UV space: top left, top right, bottom right, bottom left.
    pub fn corners_uv(&self, k: f32) -> [Vec2; 4] {
        [
            Vec2::new(-1.0, -1.0),
            Vec2::new(1.0, -1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(-1.0, 1.0),
        ]
        .map(|corner| self.uv_at(corner, k))
    }

    /// A UV point in the sticker's frame measured in surface units (not
    /// normalized): x and y are offsets from the center along the sticker's
    /// own axes, in units of the UV width.
    pub fn frame_offset(&self, uv: Vec2, k: f32) -> Vec2 {
        let delta = Vec2::new(uv.x - self.center.x, (uv.y - self.center.y) / k);
        let (s, c) = self.rotation.sin_cos();
        Vec2::new(c * delta.x + s * delta.y, -s * delta.x + c * delta.y)
    }

    /// Whether a UV point lies on the sticker.
    pub fn contains(&self, uv: Vec2, k: f32) -> bool {
        let offset = self.frame_offset(uv, k);
        offset.x.abs() <= self.half.x && offset.y.abs() <= self.half.y
    }
}

/// How stretched the surface's UV mapping is: surface length of one unit of
/// u divided by surface length of one unit of v, averaged over the triangles
/// by area. 1 means a square UV cell is a square on the model.
pub fn surface_uv_aspect(mesh: &MeshData) -> f32 {
    let Some(uvs) = &mesh.uvs else { return 1.0 };
    let (mut along_u, mut along_v) = (0.0f64, 0.0f64);
    for tri in mesh.indices.as_chunks::<3>().0 {
        let i = tri.map(|i| i as usize);
        let [a, b, c] = i.map(|i| mesh.positions[i]);
        let [ua, ub, uc] = i.map(|i| Vec2::from(uvs[i]));
        let (e1, e2) = (b - a, c - a);
        let (d1, d2) = (ub - ua, uc - ua);
        let det = d1.x * d2.y - d2.x * d1.y;
        if det.abs() < 1e-12 {
            continue;
        }
        let du = (e1 * d2.y - e2 * d1.y) / det;
        let dv = (e2 * d1.x - e1 * d2.x) / det;
        let area = f64::from(e1.cross(e2).length());
        along_u += area * f64::from(du.length());
        along_v += area * f64::from(dv.length());
    }
    if along_u > 0.0 && along_v > 0.0 {
        (along_u / along_v) as f32
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::load_glb;

    fn sticker() -> Sticker {
        Sticker {
            id: 1,
            image: 0,
            center: Vec2::new(0.5, 0.5),
            half: Vec2::new(0.1, 0.05),
            rotation: 0.0,
            opacity: 1.0,
            flip_x: false,
        }
    }

    #[test]
    fn corners_of_an_unturned_sticker() {
        let corners = sticker().corners_uv(1.0);
        assert!((corners[0] - Vec2::new(0.4, 0.45)).length() < 1e-6);
        assert!((corners[2] - Vec2::new(0.6, 0.55)).length() < 1e-6);
    }

    #[test]
    fn the_surface_aspect_stretches_only_the_vertical_direction() {
        let tall = sticker().corners_uv(2.0);
        assert!((tall[0] - Vec2::new(0.4, 0.4)).length() < 1e-6);
        assert!((tall[2] - Vec2::new(0.6, 0.6)).length() < 1e-6);
    }

    #[test]
    fn contains_matches_the_rotated_rectangle() {
        let mut s = sticker();
        assert!(s.contains(Vec2::new(0.59, 0.54), 1.0));
        assert!(!s.contains(Vec2::new(0.59, 0.56), 1.0));
        // Turned a quarter, the long side now runs vertically.
        s.rotation = std::f32::consts::FRAC_PI_2;
        assert!(s.contains(Vec2::new(0.54, 0.59), 1.0));
        assert!(!s.contains(Vec2::new(0.59, 0.54), 1.0));
    }

    #[test]
    fn frame_offset_inverts_uv_at() {
        let mut s = sticker();
        s.rotation = 0.7;
        let k = 1.43;
        let local = Vec2::new(0.3, -0.8);
        let uv = s.uv_at(local, k);
        let offset = s.frame_offset(uv, k);
        assert!((offset.x - local.x * s.half.x).abs() < 1e-5);
        assert!((offset.y - local.y * s.half.y).abs() < 1e-5);
    }

    #[test]
    fn the_lid_panel_is_wider_than_tall() {
        let model = load_glb(&ornatus_fixtures::laptop_glb()).unwrap().model;
        let lid = model.meshes.iter().find(|m| m.name == "Lid_Outer").unwrap();
        // 0.30 m along u and 0.21 m along v.
        let k = surface_uv_aspect(lid);
        assert!((k - 0.30 / 0.21).abs() < 0.01, "k = {k}");
    }

    #[test]
    fn a_square_grid_has_aspect_one_and_no_uvs_default_to_one() {
        let model = load_glb(&ornatus_fixtures::grid_glb(8)).unwrap().model;
        assert!((surface_uv_aspect(&model.meshes[0]) - 1.0).abs() < 1e-3);
        let flawed = load_glb(&ornatus_fixtures::flawed_glb()).unwrap().model;
        let no_uv = flawed
            .meshes
            .iter()
            .find(|m| m.name == "NoUV_Quad")
            .unwrap();
        assert_eq!(surface_uv_aspect(no_uv), 1.0);
    }
}
