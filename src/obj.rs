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

// ---------------------------------------------------------------------------
// Textured mode: opt-in uv + material plumbing for render_textured.
// ---------------------------------------------------------------------------

use crate::texture::Texture;
use std::collections::HashMap;

/// Per-triangle texturing data, parallel to [`Mesh::indices`]. Corner order
/// matches the triangle's indices exactly (the same fan walk `bind` uses).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TriTex {
    /// Interpolated per-corner texture coordinates. Zero when the corners
    /// carried no usable `vt`.
    pub uv: [[f64; 2]; 3],
    /// Slot into [`TexturedMesh::textures`]; `None` draws this triangle with
    /// the flat diffuse tint only.
    pub texture: Option<usize>,
    /// Material diffuse (Kd) in 0..=1 per channel; white when unset.
    /// Multiplies the sampled texel (or stands in for one).
    pub kd: [f64; 3],
    /// False when no material was in effect at this face — the surface then
    /// renders exactly like the grayscale path, so turning texturing on for
    /// an untextured mesh changes nothing but the container.
    pub shaded_by_material: bool,
}

/// A [`Mesh`] plus the per-triangle texture bindings [`load_textured`] built
/// beside it. Geometry, normals and coverage are untouched: texturing colors
/// pixels, never visibility.
#[derive(Clone, Debug, PartialEq)]
pub struct TexturedMesh {
    pub mesh: Mesh,
    pub tri_tex: Vec<TriTex>,
    pub textures: Vec<Texture>,
}

impl TexturedMesh {
    pub fn vertex_count(&self) -> usize {
        self.mesh.vertex_count()
    }

    pub fn face_count(&self) -> usize {
        self.mesh.face_count()
    }

    pub fn centroid(&self) -> [f64; 3] {
        self.mesh.centroid()
    }
}

/// Load a mesh with its texture coordinates and resolved material textures.
/// `mtllib` files are read by objserde relative to `path`; `map_Kd` file
/// paths (including Windows `\` spellings) are resolved against the same
/// directory and decoded once, deduplicated. A texture that fails to decode
/// leaves its triangles on the flat Kd tint — never a hard error, mirroring
/// how a missing `.mtl` does not fail the load.
pub fn load_textured(path: &str) -> Result<TexturedMesh, ObjError> {
    let obj = objserde::parse_file(path).map_err(|e| map_err(path, e))?;
    let base = std::path::Path::new(path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_default();
    let mesh = to_mesh(obj.bind());

    let mut tri_tex: Vec<TriTex> = Vec::with_capacity(mesh.indices.len());
    let mut textures: Vec<Texture> = Vec::new();
    let mut slots: HashMap<String, Option<usize>> = HashMap::new();

    for face in &obj.faces {
        let material = obj.material_of(face);
        let kd = material.and_then(|m| m.diffuse).unwrap_or([1.0, 1.0, 1.0]);
        let shaded = material.is_some();
        let map = material
            .and_then(|m| m.diffuse_map())
            .map(|m| normalize_map_path(&m.path));
        for tri in face.triangulate() {
            let mut uv = [[0.0; 2]; 3];
            let mut have_uv = true;
            for (slot, corner) in uv.iter_mut().zip(tri.corners) {
                match corner
                    .vt
                    .and_then(|i| obj.tex_coords.get(i as usize))
                    .filter(|tc| tc.arity >= 2)
                {
                    Some(tc) => *slot = [tc.components[0], tc.components[1]],
                    None => have_uv = false,
                }
            }
            let texture = if have_uv {
                map.as_deref().and_then(|name| match slots.get(name) {
                    Some(known) => *known,
                    None => {
                        let full = base.join(name);
                        let full_str = full.to_string_lossy().into_owned();
                        match Texture::load(&full_str) {
                            Ok(t) => {
                                let idx = textures.len();
                                textures.push(t);
                                slots.insert(name.to_string(), Some(idx));
                                Some(idx)
                            }
                            Err(_) => {
                                slots.insert(name.to_string(), None);
                                None
                            }
                        }
                    }
                })
            } else {
                None
            };
            tri_tex.push(TriTex {
                uv,
                texture: texture.filter(|_| have_uv),
                kd,
                shaded_by_material: shaded,
            });
        }
    }
    debug_assert_eq!(tri_tex.len(), mesh.indices.len());
    Ok(TexturedMesh {
        mesh,
        tri_tex,
        textures,
    })
}

/// `.\tex.jpg` and `./tex.jpg` both name `tex.jpg` beside the OBJ; deeper
/// relative paths keep their (forward-slashed) shape.
fn normalize_map_path(raw: &str) -> String {
    let unified = raw.replace('\\', "/");
    let mut s = unified.as_str();
    loop {
        let next = s.strip_prefix("./").unwrap_or(s);
        if next == s {
            break;
        }
        s = next;
    }
    s.to_string()
}
