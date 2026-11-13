//! Wavefront OBJ loading.
//!
//! The reading is delegated to [`objserde`]: its loss-preserving parser and
//! its normal-binding policy are exactly the semantics this crate requires —
//! a `vn` binds to the **position it is referenced from** (in `a/b/c`, `b` is
//! a texture coordinate, never a normal; flat-shaded exports index `vn`
//! differently from `v`), first referencing corner wins, unbound positions
//! fall back to area-weighted geometric normals accumulated in triangle
//! order, and everything is re-normalized by `max(‖n‖, 1e-12)` so a genuine
//! zero stays exactly zero. This module keeps meshview's own [`Mesh`]
//! representation and error surface stable for the rest of the crate and for
//! library users.

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
    let obj = objserde::parse_file(path).map_err(|e| map_err(path, e))?;
    Ok(to_mesh(obj.bind()))
}

/// Parse OBJ text under a display name used in error messages, without a
/// filesystem round trip. Split out so tests can feed fixtures.
pub fn load_obj_from_str(path: &str, text: &str) -> Result<Mesh, ObjError> {
    let obj = objserde::parse(text).map_err(|e| map_err(path, e))?;
    Ok(to_mesh(obj.bind()))
}

fn to_mesh(b: objserde::BoundMesh) -> Mesh {
    Mesh {
        positions: b.positions,
        normals: b.unit_normals,
        indices: b.triangles,
    }
}

fn map_err(path: &str, e: objserde::ObjError) -> ObjError {
    match e {
        objserde::ObjError::NoVertices => ObjError::NoVertices(path.to_string()),
        objserde::ObjError::BadFloat { line, got } => ObjError::BadNumber {
            path: path.to_string(),
            line,
            got,
        },
        objserde::ObjError::NegativeIndex { line, value } => ObjError::NegativeFaceIndex {
            path: path.to_string(),
            line,
            value,
        },
        objserde::ObjError::IndexOutOfRange { line, value, count } => {
            ObjError::FaceIndexOutOfRange {
                path: path.to_string(),
                line,
                value,
                count,
            }
        }
        objserde::ObjError::Io { path: p, source } => ObjError::Io {
            path: p.display().to_string(),
            source,
        },
    }
}
