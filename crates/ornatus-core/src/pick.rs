//! Picking: shoot a ray at the model and find the surface point, UV and normal
//! under it (decision 0003: only the selected surface receives hits, and
//! rays never paint on back-facing triangles).
//!
//! Each mesh gets a bounding volume hierarchy, so a ray only visits the few
//! triangles near it, even on surfaces with millions of triangles.

use glam::Vec3;

use crate::model::{MeshData, Model};

/// Triangles per leaf before the builder stops splitting.
const LEAF_SIZE: usize = 4;
const STACK_SIZE: usize = 64;

/// A ray with a unit-length direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray {
    /// Where the ray starts.
    pub origin: Vec3,
    /// Unit direction.
    pub dir: Vec3,
}

impl Ray {
    /// A ray from `origin` towards `dir`, which is normalized here.
    pub fn new(origin: Vec3, dir: Vec3) -> Self {
        Self {
            origin,
            dir: dir.normalize_or_zero(),
        }
    }

    /// The point `t` units along the ray.
    pub fn at(&self, t: f32) -> Vec3 {
        self.origin + self.dir * t
    }
}

/// Where a ray met a surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceHit {
    /// Index of the mesh in `Model::meshes`.
    pub mesh: usize,
    /// Triangle number within the mesh (index list position divided by three).
    pub triangle: u32,
    /// Distance along the ray.
    pub distance: f32,
    /// The hit point in world space.
    pub point: Vec3,
    /// Unit normal of the side the triangle faces (its outward side).
    pub normal: Vec3,
    /// Weights of the triangle's three vertices at the hit point.
    pub barycentric: [f32; 3],
    /// Interpolated UV, if the mesh has UVs.
    pub uv: Option<[f32; 2]>,
    /// True when the ray arrived from the triangle's outward side.
    pub front_face: bool,
}

/// What a ray found, judged against the surface the user wants to paint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pick {
    /// The nearest thing under the ray is the target surface, seen from outside.
    Hit(SurfaceHit),
    /// Another surface is in front of the target.
    Occluded(SurfaceHit),
    /// The nearest thing is the target, but seen from behind.
    BackFacing(SurfaceHit),
    /// The ray hit nothing.
    Miss,
}

#[derive(Debug, Clone, Copy)]
struct Node {
    min: Vec3,
    max: Vec3,
    /// Leaf: first entry in the triangle order. Inner: index of the right child.
    start: u32,
    /// Leaf: triangle count. Inner: zero. The left child follows the node.
    count: u32,
}

/// A bounding volume hierarchy over one mesh's triangles.
#[derive(Debug, Clone, Default)]
pub struct Bvh {
    nodes: Vec<Node>,
    order: Vec<u32>,
}

fn triangle(mesh: &MeshData, t: usize) -> [Vec3; 3] {
    let i = &mesh.indices[t * 3..t * 3 + 3];
    [
        mesh.positions[i[0] as usize],
        mesh.positions[i[1] as usize],
        mesh.positions[i[2] as usize],
    ]
}

/// A triangle's bounds, kept together so the builder reads memory in order.
#[derive(Clone, Copy)]
struct Prim {
    min: Vec3,
    max: Vec3,
    centroid: Vec3,
    id: u32,
}

impl Bvh {
    /// Build the hierarchy for a mesh.
    pub fn new(mesh: &MeshData) -> Self {
        let count = mesh.indices.len() / 3;
        let mut prims: Vec<Prim> = (0..count)
            .map(|t| {
                let [a, b, c] = triangle(mesh, t);
                Prim {
                    min: a.min(b).min(c),
                    max: a.max(b).max(c),
                    centroid: (a + b + c) / 3.0,
                    id: t as u32,
                }
            })
            .collect();
        let mut nodes = Vec::with_capacity(count / 2 + 1);
        if count > 0 {
            build(&mut prims, 0, &mut nodes);
        }
        Self {
            nodes,
            order: prims.iter().map(|p| p.id).collect(),
        }
    }

    /// The nearest triangle hit closer than `t_max`, as `(triangle, t, u, v)`.
    /// Both sides of a triangle can be hit.
    fn closest(&self, mesh: &MeshData, ray: &Ray, mut t_max: f32) -> Option<(u32, f32, f32, f32)> {
        if self.nodes.is_empty() {
            return None;
        }
        let inv = ray.dir.recip();
        let mut best = None;
        let mut stack = [0u32; STACK_SIZE];
        let mut top = 1;
        while top > 0 {
            top -= 1;
            let index = stack[top] as usize;
            let node = &self.nodes[index];
            if !hits_box(node, ray.origin, inv, t_max) {
                continue;
            }
            if node.count > 0 {
                for &t in &self.order[node.start as usize..(node.start + node.count) as usize] {
                    let [a, b, c] = triangle(mesh, t as usize);
                    if let Some((distance, u, v)) = ray_triangle(ray, a, b, c)
                        && distance < t_max
                    {
                        t_max = distance;
                        best = Some((t, distance, u, v));
                    }
                }
            } else if top + 2 <= STACK_SIZE {
                stack[top] = node.start;
                stack[top + 1] = index as u32 + 1;
                top += 2;
            }
        }
        best
    }
}

/// Build the subtree for `prims`, which start at `offset` in the final order.
fn build(prims: &mut [Prim], offset: usize, nodes: &mut Vec<Node>) {
    let (mut min, mut max) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    let (mut cmin, mut cmax) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for prim in prims.iter() {
        min = min.min(prim.min);
        max = max.max(prim.max);
        cmin = cmin.min(prim.centroid);
        cmax = cmax.max(prim.centroid);
    }
    let me = nodes.len();
    nodes.push(Node {
        min,
        max,
        start: offset as u32,
        count: prims.len() as u32,
    });
    let extent = cmax - cmin;
    if prims.len() <= LEAF_SIZE || extent.max_element() <= f32::EPSILON {
        return;
    }
    let axis = if extent.x >= extent.y && extent.x >= extent.z {
        0
    } else if extent.y >= extent.z {
        1
    } else {
        2
    };
    let middle = prims.len() / 2;
    prims.select_nth_unstable_by(middle, |a, b| a.centroid[axis].total_cmp(&b.centroid[axis]));
    let (left, right) = prims.split_at_mut(middle);
    build(left, offset, nodes);
    let right_node = nodes.len() as u32;
    build(right, offset + middle, nodes);
    nodes[me].start = right_node;
    nodes[me].count = 0;
}

/// Slab test of a ray against a node's box.
fn hits_box(node: &Node, origin: Vec3, inv_dir: Vec3, t_max: f32) -> bool {
    let t1 = (node.min - origin) * inv_dir;
    let t2 = (node.max - origin) * inv_dir;
    let near = t1.min(t2).max_element().max(0.0);
    let far = t1.max(t2).min_element().min(t_max);
    near <= far
}

/// Moller-Trumbore ray and triangle test, hitting both sides. Returns the
/// distance along the ray and the barycentric weights of the second and
/// third vertex.
pub fn ray_triangle(ray: &Ray, a: Vec3, b: Vec3, c: Vec3) -> Option<(f32, f32, f32)> {
    let (e1, e2) = (b - a, c - a);
    let p = ray.dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let s = ray.origin - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = ray.dir.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 1e-6).then_some((t, u, v))
}

/// Bounding volume hierarchies for every mesh of a model.
#[derive(Debug, Clone, Default)]
pub struct PickScene {
    bvhs: Vec<Bvh>,
}

impl PickScene {
    /// Build the hierarchies. Costs a moment on very large models.
    pub fn new(model: &Model) -> Self {
        Self {
            bvhs: model.meshes.iter().map(Bvh::new).collect(),
        }
    }

    /// The nearest surface under the ray, whichever mesh and side it is on.
    pub fn nearest(&self, model: &Model, ray: &Ray) -> Option<SurfaceHit> {
        let mut best: Option<(usize, u32, f32, f32, f32)> = None;
        let mut t_max = f32::MAX;
        for (mesh_index, (mesh, bvh)) in model.meshes.iter().zip(&self.bvhs).enumerate() {
            if let Some((triangle, t, u, v)) = bvh.closest(mesh, ray, t_max) {
                t_max = t;
                best = Some((mesh_index, triangle, t, u, v));
            }
        }
        let (mesh_index, triangle, t, u, v) = best?;
        Some(describe(
            &model.meshes[mesh_index],
            mesh_index,
            triangle,
            t,
            u,
            v,
            ray,
        ))
    }

    /// What the ray finds, judged against the surface the user wants to paint on.
    pub fn pick(&self, model: &Model, ray: &Ray, target: usize) -> Pick {
        match self.nearest(model, ray) {
            None => Pick::Miss,
            Some(hit) if hit.mesh != target => Pick::Occluded(hit),
            Some(hit) if hit.front_face => Pick::Hit(hit),
            Some(hit) => Pick::BackFacing(hit),
        }
    }
}

fn describe(
    mesh: &MeshData,
    mesh_index: usize,
    triangle: u32,
    t: f32,
    u: f32,
    v: f32,
    ray: &Ray,
) -> SurfaceHit {
    let i = &mesh.indices[triangle as usize * 3..triangle as usize * 3 + 3];
    let i = [i[0] as usize, i[1] as usize, i[2] as usize];
    let weights = [1.0 - u - v, u, v];
    let [a, b, c] = i.map(|i| mesh.positions[i]);
    let mut normal = i.iter().map(|&i| mesh.normals[i]).sum::<Vec3>();
    if normal.length_squared() < 1e-12 {
        normal = (b - a).cross(c - a);
    }
    let normal = normal.normalize_or_zero();
    let uv = mesh.uvs.as_ref().map(|uvs| {
        let mut out = [0.0f32; 2];
        for (w, &i) in weights.iter().zip(&i) {
            out[0] += w * uvs[i][0];
            out[1] += w * uvs[i][1];
        }
        out
    });
    SurfaceHit {
        mesh: mesh_index,
        triangle,
        distance: t,
        point: ray.at(t),
        normal,
        barycentric: weights,
        uv,
        front_face: normal.dot(ray.dir) < 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::load_glb;

    fn laptop() -> Model {
        load_glb(&ornatus_fixtures::laptop_glb()).unwrap().model
    }

    fn index_of(model: &Model, name: &str) -> usize {
        model.meshes.iter().position(|m| m.name == name).unwrap()
    }

    #[test]
    fn ray_hits_and_misses_a_triangle() {
        let (a, b, c) = (Vec3::ZERO, Vec3::X, Vec3::Y);
        let down = Ray::new(Vec3::new(0.25, 0.25, 1.0), -Vec3::Z);
        let (t, u, v) = ray_triangle(&down, a, b, c).expect("hit");
        assert!((t - 1.0).abs() < 1e-6 && (u - 0.25).abs() < 1e-6 && (v - 0.25).abs() < 1e-6);
        assert!(ray_triangle(&Ray::new(Vec3::new(2.0, 2.0, 1.0), -Vec3::Z), a, b, c).is_none());
        assert!(ray_triangle(&Ray::new(Vec3::new(0.25, 0.25, 1.0), Vec3::X), a, b, c).is_none());
        // Behind the ray start does not count.
        assert!(ray_triangle(&Ray::new(Vec3::new(0.25, 0.25, -1.0), -Vec3::Z), a, b, c).is_none());
    }

    #[test]
    fn bvh_agrees_with_brute_force() {
        let model = load_glb(&ornatus_fixtures::grid_glb(40)).unwrap().model;
        let mesh = &model.meshes[0];
        let bvh = Bvh::new(mesh);
        let mut state = 12345u32;
        let mut random = move || {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            (state >> 8) as f32 / (1u32 << 24) as f32
        };
        for _ in 0..500 {
            let origin = Vec3::new(random() * 1.4 - 0.2, 1.0, random() * 1.4 - 0.2);
            let target = Vec3::new(random() * 1.4 - 0.2, 0.0, random() * 1.4 - 0.2);
            let ray = Ray::new(origin, target - origin);
            let brute = (0..mesh.indices.len() / 3)
                .filter_map(|t| {
                    let [a, b, c] = triangle(mesh, t);
                    ray_triangle(&ray, a, b, c).map(|(d, _, _)| (t as u32, d))
                })
                .min_by(|x, y| x.1.total_cmp(&y.1));
            let fast = bvh.closest(mesh, &ray, f32::MAX).map(|(t, d, _, _)| (t, d));
            match (brute, fast) {
                (None, None) => {}
                (Some(x), Some(y)) => assert!((x.1 - y.1).abs() < 1e-5, "{x:?} vs {y:?}"),
                other => panic!("disagree: {other:?}"),
            }
        }
    }

    /// Center and outward normal of the lid's outer panel.
    fn lid_center_and_normal(model: &Model) -> (usize, Vec3, Vec3) {
        let index = index_of(model, "Lid_Outer");
        let mesh = &model.meshes[index];
        let center = mesh.positions.iter().sum::<Vec3>() / mesh.positions.len() as f32;
        (index, center, mesh.normals[0])
    }

    #[test]
    fn ray_at_the_lid_center_reports_the_middle_of_the_uv_square() {
        let model = laptop();
        let scene = PickScene::new(&model);
        let (lid, center, normal) = lid_center_and_normal(&model);
        let ray = Ray::new(center + normal * 0.5, -normal);
        let Pick::Hit(hit) = scene.pick(&model, &ray, lid) else {
            panic!("expected a hit");
        };
        let uv = hit.uv.expect("uv");
        assert!(
            (uv[0] - 0.5).abs() < 1e-3 && (uv[1] - 0.5).abs() < 1e-3,
            "{uv:?}"
        );
        assert!((hit.point - center).length() < 1e-4);
        assert!(hit.normal.dot(normal) > 0.999 && hit.front_face);
        assert!((hit.distance - 0.5).abs() < 1e-4);
    }

    #[test]
    fn rays_near_the_corners_report_the_corner_uvs() {
        let model = laptop();
        let scene = PickScene::new(&model);
        let (lid, center, normal) = lid_center_and_normal(&model);
        let mesh = &model.meshes[lid];
        for (corner, uv) in mesh.positions.iter().zip(mesh.uvs.as_ref().unwrap()) {
            let aim = center.lerp(*corner, 0.98);
            let Pick::Hit(hit) = scene.pick(&model, &Ray::new(aim + normal, -normal), lid) else {
                panic!("expected a hit near a corner");
            };
            let got = hit.uv.unwrap();
            assert!(
                (got[0] - uv[0]).abs() < 0.03 && (got[1] - uv[1]).abs() < 0.03,
                "{got:?} vs {uv:?}"
            );
        }
    }

    #[test]
    fn the_screen_blocks_the_lid_seen_from_the_user_side() {
        let model = laptop();
        let scene = PickScene::new(&model);
        let (lid, center, normal) = lid_center_and_normal(&model);
        let ray = Ray::new(center - normal * 0.5, normal);
        match scene.pick(&model, &ray, lid) {
            Pick::Occluded(hit) => assert_eq!(hit.mesh, index_of(&model, "Lid_Screen")),
            other => panic!("expected the screen to block, got {other:?}"),
        }
    }

    #[test]
    fn back_faces_are_not_hit() {
        let model = load_glb(&ornatus_fixtures::flawed_glb()).unwrap().model;
        let scene = PickScene::new(&model);
        let quad = index_of(&model, "Clean_Quad");
        let from_below = Ray::new(Vec3::new(-1.0, -1.0, 0.0), Vec3::Y);
        assert!(matches!(
            scene.pick(&model, &from_below, quad),
            Pick::BackFacing(_)
        ));
        let from_above = Ray::new(Vec3::new(-1.0, 1.0, 0.0), -Vec3::Y);
        assert!(matches!(
            scene.pick(&model, &from_above, quad),
            Pick::Hit(_)
        ));
    }

    #[test]
    fn rays_that_hit_nothing_miss() {
        let model = laptop();
        let scene = PickScene::new(&model);
        let ray = Ray::new(Vec3::new(0.0, 10.0, 0.0), Vec3::Y);
        assert_eq!(scene.pick(&model, &ray, 0), Pick::Miss);
    }

    #[test]
    fn degenerate_triangles_do_not_break_the_bvh() {
        let mut model = laptop();
        let mesh = &mut model.meshes[0];
        // Collapse every vertex to one point: all centroids are equal, so the
        // builder must stop splitting and make one big leaf.
        for p in &mut mesh.positions {
            *p = Vec3::ZERO;
        }
        let scene = PickScene::new(&model);
        let ray = Ray::new(Vec3::new(0.0, 5.0, 0.0), -Vec3::Y);
        let _ = scene.nearest(&model, &ray);
    }

    #[test]
    fn empty_models_and_meshes_are_safe() {
        let scene = PickScene::new(&Model::default());
        let ray = Ray::new(Vec3::ZERO, Vec3::Z);
        assert!(scene.nearest(&Model::default(), &ray).is_none());
    }
}
