//! The default laptop and small fixtures. All units are meters.

use crate::glb::{MaterialSpec, MeshSpec, NodeSpec, SceneSpec};

const IDENTITY: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// Axis-aligned box with one 0..1 UV square per face.
fn box_mesh(name: &str, material: usize, center: [f32; 3], size: [f32; 3]) -> MeshSpec {
    let h = [size[0] / 2.0, size[1] / 2.0, size[2] / 2.0];
    // (normal, u axis, v axis) per face, so each face is a right-handed quad.
    let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]),
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, -1.0, 0.0]),
        ([0.0, 0.0, -1.0], [-1.0, 0.0, 0.0], [0.0, -1.0, 0.0]),
        ([1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, -1.0, 0.0]),
        ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]),
    ];
    let mut mesh = MeshSpec {
        name: name.to_owned(),
        material,
        positions: Vec::new(),
        normals: Vec::new(),
        uvs: Some(Vec::new()),
        indices: Vec::new(),
    };
    for (normal, u, v) in faces {
        let base = u32::try_from(mesh.positions.len()).expect("small mesh");
        for (su, sv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let mut p = center;
            for axis in 0..3 {
                p[axis] += normal[axis] * h[axis] + u[axis] * su * h[axis] + v[axis] * sv * h[axis];
            }
            mesh.positions.push(p);
            mesh.normals.push(normal);
            mesh.uvs
                .as_mut()
                .expect("uvs")
                .push([(su + 1.0) / 2.0, (sv + 1.0) / 2.0]);
        }
        // The (u, v) square order is counter-clockwise around u x v, which is
        // opposite to the outward normal, so flip it.
        mesh.indices
            .extend([base, base + 2, base + 1, base, base + 3, base + 2]);
    }
    mesh
}

/// A flat quad on the XZ plane at height `y`, facing up (`up`) or down.
/// UVs run 0..1 across the quad, unmirrored when seen from the facing side.
fn flat_quad(name: &str, material: usize, y: f32, width: f32, depth: f32, up: bool) -> MeshSpec {
    let (hx, hz) = (width / 2.0, depth / 2.0);
    let normal = if up {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, -1.0, 0.0]
    };
    // Corners listed counter-clockwise when seen from the facing side.
    let (positions, uvs) = if up {
        (
            vec![[-hx, y, -hz], [-hx, y, hz], [hx, y, hz], [hx, y, -hz]],
            vec![[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
        )
    } else {
        (
            vec![[-hx, y, -hz], [hx, y, -hz], [hx, y, hz], [-hx, y, hz]],
            vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
        )
    };
    MeshSpec {
        name: name.to_owned(),
        material,
        positions,
        normals: vec![normal; 4],
        uvs: Some(uvs),
        indices: vec![0, 1, 2, 0, 2, 3],
    }
}

fn material(name: &str, color: [f32; 4], metallic: f32, roughness: f32) -> MaterialSpec {
    MaterialSpec {
        name: name.to_owned(),
        base_color: color,
        metallic,
        roughness,
    }
}

/// The default laptop: base, hinge, and a lid (body, screen, outer panel)
/// opened about 100 degrees. `Lid_Outer` is the paintable surface, a flat quad
/// with clean 0..1 UVs.
pub fn laptop_glb() -> Vec<u8> {
    let (width, depth) = (0.30, 0.21);
    let lid_thickness = 0.008;
    let meshes = vec![
        box_mesh("Base", 0, [0.0, 0.0075, 0.0], [width, 0.015, depth]),
        box_mesh("Hinge", 2, [0.0, 0.0, 0.0], [0.2, 0.012, 0.012]),
        box_mesh(
            "Lid_Body",
            0,
            [0.0, lid_thickness / 2.0, depth / 2.0],
            [width, lid_thickness, depth],
        ),
        flat_quad("Lid_Screen", 1, -0.0005, 0.27, 0.18, false),
        flat_quad("Lid_Outer", 3, lid_thickness + 0.0005, width, depth, true),
    ];
    // Screen and outer quads sit in the lid's local space, shifted along z.
    let mut meshes = meshes;
    for mesh in meshes.iter_mut().skip(3) {
        for p in &mut mesh.positions {
            p[2] += depth / 2.0;
        }
    }
    let angle = 100.0_f32.to_radians();
    let half = -angle / 2.0;
    let nodes = vec![
        NodeSpec {
            name: "Laptop".into(),
            mesh: None,
            translation: [0.0; 3],
            rotation: IDENTITY,
            children: vec![1, 2, 3],
        },
        NodeSpec {
            name: "Base".into(),
            mesh: Some(0),
            translation: [0.0; 3],
            rotation: IDENTITY,
            children: vec![],
        },
        NodeSpec {
            name: "Hinge".into(),
            mesh: Some(1),
            translation: [0.0, 0.015, -depth / 2.0],
            rotation: IDENTITY,
            children: vec![],
        },
        NodeSpec {
            name: "Lid".into(),
            mesh: None,
            translation: [0.0, 0.015, -depth / 2.0],
            rotation: [half.sin(), 0.0, 0.0, half.cos()],
            children: vec![4, 5, 6],
        },
        NodeSpec {
            name: "Lid_Body".into(),
            mesh: Some(2),
            translation: [0.0; 3],
            rotation: IDENTITY,
            children: vec![],
        },
        NodeSpec {
            name: "Lid_Screen".into(),
            mesh: Some(3),
            translation: [0.0; 3],
            rotation: IDENTITY,
            children: vec![],
        },
        NodeSpec {
            name: "Lid_Outer".into(),
            mesh: Some(4),
            translation: [0.0; 3],
            rotation: IDENTITY,
            children: vec![],
        },
    ];
    SceneSpec {
        materials: vec![
            material("Chassis", [0.62, 0.64, 0.67, 1.0], 0.9, 0.4),
            material("Screen", [0.02, 0.02, 0.03, 1.0], 0.0, 0.2),
            material("Hinge", [0.15, 0.15, 0.16, 1.0], 0.8, 0.5),
            material("LidOuter", [0.62, 0.64, 0.67, 1.0], 0.9, 0.4),
        ],
        meshes,
        nodes,
        roots: vec![0],
    }
    .to_glb()
}

/// A single triangle without UVs, for loader error-path tests.
pub fn triangle_glb(with_uvs: bool) -> Vec<u8> {
    SceneSpec {
        materials: vec![material("Plain", [1.0, 1.0, 1.0, 1.0], 0.0, 1.0)],
        meshes: vec![MeshSpec {
            name: "Triangle".into(),
            material: 0,
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            uvs: with_uvs.then(|| vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]),
            indices: vec![0, 1, 2],
        }],
        nodes: vec![NodeSpec {
            name: "Triangle".into(),
            mesh: Some(0),
            translation: [0.0; 3],
            rotation: IDENTITY,
            children: vec![],
        }],
        roots: vec![0],
    }
    .to_glb()
}

/// A flat grid of `side` x `side` vertices, for load and memory stress tests.
/// Each vertex costs about 56 bytes in the file (32 for attributes, 24 for indices).
pub fn grid_glb(side: u32) -> Vec<u8> {
    let step = 1.0 / (side.saturating_sub(1).max(1)) as f32;
    let count = (side * side) as usize;
    let mut positions = Vec::with_capacity(count);
    let mut uvs = Vec::with_capacity(count);
    for i in 0..side {
        for j in 0..side {
            let (x, z) = (i as f32 * step, j as f32 * step);
            positions.push([x, 0.0, z]);
            uvs.push([x, z]);
        }
    }
    let mut indices = Vec::with_capacity((side as usize).saturating_sub(1).pow(2) * 6);
    for i in 0..side.saturating_sub(1) {
        for j in 0..side.saturating_sub(1) {
            let a = i * side + j;
            let (b, c, d) = (a + 1, a + side + 1, a + side);
            indices.extend([a, b, c, a, c, d]);
        }
    }
    SceneSpec {
        materials: vec![material("Grid", [0.5, 0.5, 0.5, 1.0], 0.0, 0.8)],
        meshes: vec![MeshSpec {
            name: "Grid".into(),
            material: 0,
            positions,
            normals: vec![[0.0, 1.0, 0.0]; count],
            uvs: Some(uvs),
            indices,
        }],
        nodes: vec![NodeSpec {
            name: "Grid".into(),
            mesh: Some(0),
            translation: [0.0; 3],
            rotation: IDENTITY,
            children: vec![],
        }],
        roots: vec![0],
    }
    .to_glb()
}

/// Four flat quads side by side, each with a different UV problem, for
/// testing the UV audit by eye: clean, mirrored, overlapping, and no UVs.
pub fn flawed_glb() -> Vec<u8> {
    let corners = [
        [-0.4, 0.0, -0.4],
        [-0.4, 0.0, 0.4],
        [0.4, 0.0, 0.4],
        [0.4, 0.0, -0.4],
    ];
    let good = [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]];
    let quad = |name: &str, x: f32, uvs: Option<Vec<[f32; 2]>>| {
        let positions: Vec<[f32; 3]> = corners.iter().map(|p| [p[0] + x, p[1], p[2]]).collect();
        MeshSpec {
            name: name.to_owned(),
            material: 0,
            normals: vec![[0.0, 1.0, 0.0]; positions.len()],
            positions,
            uvs,
            indices: vec![0, 1, 2, 0, 2, 3],
        }
    };
    // Two separate triangles that both map onto the same UV triangle.
    let overlapping = {
        let mut mesh = quad("Overlapping_Quad", 1.0, None);
        let [a, b, c, d] = [corners[0], corners[1], corners[2], corners[3]];
        mesh.positions = [a, b, c, a, c, d]
            .iter()
            .map(|p| [p[0] + 1.0, p[1], p[2]])
            .collect();
        mesh.normals = vec![[0.0, 1.0, 0.0]; 6];
        mesh.uvs = Some(vec![
            [0.0, 0.0],
            [0.0, 1.0],
            [1.0, 1.0],
            [0.0, 0.0],
            [0.0, 1.0],
            [1.0, 1.0],
        ]);
        mesh.indices = vec![0, 1, 2, 3, 4, 5];
        mesh
    };
    let meshes = vec![
        quad("Clean_Quad", -1.0, Some(good.to_vec())),
        quad(
            "Mirrored_Quad",
            0.0,
            Some(vec![[1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.0, 0.0]]),
        ),
        overlapping,
        quad("NoUV_Quad", 2.0, None),
    ];
    let nodes = (0..meshes.len())
        .map(|i| NodeSpec {
            name: meshes[i].name.clone(),
            mesh: Some(i),
            translation: [0.0; 3],
            rotation: IDENTITY,
            children: vec![],
        })
        .collect();
    SceneSpec {
        materials: vec![
            material("Plain", [0.7, 0.7, 0.75, 1.0], 0.2, 0.5),
            material("Plain2", [0.7, 0.7, 0.75, 1.0], 0.2, 0.5),
        ],
        roots: (0..meshes.len()).collect(),
        meshes,
        nodes,
    }
    .to_glb()
}
