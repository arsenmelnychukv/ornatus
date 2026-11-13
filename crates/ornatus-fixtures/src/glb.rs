//! Minimal binary glTF (`.glb`) writer.

use serde_json::{Value, json};

const FLOAT: u32 = 5126;
const UNSIGNED_INT: u32 = 5125;
const ARRAY_BUFFER: u32 = 34962;
const ELEMENT_ARRAY_BUFFER: u32 = 34963;

/// A material with metallic-roughness values.
pub struct MaterialSpec {
    pub name: String,
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
}

/// One triangle-list primitive wrapped in its own mesh.
pub struct MeshSpec {
    pub name: String,
    pub material: usize,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Option<Vec<[f32; 2]>>,
    pub indices: Vec<u32>,
}

/// A scene node with a translation, rotation (quaternion xyzw) and children.
pub struct NodeSpec {
    pub name: String,
    pub mesh: Option<usize>,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub children: Vec<usize>,
}

/// A whole scene, ready to serialize.
pub struct SceneSpec {
    pub materials: Vec<MaterialSpec>,
    pub meshes: Vec<MeshSpec>,
    pub nodes: Vec<NodeSpec>,
    pub roots: Vec<usize>,
}

struct Packer {
    bin: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
}

impl Packer {
    fn view(&mut self, bytes: &[u8], target: u32) -> usize {
        while !self.bin.len().is_multiple_of(4) {
            self.bin.push(0);
        }
        let offset = self.bin.len();
        self.bin.extend_from_slice(bytes);
        self.views.push(json!({
            "buffer": 0, "byteOffset": offset, "byteLength": bytes.len(), "target": target
        }));
        self.views.len() - 1
    }

    fn floats<const N: usize>(&mut self, data: &[[f32; N]], kind: &str, bounds: bool) -> usize {
        let bytes: Vec<u8> = data
            .iter()
            .flatten()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        let view = self.view(&bytes, ARRAY_BUFFER);
        let mut accessor = json!({
            "bufferView": view, "componentType": FLOAT, "count": data.len(), "type": kind
        });
        if bounds {
            let mut min = [f32::MAX; N];
            let mut max = [f32::MIN; N];
            for item in data {
                for i in 0..N {
                    min[i] = min[i].min(item[i]);
                    max[i] = max[i].max(item[i]);
                }
            }
            accessor["min"] = json!(min.to_vec());
            accessor["max"] = json!(max.to_vec());
        }
        self.accessors.push(accessor);
        self.accessors.len() - 1
    }

    fn indices(&mut self, data: &[u32]) -> usize {
        let bytes: Vec<u8> = data.iter().flat_map(|i| i.to_le_bytes()).collect();
        let view = self.view(&bytes, ELEMENT_ARRAY_BUFFER);
        self.accessors.push(json!({
            "bufferView": view, "componentType": UNSIGNED_INT, "count": data.len(), "type": "SCALAR"
        }));
        self.accessors.len() - 1
    }
}

impl SceneSpec {
    /// Serialize the scene as a `.glb` file.
    pub fn to_glb(&self) -> Vec<u8> {
        let mut packer = Packer {
            bin: Vec::new(),
            views: Vec::new(),
            accessors: Vec::new(),
        };
        let mut meshes = Vec::new();
        for mesh in &self.meshes {
            let position = packer.floats(&mesh.positions, "VEC3", true);
            let normal = packer.floats(&mesh.normals, "VEC3", false);
            let mut attributes = json!({ "POSITION": position, "NORMAL": normal });
            if let Some(uvs) = &mesh.uvs {
                attributes["TEXCOORD_0"] = json!(packer.floats(uvs, "VEC2", false));
            }
            let indices = packer.indices(&mesh.indices);
            meshes.push(json!({
                "name": mesh.name,
                "primitives": [{
                    "attributes": attributes, "indices": indices,
                    "material": mesh.material, "mode": 4
                }]
            }));
        }
        let materials: Vec<Value> = self
            .materials
            .iter()
            .map(|m| {
                json!({
                    "name": m.name,
                    "pbrMetallicRoughness": {
                        "baseColorFactor": m.base_color,
                        "metallicFactor": m.metallic,
                        "roughnessFactor": m.roughness
                    }
                })
            })
            .collect();
        let nodes: Vec<Value> = self
            .nodes
            .iter()
            .map(|n| {
                let mut node = json!({
                    "name": n.name, "translation": n.translation, "rotation": n.rotation
                });
                if let Some(mesh) = n.mesh {
                    node["mesh"] = json!(mesh);
                }
                if !n.children.is_empty() {
                    node["children"] = json!(n.children);
                }
                node
            })
            .collect();
        while !packer.bin.len().is_multiple_of(4) {
            packer.bin.push(0);
        }
        let document = json!({
            "asset": { "version": "2.0", "generator": "ornatus-fixtures" },
            "scene": 0,
            "scenes": [{ "nodes": self.roots }],
            "nodes": nodes,
            "meshes": meshes,
            "materials": materials,
            "accessors": packer.accessors,
            "bufferViews": packer.views,
            "buffers": [{ "byteLength": packer.bin.len() }]
        });
        let mut json_bytes = document.to_string().into_bytes();
        while !json_bytes.len().is_multiple_of(4) {
            json_bytes.push(b' ');
        }
        let total = 12 + 8 + json_bytes.len() + 8 + packer.bin.len();
        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(
            &u32::try_from(total)
                .expect("fixture fits in u32")
                .to_le_bytes(),
        );
        out.extend_from_slice(&u32::try_from(json_bytes.len()).expect("fits").to_le_bytes());
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(&json_bytes);
        out.extend_from_slice(&u32::try_from(packer.bin.len()).expect("fits").to_le_bytes());
        out.extend_from_slice(b"BIN\0");
        out.extend_from_slice(&packer.bin);
        out
    }
}
