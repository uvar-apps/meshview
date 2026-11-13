//! Orthographic camera looking down −z, centred on a world-space focus.
//!
//! `focus` is the world point mapped onto the tile centre, which is what keeps
//! different meshes comparable: share one focus and zoom and any pixel
//! difference is geometry, not framing. A point `p` maps to view coordinates
//! via `M = Rz(roll) · Ry(yaw) · Rx(pitch)` as `p' = M·p`, then to screen
//!
//! ```text
//! x     = (p'.x − f.x)·zoom + size/2
//! y     = −(p'.y − f.y)·zoom + size/2
//! depth = p'.z              (larger depth = nearer the camera)
//! ```
//!
//! where `f = M·focus`. Shading uses `M·n`, i.e. the light direction is defined
//! relative to the camera, not the world.

use crate::obj::Mesh;

/// Row-major 3×3 matrix.
pub type Mat3 = [[f64; 3]; 3];

pub(crate) fn mul(a: &Mat3, b: &Mat3) -> Mat3 {
    let mut out = [[0.0f64; 3]; 3];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    out
}

pub(crate) fn mul_vec(m: &Mat3, v: &[f64; 3]) -> [f64; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

fn rot_x(a: f64) -> Mat3 {
    let (c, s) = (a.cos(), a.sin());
    [[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]]
}

fn rot_y(a: f64) -> Mat3 {
    let (c, s) = (a.cos(), a.sin());
    [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]]
}

fn rot_z(a: f64) -> Mat3 {
    let (c, s) = (a.cos(), a.sin());
    [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]]
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// World-space point placed at the tile centre.
    pub focus: [f64; 3],
    /// Pixels per world unit.
    pub zoom: f64,
    /// Radians about z.
    pub yaw: f64,
    /// Radians about x.
    pub pitch: f64,
    /// Radians about z after yaw (kept for API completeness).
    pub roll: f64,
}

impl Camera {
    pub fn new(focus: [f64; 3], zoom: f64) -> Self {
        Self {
            focus,
            zoom,
            yaw: 0.0,
            pitch: 0.0,
            roll: 0.0,
        }
    }

    pub fn matrix(&self) -> Mat3 {
        mul(&mul(&rot_z(self.roll), &rot_y(self.yaw)), &rot_x(self.pitch))
    }

    /// Put `point` through the camera: `(x, y, depth)` in a `size`×`size` tile.
    pub fn project_point(&self, size: usize, point: &[f64; 3]) -> (f64, f64, f64) {
        let m = self.matrix();
        let f = mul_vec(&m, &self.focus);
        let v = mul_vec(&m, point);
        (
            (v[0] - f[0]) * self.zoom + size as f64 / 2.0,
            -(v[1] - f[1]) * self.zoom + size as f64 / 2.0,
            v[2],
        )
    }
}

/// Per-vertex screen position, view depth and view-space normal for one
/// camera/tile precomputation.
pub(crate) struct Projected {
    pub sx: Vec<f64>,
    pub sy: Vec<f64>,
    pub depth: Vec<f64>,
    pub nview: Vec<[f64; 3]>,
}

pub(crate) fn project_mesh(cam: &Camera, mesh: &Mesh, size: usize) -> Projected {
    let m = cam.matrix();
    let f = mul_vec(&m, &cam.focus);
    let half = size as f64 / 2.0;
    let mut sx = Vec::with_capacity(mesh.positions.len());
    let mut sy = Vec::with_capacity(mesh.positions.len());
    let mut depth = Vec::with_capacity(mesh.positions.len());
    for p in &mesh.positions {
        let v = mul_vec(&m, p);
        sx.push((v[0] - f[0]) * cam.zoom + half);
        sy.push(-(v[1] - f[1]) * cam.zoom + half);
        depth.push(v[2]);
    }
    let nview = mesh.normals.iter().map(|n| mul_vec(&m, n)).collect();
    Projected {
        sx,
        sy,
        depth,
        nview,
    }
}
