//! UV audit for a surface chosen as the paint target (decision 0003).
//!
//! Missing UVs block painting. Mirrored, overlapping, out-of-range or
//! degenerate UVs, and low UV coverage, produce warnings, and the affected
//! triangles are returned so the viewer can highlight them.

use std::fmt;

use glam::{Vec2, Vec3};

use crate::model::MeshData;

/// Side length of the grid used to detect overlaps and measure coverage.
const GRID: usize = 1024;
/// Coverage below this fraction of the 0..1 UV square gets a warning.
const LOW_COVERAGE: f32 = 0.10;
/// Most grid cells the overlap check will visit, so odd UV layouts cannot hang the app.
const CELL_BUDGET: usize = 200_000_000;
/// UV area (in the 0..1 square) below which a triangle counts as degenerate.
const DEGENERATE_UV_AREA: f32 = 1e-12;

/// One thing the audit found.
#[derive(Debug, Clone, PartialEq)]
pub enum UvFinding {
    /// The surface has no UV coordinates. Painting is blocked.
    MissingUvs,
    /// Triangles whose texture would appear flipped.
    Mirrored { triangles: usize, total: usize },
    /// Triangles that share UV space with another triangle.
    Overlap { triangles: usize },
    /// Triangles with UVs outside the 0..1 square.
    OutOfRange { triangles: usize },
    /// Triangles that cover no UV area.
    Degenerate { triangles: usize },
    /// The surface uses only a small part of the UV square.
    LowCoverage { percent: f32 },
    /// The overlap check was skipped for some triangles because the layout is too heavy.
    Unchecked { triangles: usize },
}

impl UvFinding {
    /// True when painting cannot go ahead.
    pub fn is_blocking(&self) -> bool {
        matches!(self, Self::MissingUvs)
    }
}

impl fmt::Display for UvFinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingUvs => write!(
                f,
                "This surface has no UV coordinates, so it cannot be painted."
            ),
            Self::Mirrored { triangles, total } => write!(
                f,
                "{triangles} of {total} triangles have mirrored UVs. Paint and stickers would appear flipped there."
            ),
            Self::Overlap { triangles } => write!(
                f,
                "{triangles} triangles share UV space with others. Strokes would appear in more than one place."
            ),
            Self::OutOfRange { triangles } => write!(
                f,
                "{triangles} triangles have UVs outside the 0 to 1 square. Paint would be clipped there."
            ),
            Self::Degenerate { triangles } => write!(
                f,
                "{triangles} triangles have no UV area. Paint cannot show on them."
            ),
            Self::Unchecked { triangles } => write!(
                f,
                "The overlap check was skipped for {triangles} triangles because the UV layout is very heavy."
            ),
            Self::LowCoverage { percent } => write!(
                f,
                "The surface uses only {percent:.0}% of the UV square, so paint will look low resolution."
            ),
        }
    }
}

/// The result of auditing one surface.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UvAudit {
    /// Everything that was found, blocking findings first.
    pub findings: Vec<UvFinding>,
    /// Indices (in triangle order) of triangles with a problem worth highlighting.
    pub flagged_triangles: Vec<u32>,
    /// Fraction of the 0..1 UV square the surface covers, 0 to 1.
    pub coverage: f32,
}

impl UvAudit {
    /// True when no finding blocks painting.
    pub fn is_paintable(&self) -> bool {
        !self.findings.iter().any(UvFinding::is_blocking)
    }
}

fn triangle_count(mesh: &MeshData) -> usize {
    mesh.indices.len() / 3
}

/// Seen from outside, an unmirrored texture has u running right and v running
/// down, so the u tangent crossed with the v tangent points into the surface.
/// Returns `None` when the triangle has no usable UV or surface area.
fn is_mirrored(positions: [Vec3; 3], normal: Vec3, uvs: [Vec2; 3]) -> Option<bool> {
    let (e1, e2) = (positions[1] - positions[0], positions[2] - positions[0]);
    let (d1, d2) = (uvs[1] - uvs[0], uvs[2] - uvs[0]);
    let det = d1.x * d2.y - d2.x * d1.y;
    if det.abs() * 0.5 < DEGENERATE_UV_AREA || e1.cross(e2).length_squared() < 1e-20 {
        return None;
    }
    let du = (e1 * d2.y - e2 * d1.y) / det;
    let dv = (e2 * d1.x - e1 * d2.x) / det;
    Some(du.cross(dv).dot(normal) > 0.0)
}

/// Audit a surface's UVs.
pub fn audit(mesh: &MeshData) -> UvAudit {
    audit_with_budget(mesh, CELL_BUDGET)
}

fn audit_with_budget(mesh: &MeshData, mut budget: usize) -> UvAudit {
    let Some(uvs) = mesh.uvs.as_ref() else {
        return UvAudit {
            findings: vec![UvFinding::MissingUvs],
            ..UvAudit::default()
        };
    };
    let total = triangle_count(mesh);
    let mut mirrored = vec![false; total];
    let mut out_of_range = vec![false; total];
    let mut degenerate = vec![false; total];
    let mut overlapping = vec![false; total];
    let mut owner = vec![u32::MAX; GRID * GRID];
    let mut covered = 0usize;
    let mut unchecked = 0usize;

    for (t, tri) in mesh.indices.as_chunks::<3>().0.iter().enumerate() {
        let i = tri.map(|i| i as usize);
        let uv = i.map(|i| Vec2::from(uvs[i]));
        let pos = i.map(|i| mesh.positions[i]);
        let normal = i.iter().map(|&i| mesh.normals[i]).sum::<Vec3>();
        let normal = if normal.length_squared() > 1e-12 {
            normal
        } else {
            (pos[1] - pos[0]).cross(pos[2] - pos[0])
        };

        out_of_range[t] = uv
            .iter()
            .any(|p| p.x < -1e-4 || p.x > 1.0 + 1e-4 || p.y < -1e-4 || p.y > 1.0 + 1e-4);
        match is_mirrored(pos, normal, uv) {
            Some(flag) => mirrored[t] = flag,
            None => {
                // No surface area is a modeling quirk, not a UV problem.
                let det = (uv[1] - uv[0]).perp_dot(uv[2] - uv[0]);
                degenerate[t] = det.abs() * 0.5 < DEGENERATE_UV_AREA;
            }
        }
        if !rasterize(
            uv,
            t as u32,
            &mut owner,
            &mut overlapping,
            &mut covered,
            &mut budget,
        ) {
            unchecked += 1;
        }
    }

    let count = |flags: &[bool]| flags.iter().filter(|&&f| f).count();
    let coverage = covered as f32 / (GRID * GRID) as f32;
    let mut findings = Vec::new();
    let mirrored_count = count(&mirrored);
    if mirrored_count > 0 {
        findings.push(UvFinding::Mirrored {
            triangles: mirrored_count,
            total,
        });
    }
    let overlap_count = count(&overlapping);
    if overlap_count > 0 {
        findings.push(UvFinding::Overlap {
            triangles: overlap_count,
        });
    }
    let range_count = count(&out_of_range);
    if range_count > 0 {
        findings.push(UvFinding::OutOfRange {
            triangles: range_count,
        });
    }
    let degenerate_count = count(&degenerate);
    if degenerate_count > 0 {
        findings.push(UvFinding::Degenerate {
            triangles: degenerate_count,
        });
    }
    if unchecked > 0 {
        findings.push(UvFinding::Unchecked {
            triangles: unchecked,
        });
    }
    if total > 0 && unchecked == 0 && coverage < LOW_COVERAGE {
        findings.push(UvFinding::LowCoverage {
            percent: coverage * 100.0,
        });
    }
    let flagged_triangles = (0..total)
        .filter(|&t| mirrored[t] || overlapping[t] || out_of_range[t])
        .map(|t| t as u32)
        .collect();
    UvAudit {
        findings,
        flagged_triangles,
        coverage,
    }
}

/// Mark the grid cells whose centers lie strictly inside the triangle. A cell
/// already owned by another triangle flags both as overlapping. Cells exactly
/// on a shared edge belong to neither triangle, so neighbors never overlap.
/// Returns false when the work budget ran out and the triangle was skipped.
fn rasterize(
    uv: [Vec2; 3],
    id: u32,
    owner: &mut [u32],
    overlapping: &mut [bool],
    covered: &mut usize,
    budget: &mut usize,
) -> bool {
    let scale = GRID as f32;
    let p = uv.map(|p| p * scale);
    let area = (p[1] - p[0]).perp_dot(p[2] - p[0]);
    if area.abs() < 1e-9 {
        return true;
    }
    let min = p[0].min(p[1]).min(p[2]).max(Vec2::ZERO);
    let max = p[0].max(p[1]).max(p[2]).min(Vec2::splat(scale));
    if min.x >= max.x || min.y >= max.y {
        return true;
    }
    let (x0, x1) = (min.x.floor() as usize, (max.x.ceil() as usize).min(GRID));
    let (y0, y1) = (min.y.floor() as usize, (max.y.ceil() as usize).min(GRID));
    let cells = (x1 - x0) * (y1 - y0);
    if cells > *budget {
        return false;
    }
    *budget -= cells;
    let sign = area.signum();
    for y in y0..y1 {
        for x in x0..x1 {
            let c = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            let w0 = (p[2] - p[1]).perp_dot(c - p[1]) * sign;
            let w1 = (p[0] - p[2]).perp_dot(c - p[2]) * sign;
            let w2 = (p[1] - p[0]).perp_dot(c - p[0]) * sign;
            if w0 > 0.0 && w1 > 0.0 && w2 > 0.0 {
                let cell = &mut owner[y * GRID + x];
                if *cell == u32::MAX {
                    *cell = id;
                    *covered += 1;
                } else if *cell != id {
                    overlapping[*cell as usize] = true;
                    overlapping[id as usize] = true;
                }
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{MeshData, load_glb};

    fn quad(uvs: [[f32; 2]; 4]) -> MeshData {
        MeshData {
            name: "Quad".into(),
            material: None,
            base_color: [1.0; 4],
            metallic: 0.0,
            roughness: 1.0,
            // Counter-clockwise seen from +Y, so the outside is the +Y side.
            positions: vec![
                Vec3::new(-1.0, 0.0, -1.0),
                Vec3::new(-1.0, 0.0, 1.0),
                Vec3::new(1.0, 0.0, 1.0),
                Vec3::new(1.0, 0.0, -1.0),
            ],
            normals: vec![Vec3::Y; 4],
            uvs: Some(uvs.to_vec()),
            indices: vec![0, 1, 2, 0, 2, 3],
        }
    }

    const GOOD: [[f32; 2]; 4] = [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]];

    #[test]
    fn clean_quad_has_no_findings() {
        let audit = audit(&quad(GOOD));
        assert!(audit.findings.is_empty(), "{:?}", audit.findings);
        assert!(audit.flagged_triangles.is_empty());
        assert!(audit.is_paintable());
        assert!(audit.coverage > 0.99, "coverage {}", audit.coverage);
    }

    #[test]
    fn default_laptop_lid_is_clean() {
        let model = load_glb(&ornatus_fixtures::laptop_glb()).unwrap().model;
        let lid = model.meshes.iter().find(|m| m.name == "Lid_Outer").unwrap();
        let audit = audit(lid);
        assert!(audit.findings.is_empty(), "{:?}", audit.findings);
        assert!(audit.coverage > 0.99);
    }

    #[test]
    fn missing_uvs_block_painting() {
        let mut mesh = quad(GOOD);
        mesh.uvs = None;
        let audit = audit(&mesh);
        assert_eq!(audit.findings, [UvFinding::MissingUvs]);
        assert!(!audit.is_paintable());
    }

    #[test]
    fn flipped_u_is_mirrored_and_still_paintable() {
        let audit = audit(&quad([[1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.0, 0.0]]));
        assert_eq!(
            audit.findings,
            [UvFinding::Mirrored {
                triangles: 2,
                total: 2
            }]
        );
        assert_eq!(audit.flagged_triangles, [0, 1]);
        assert!(audit.is_paintable());
    }

    #[test]
    fn two_triangles_on_the_same_uv_area_overlap() {
        let mut mesh = quad(GOOD);
        // Both triangles use the same three vertices, so they cover the same UV area.
        mesh.indices = vec![0, 1, 2, 0, 1, 2];
        let audit = audit(&mesh);
        assert!(
            audit
                .findings
                .contains(&UvFinding::Overlap { triangles: 2 }),
            "{:?}",
            audit.findings
        );
        assert_eq!(audit.flagged_triangles, [0, 1]);
    }

    #[test]
    fn neighbouring_triangles_do_not_overlap() {
        let model = load_glb(&ornatus_fixtures::grid_glb(24)).unwrap().model;
        let audit = audit(&model.meshes[0]);
        assert!(
            !audit
                .findings
                .iter()
                .any(|f| matches!(f, UvFinding::Overlap { .. })),
            "{:?}",
            audit.findings
        );
    }

    #[test]
    fn uvs_outside_the_square_are_reported() {
        let audit = audit(&quad([[0.0, 0.0], [0.0, 2.0], [2.0, 2.0], [2.0, 0.0]]));
        assert!(
            audit
                .findings
                .contains(&UvFinding::OutOfRange { triangles: 2 })
        );
        assert!(audit.is_paintable());
    }

    #[test]
    fn collapsed_uvs_are_degenerate() {
        let audit = audit(&quad([[0.5, 0.5]; 4]));
        assert!(
            audit
                .findings
                .contains(&UvFinding::Degenerate { triangles: 2 })
        );
    }

    #[test]
    fn small_uv_islands_warn_about_coverage() {
        let audit = audit(&quad([[0.0, 0.0], [0.0, 0.1], [0.1, 0.1], [0.1, 0.0]]));
        assert!(
            audit
                .findings
                .iter()
                .any(|f| matches!(f, UvFinding::LowCoverage { .. })),
            "{:?}",
            audit.findings
        );
        assert!(audit.coverage < 0.02);
    }

    #[test]
    fn flawed_fixture_reports_each_problem() {
        let model = load_glb(&ornatus_fixtures::flawed_glb()).unwrap().model;
        let by_name = |name: &str| {
            let mesh = model.meshes.iter().find(|m| m.name == name).unwrap();
            audit(mesh)
        };
        assert!(by_name("Clean_Quad").findings.is_empty());
        assert!(matches!(
            by_name("Mirrored_Quad").findings[0],
            UvFinding::Mirrored { .. }
        ));
        assert!(
            by_name("Overlapping_Quad")
                .findings
                .iter()
                .any(|f| matches!(f, UvFinding::Overlap { .. }))
        );
        assert_eq!(by_name("NoUV_Quad").findings, [UvFinding::MissingUvs]);
    }

    #[test]
    fn heavy_layouts_are_cut_off_instead_of_hanging() {
        let mut mesh = quad(GOOD);
        // The same large triangle 50 times: each one costs about a million grid cells, so only 4 fit.
        mesh.indices = [0, 1, 2].repeat(50);
        let audit = audit_with_budget(&mesh, 5_000_000);
        assert!(
            audit
                .findings
                .contains(&UvFinding::Unchecked { triangles: 46 }),
            "{:?}",
            audit.findings
        );
        assert!(
            !audit
                .findings
                .iter()
                .any(|f| matches!(f, UvFinding::LowCoverage { .. }))
        );
        assert!(audit.is_paintable());
    }

    #[test]
    fn every_finding_has_a_message() {
        let findings = [
            UvFinding::MissingUvs,
            UvFinding::Mirrored {
                triangles: 1,
                total: 2,
            },
            UvFinding::Overlap { triangles: 1 },
            UvFinding::OutOfRange { triangles: 1 },
            UvFinding::Degenerate { triangles: 1 },
            UvFinding::LowCoverage { percent: 3.0 },
            UvFinding::Unchecked { triangles: 4 },
        ];
        assert!(findings.iter().all(|f| !f.to_string().is_empty()));
    }
}
