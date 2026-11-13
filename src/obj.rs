//! Wavefront OBJ loading.
//!
//! Corner records in all four forms — `v`, `v/vt`, `v//vn`, `v/vt/vn` — are
//! accepted, and a normal is bound to the **position** it is referenced from,
//! so files whose `vn` indices differ from their `v` indices (common for
//! flat-shaded or per-corner-normal exports) load correctly. `b` in `a/b/c` is
//! a texture coordinate and is never read as a normal. Vertices that end up
//! with no bound normal get area-weighted geometric normals, and everything is
//! re-normalized because exported normals are not always unit length.

use thiserror::Error;

/// A triangle soup: positions, unit normals and 0-based triangle indices.
#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    /// One `[x, y, z]` per vertex.
    pub positions: Vec<[f64; 3]>,
    /// One unit `[x, y, z]` per vertex, parallel to `positions`.
    pub normals: Vec<[f64; 3]>,
    /// One `[i, j, k]` per triangle, indices into `positions`/`normals`.
    pub indices: Vec<[u32; 3]>,
}

impl Mesh {
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    pub fn face_count(&self) -> usize {
        self.indices.len()
    }

    /// Mean of all vertex positions — the default camera focus.
    pub fn centroid(&self) -> [f64; 3] {
        let mut s = [0.0f64; 3];
        for p in &self.positions {
            for (acc, v) in s.iter_mut().zip(p) {
                *acc += v;
            }
        }
        let n = self.positions.len().max(1) as f64;
        [s[0] / n, s[1] / n, s[2] / n]
    }
}

#[derive(Debug, Error)]
pub enum ObjError {
    #[error("{path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("{0}: no vertices")]
    NoVertices(String),
    #[error("{path}: line {line}: cannot read `{got}` as a number")]
    BadNumber {
        path: String,
        line: usize,
        got: String,
    },
    #[error(
        "{path}: line {line}: negative face index {value}; relative (negative) indices are not supported"
    )]
    NegativeFaceIndex {
        path: String,
        line: usize,
        value: i64,
    },
    #[error("{path}: line {line}: face index {value} is out of range (file has {count} vertices)")]
    FaceIndexOutOfRange {
        path: String,
        line: usize,
        value: i64,
        count: usize,
    },
    #[error("{0}: more vertices than this build can address (u32 index space exhausted)")]
    TooManyVertices(String),
}

/// Load a mesh from an OBJ file. See the [module docs](self) for the exact
/// corner-form and normal-binding semantics.
pub fn load_obj(path: &str) -> Result<Mesh, ObjError> {
    let bytes = std::fs::read(path).map_err(|source| ObjError::Io {
        path: path.to_string(),
        source,
    })?;
    let text = String::from_utf8_lossy(&bytes);
    load_obj_from_str(path, &text)
}

/// The parser body, split out so tests can feed fixtures without a filesystem.
pub fn load_obj_from_str(path: &str, text: &str) -> Result<Mesh, ObjError> {
    let mut verts: Vec<[f64; 3]> = Vec::new();
    let mut vns: Vec<[f64; 3]> = Vec::new();
    let mut faces: Vec<[i64; 3]> = Vec::new();
    // Parallel to `faces`: the 0-based `vn` index per corner, -1 when absent.
    let mut normal_idx: Vec<[i64; 3]> = Vec::new();
    // Parallel to `faces`: the source line, for index errors.
    let mut face_lines: Vec<usize> = Vec::new();

    for (idx, line) in text.lines().enumerate() {
        let line_no = idx + 1;
        if let Some(rest) = line.strip_prefix("v ") {
            let fields = rest.split_whitespace();
            verts.push(read_triple(path, line_no, fields)?);
        } else if let Some(rest) = line.strip_prefix("vn ") {
            let fields = rest.split_whitespace();
            vns.push(read_triple(path, line_no, fields)?);
        } else if let Some(rest) = line.strip_prefix("f ") {
            read_face(
                path,
                line_no,
                rest,
                &mut faces,
                &mut normal_idx,
                &mut face_lines,
            )?;
        }
        // Every other line (`vt`, `usemtl`, comments, blanks…) is ignored.
    }

    if verts.is_empty() {
        return Err(ObjError::NoVertices(path.to_string()));
    }

    // Validate corner indices against the final vertex count. Negative
    // (relative) indices are rejected in v1 with a clear error; the Python
    // original silently wrapped them as Python would.
    let n_verts = verts.len();
    if n_verts > u32::MAX as usize {
        return Err(ObjError::TooManyVertices(path.to_string()));
    }
    for (tri, line) in faces.iter().zip(&face_lines) {
        for &v in tri {
            if v < 0 {
                return Err(ObjError::NegativeFaceIndex {
                    path: path.to_string(),
                    line: *line,
                    value: v + 1,
                });
            }
            if (v as usize) >= n_verts {
                return Err(ObjError::FaceIndexOutOfRange {
                    path: path.to_string(),
                    line: *line,
                    value: v + 1,
                    count: n_verts,
                });
            }
        }
    }
    let indices: Vec<[u32; 3]> = faces
        .iter()
        .map(|t| [t[0] as u32, t[1] as u32, t[2] as u32])
        .collect();

    // Bind `vn` entries to the positions that reference them; first reference
    // wins, out-of-range `vn` indices leave the corner unbound.
    let mut normals = vec![[0.0f64; 3]; n_verts];
    let mut bound = vec![false; n_verts];
    if !vns.is_empty() {
        for (tri, nis) in faces.iter().zip(&normal_idx) {
            for (&vi, &ni) in tri.iter().zip(nis) {
                if ni >= 0 && (ni as usize) < vns.len() {
                    let vi = vi as usize;
                    if !bound[vi] {
                        normals[vi] = vns[ni as usize];
                        bound[vi] = true;
                    }
                }
            }
        }
    }

    // Unbound vertices get area-weighted geometric normals.
    if !bound.iter().all(|b| *b) {
        let geo = geometric_normals(&verts, &indices);
        for (slot, (g, b)) in normals.iter_mut().zip(geo.iter().zip(&bound)) {
            if !*b {
                *slot = *g;
            }
        }
    }

    // Re-normalize everything: exported `vn` values are not always unit.
    for n in &mut normals {
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        let d = len.max(1e-12);
        for c in n.iter_mut() {
            *c /= d;
        }
    }

    Ok(Mesh {
        positions: verts,
        normals,
        indices,
    })
}

/// Read the first three whitespace-separated fields as coordinates; extra
/// trailing floats (per-vertex colors) are accepted and ignored.
fn read_triple<'a>(
    path: &str,
    line_no: usize,
    mut fields: impl Iterator<Item = &'a str>,
) -> Result<[f64; 3], ObjError> {
    let mut out = [0.0f64; 3];
    for slot in out.iter_mut() {
        let tok = match fields.next() {
            Some(t) => t,
            None => {
                return Err(ObjError::BadNumber {
                    path: path.to_string(),
                    line: line_no,
                    got: "<missing field>".to_string(),
                });
            }
        };
        *slot = tok.parse::<f64>().map_err(|_| ObjError::BadNumber {
            path: path.to_string(),
            line: line_no,
            got: tok.to_string(),
        })?;
    }
    Ok(out)
}

/// Parse one `f` line: fan-triangulate corners, keep the `vn` index of each
/// corner (`-1` when absent) parallel to the produced triangles.
fn read_face(
    path: &str,
    line_no: usize,
    rest: &str,
    faces: &mut Vec<[i64; 3]>,
    normal_idx: &mut Vec<[i64; 3]>,
    face_lines: &mut Vec<usize>,
) -> Result<(), ObjError> {
    let mut pos: Vec<i64> = Vec::new();
    let mut nrm: Vec<i64> = Vec::new();
    for tok in rest.split_whitespace() {
        let parts = tok.split('/').collect::<Vec<_>>();
        let head = parts.first().copied().unwrap_or("");
        pos.push(
            head.parse::<i64>().map_err(|_| ObjError::BadNumber {
                path: path.to_string(),
                line: line_no,
                got: tok.to_string(),
            })? - 1,
        );
        // `v//vn` and `v/vt/vn` carry a normal in parts[2]; `v` and
        // `v/vt` carry none — parts[1] is a texture index, never one.
        let mut n = -1;
        if parts.len() >= 3 && !parts[2].is_empty() {
            n = parts[2].parse::<i64>().map_err(|_| ObjError::BadNumber {
                path: path.to_string(),
                line: line_no,
                got: tok.to_string(),
            })? - 1;
        }
        nrm.push(n);
    }
    for k in 1..pos.len().saturating_sub(1) {
        faces.push([pos[0], pos[k], pos[k + 1]]);
        normal_idx.push([nrm[0], nrm[k], nrm[k + 1]]);
        face_lines.push(line_no);
    }
    Ok(())
}

/// Area-weighted vertex normals: each face adds its unnormalized
/// `(b − a) × (c − a)` to all three corners, so the sum over incident faces is
/// proportional to the mean of the adjacent face areas. Accumulated in face
/// order for a deterministic float sum.
pub(crate) fn geometric_normals(positions: &[[f64; 3]], indices: &[[u32; 3]]) -> Vec<[f64; 3]> {
    let mut acc = vec![[0.0f64; 3]; positions.len()];
    for tri in indices {
        let a = positions[tri[0] as usize];
        let b = positions[tri[1] as usize];
        let c = positions[tri[2] as usize];
        let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let cross = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        for &vi in tri {
            let vi = vi as usize;
            for (slot, v) in acc[vi].iter_mut().zip(&cross) {
                *slot += v;
            }
        }
    }
    acc
}
