//! Model data and the `.glb` loader (decision 0004).
//!
//! Only plain, self-contained `.glb` files are accepted. Node transforms are
//! baked into the vertex data, so every mesh is in world space after loading.

use glam::{Mat3, Mat4, Vec3};
use thiserror::Error;

/// Files above this size load with a warning (decision 0004).
pub const SOFT_SIZE_LIMIT_BYTES: usize = 100 * 1024 * 1024;

/// One triangle mesh, in world space.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshData {
    /// Mesh name from the file, or a generated one.
    pub name: String,
    /// Material name from the file, if it has one.
    pub material: Option<String>,
    /// Base color (linear RGBA) from the material.
    pub base_color: [f32; 4],
    /// Metalness from the material, 0 to 1.
    pub metallic: f32,
    /// Roughness from the material, 0 to 1.
    pub roughness: f32,
    /// Vertex positions.
    pub positions: Vec<Vec3>,
    /// Vertex normals, one per position.
    pub normals: Vec<Vec3>,
    /// First UV set, if the file has one.
    pub uvs: Option<Vec<[f32; 2]>>,
    /// Triangle list indices.
    pub indices: Vec<u32>,
}

/// A loaded model.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Model {
    /// All triangle meshes found in the default scene.
    pub meshes: Vec<MeshData>,
}

/// Something worth telling the user that did not stop the load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadWarning {
    /// The file is larger than [`SOFT_SIZE_LIMIT_BYTES`].
    LargeFile { bytes: usize },
    /// A primitive that is not a triangle list was skipped.
    SkippedPrimitive { mesh: String },
}

/// A successful load.
#[derive(Debug, Clone)]
pub struct Loaded {
    /// The model.
    pub model: Model,
    /// Non-fatal issues.
    pub warnings: Vec<LoadWarning>,
}

/// Why a file could not be loaded.
#[derive(Debug, Error)]
pub enum LoadError {
    /// The bytes are not a valid glTF binary.
    #[error("not a valid .glb file: {0}")]
    Parse(String),
    /// The file needs a feature this version does not support.
    #[error("unsupported glTF extension(s): {0}. Only plain .glb files are supported")]
    UnsupportedExtension(String),
    /// Buffers or geometry live outside the file.
    #[error("the file refers to external data. Use a self-contained .glb")]
    ExternalData,
    /// The default scene has no triangle meshes.
    #[error("the file contains no triangle meshes")]
    NoMeshes,
    /// The geometry is inconsistent, for example an index past the last vertex.
    #[error("the file has broken geometry: {0}")]
    Invalid(String),
}

/// Deepest node hierarchy the loader follows, to stay clear of stack overflows.
const MAX_NODE_DEPTH: usize = 256;

/// Load a `.glb` from bytes.
pub fn load_glb(bytes: &[u8]) -> Result<Loaded, LoadError> {
    let gltf = gltf::Gltf::from_slice(bytes).map_err(|e| LoadError::Parse(e.to_string()))?;
    let unsupported: Vec<&str> = gltf.document.extensions_required().collect();
    if !unsupported.is_empty() {
        return Err(LoadError::UnsupportedExtension(unsupported.join(", ")));
    }
    let blob = gltf.blob.as_deref();
    let scene = gltf
        .document
        .default_scene()
        .or_else(|| gltf.document.scenes().next())
        .ok_or(LoadError::NoMeshes)?;

    let mut model = Model::default();
    let mut warnings = Vec::new();
    if bytes.len() > SOFT_SIZE_LIMIT_BYTES {
        warnings.push(LoadWarning::LargeFile { bytes: bytes.len() });
    }
    for node in scene.nodes() {
        visit(&node, Mat4::IDENTITY, 0, blob, &mut model, &mut warnings)?;
    }
    if model.meshes.is_empty() {
        return Err(LoadError::NoMeshes);
    }
    Ok(Loaded { model, warnings })
}

fn visit(
    node: &gltf::Node<'_>,
    parent: Mat4,
    depth: usize,
    blob: Option<&[u8]>,
    model: &mut Model,
    warnings: &mut Vec<LoadWarning>,
) -> Result<(), LoadError> {
    if depth > MAX_NODE_DEPTH {
        return Err(LoadError::Invalid("node hierarchy is too deep".to_owned()));
    }
    let world = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
    if let Some(mesh) = node.mesh() {
        let base = mesh
            .name()
            .or_else(|| node.name())
            .unwrap_or("Mesh")
            .to_owned();
        let multi = mesh.primitives().len() > 1;
        for (index, primitive) in mesh.primitives().enumerate() {
            if primitive.mode() != gltf::mesh::Mode::Triangles {
                warnings.push(LoadWarning::SkippedPrimitive { mesh: base.clone() });
                continue;
            }
            let name = if multi {
                format!("{base}.{index}")
            } else {
                base.clone()
            };
            model
                .meshes
                .push(read_primitive(&primitive, name, world, blob)?);
        }
    }
    for child in node.children() {
        visit(&child, world, depth + 1, blob, model, warnings)?;
    }
    Ok(())
}

fn read_primitive(
    primitive: &gltf::Primitive<'_>,
    name: String,
    world: Mat4,
    blob: Option<&[u8]>,
) -> Result<MeshData, LoadError> {
    let reader = primitive.reader(|buffer| match buffer.source() {
        gltf::buffer::Source::Bin => blob,
        gltf::buffer::Source::Uri(_) => None,
    });
    let local: Vec<Vec3> = reader
        .read_positions()
        .ok_or(LoadError::ExternalData)?
        .map(Vec3::from)
        .collect();
    let indices: Vec<u32> = match reader.read_indices() {
        Some(indices) => indices.into_u32().collect(),
        None => {
            let count = u32::try_from(local.len())
                .map_err(|_| LoadError::Invalid("too many vertices".to_owned()))?;
            (0..count).collect()
        }
    };
    if indices.iter().any(|&i| i as usize >= local.len()) {
        return Err(LoadError::Invalid(
            "an index points past the last vertex".to_owned(),
        ));
    }
    let positions: Vec<Vec3> = local.iter().map(|p| world.transform_point3(*p)).collect();
    let normals = match reader.read_normals() {
        Some(normals) => {
            let normal_matrix = Mat3::from_mat4(world).inverse().transpose();
            let normals: Vec<Vec3> = normals
                .map(|n| (normal_matrix * Vec3::from(n)).normalize_or_zero())
                .collect();
            if normals.len() != positions.len() {
                return Err(LoadError::Invalid(
                    "normal count differs from vertex count".to_owned(),
                ));
            }
            normals
        }
        None => smooth_normals(&positions, &indices),
    };
    let uvs: Option<Vec<[f32; 2]>> = reader
        .read_tex_coords(0)
        .map(|uvs| uvs.into_f32().collect());
    if uvs.as_ref().is_some_and(|uvs| uvs.len() != positions.len()) {
        return Err(LoadError::Invalid(
            "UV count differs from vertex count".to_owned(),
        ));
    }
    let pbr = primitive.material().pbr_metallic_roughness();
    Ok(MeshData {
        name,
        material: primitive.material().name().map(str::to_owned),
        base_color: pbr.base_color_factor(),
        metallic: pbr.metallic_factor(),
        roughness: pbr.roughness_factor(),
        positions,
        normals,
        uvs,
        indices,
    })
}

/// Area-weighted vertex normals, used when the file has none.
pub fn smooth_normals(positions: &[Vec3], indices: &[u32]) -> Vec<Vec3> {
    let mut normals = vec![Vec3::ZERO; positions.len()];
    for &[a, b, c] in indices.as_chunks::<3>().0 {
        let [a, b, c] = [a as usize, b as usize, c as usize];
        let face = (positions[b] - positions[a]).cross(positions[c] - positions[a]);
        normals[a] += face;
        normals[b] += face;
        normals[c] += face;
    }
    normals.into_iter().map(Vec3::normalize_or_zero).collect()
}

impl Model {
    /// Axis-aligned bounds of every vertex, or `None` for an empty model.
    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        let mut points = self.meshes.iter().flat_map(|m| m.positions.iter().copied());
        let first = points.next()?;
        Some(points.fold((first, first), |(min, max), p| (min.min(p), max.max(p))))
    }

    /// Move the model so its bounds are centered on the origin and scale it
    /// so the longest side is `size` units. Returns the scale that was applied.
    pub fn center_and_fit(&mut self, size: f32) -> Option<f32> {
        let (min, max) = self.bounds()?;
        let center = (min + max) / 2.0;
        let longest = (max - min).max_element();
        if longest <= f32::EPSILON {
            return None;
        }
        let scale = size / longest;
        for mesh in &mut self.meshes {
            for p in &mut mesh.positions {
                *p = (*p - center) * scale;
            }
        }
        Some(scale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_generated_laptop() {
        let loaded = load_glb(&ornatus_fixtures::laptop_glb()).expect("laptop loads");
        let names: Vec<&str> = loaded
            .model
            .meshes
            .iter()
            .map(|m| m.name.as_str())
            .collect();
        assert_eq!(
            names,
            ["Base", "Hinge", "Lid_Body", "Lid_Screen", "Lid_Outer"]
        );
        assert!(loaded.warnings.is_empty());
        assert!(loaded.model.meshes.iter().all(|m| m.uvs.is_some()));
        let outer = &loaded.model.meshes[4];
        assert_eq!(outer.material.as_deref(), Some("LidOuter"));
        assert!((outer.metallic - 0.9).abs() < 1e-6);
        assert!((outer.roughness - 0.4).abs() < 1e-6);
    }

    #[test]
    fn node_transforms_are_baked() {
        let loaded = load_glb(&ornatus_fixtures::laptop_glb()).unwrap();
        let outer = &loaded.model.meshes[4];
        // The lid is opened about 100 degrees, so its outer face points
        // backwards and slightly down, away from the user.
        let normal = outer.normals[0];
        assert!(normal.z < -0.9, "normal was {normal}");
        assert!(normal.y < 0.0 && normal.y > -0.3, "normal was {normal}");
        // The lid is raised above the base.
        assert!(outer.positions.iter().any(|p| p.y > 0.1));
    }

    #[test]
    fn missing_uvs_are_reported_as_none() {
        let loaded = load_glb(&ornatus_fixtures::triangle_glb(false)).unwrap();
        assert!(loaded.model.meshes[0].uvs.is_none());
        let loaded = load_glb(&ornatus_fixtures::triangle_glb(true)).unwrap();
        assert_eq!(loaded.model.meshes[0].uvs.as_ref().map(Vec::len), Some(3));
    }

    fn triangles(mesh: &MeshData) -> impl Iterator<Item = [usize; 3]> + '_ {
        mesh.indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|&[a, b, c]| [a as usize, b as usize, c as usize])
    }

    #[test]
    fn winding_agrees_with_normals() {
        let loaded = load_glb(&ornatus_fixtures::laptop_glb()).unwrap();
        for mesh in &loaded.model.meshes {
            for [a, b, c] in triangles(mesh) {
                let face = (mesh.positions[b] - mesh.positions[a])
                    .cross(mesh.positions[c] - mesh.positions[a]);
                assert!(
                    face.dot(mesh.normals[a]) > 0.0,
                    "{} has a reversed triangle",
                    mesh.name
                );
            }
        }
    }

    /// Seen from outside, an unmirrored texture has u running right and v
    /// running down, so du x dv points into the surface.
    fn uv_is_unmirrored(mesh: &MeshData) -> bool {
        let uvs = mesh.uvs.as_ref().expect("uvs");
        triangles(mesh).all(|[a, b, c]| {
            let e1 = mesh.positions[b] - mesh.positions[a];
            let e2 = mesh.positions[c] - mesh.positions[a];
            let d1 = [uvs[b][0] - uvs[a][0], uvs[b][1] - uvs[a][1]];
            let d2 = [uvs[c][0] - uvs[a][0], uvs[c][1] - uvs[a][1]];
            let det = d1[0] * d2[1] - d2[0] * d1[1];
            let du = (e1 * d2[1] - e2 * d1[1]) / det;
            let dv = (e2 * d1[0] - e1 * d2[0]) / det;
            du.cross(dv).dot(mesh.normals[a]) < 0.0
        })
    }

    #[test]
    fn paintable_lid_uvs_are_not_mirrored() {
        let loaded = load_glb(&ornatus_fixtures::laptop_glb()).unwrap();
        for name in ["Lid_Outer", "Lid_Screen"] {
            let mesh = loaded.model.meshes.iter().find(|m| m.name == name).unwrap();
            assert!(uv_is_unmirrored(mesh), "{name} has mirrored UVs");
        }
    }

    fn one_mesh(indices: Vec<u32>, uvs: Option<Vec<[f32; 2]>>) -> Vec<u8> {
        use ornatus_fixtures::{MaterialSpec, MeshSpec, NodeSpec, SceneSpec};
        SceneSpec {
            materials: vec![MaterialSpec {
                name: "M".into(),
                base_color: [1.0; 4],
                metallic: 0.0,
                roughness: 1.0,
            }],
            meshes: vec![MeshSpec {
                name: "Broken".into(),
                material: 0,
                positions: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                normals: vec![[0.0, 0.0, 1.0]; 3],
                uvs,
                indices,
            }],
            nodes: vec![NodeSpec {
                name: "Broken".into(),
                mesh: Some(0),
                translation: [0.0; 3],
                rotation: [0.0, 0.0, 0.0, 1.0],
                children: vec![],
            }],
            roots: vec![0],
        }
        .to_glb()
    }

    #[test]
    fn index_past_last_vertex_is_rejected() {
        let result = load_glb(&one_mesh(vec![0, 1, 99], None));
        assert!(matches!(result, Err(LoadError::Invalid(_))), "{result:?}");
    }

    #[test]
    fn uv_count_mismatch_is_rejected() {
        let result = load_glb(&one_mesh(vec![0, 1, 2], Some(vec![[0.0, 0.0]])));
        assert!(matches!(result, Err(LoadError::Invalid(_))), "{result:?}");
    }

    #[test]
    fn rejects_garbage() {
        assert!(matches!(load_glb(b"not a glb"), Err(LoadError::Parse(_))));
        assert!(matches!(load_glb(&[]), Err(LoadError::Parse(_))));
    }

    #[test]
    fn smooth_normals_point_up_for_flat_triangle() {
        let positions = [Vec3::ZERO, Vec3::X, Vec3::Y];
        let normals = smooth_normals(&positions, &[0, 1, 2]);
        assert!(normals.iter().all(|n| (*n - Vec3::Z).length() < 1e-6));
    }

    #[test]
    fn center_and_fit_centers_and_scales() {
        let mut model = load_glb(&ornatus_fixtures::laptop_glb()).unwrap().model;
        let scale = model.center_and_fit(2.0).expect("scale");
        assert!(scale > 0.0);
        let (min, max) = model.bounds().unwrap();
        let center = (min + max) / 2.0;
        assert!(center.length() < 1e-5, "center was {center}");
        assert!(((max - min).max_element() - 2.0).abs() < 1e-4);
    }
}
