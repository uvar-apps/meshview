//! The z-buffer triangle rasterizer.
//!
//! Visibility is the z-buffer and nothing else — no face is ever culled on a
//! geometric orientation test, because such a test disagrees with the stored
//! smooth normals of a creased surface and used to paint a checkerboard of
//! background speckles onto every mesh. Faces are all drawn; shading turns the
//! interpolated normal toward the camera (see [`crate::shading`]).
//!
//! Coverage is an explicit boolean mask returned alongside the pixels, never
//! inferred from the pixel values: a fully lit surface pixel reads exactly 255,
//! which equals the background, so colour alone cannot tell "brightest pixel"
//! from "no surface here".

use crate::camera::{Camera, project_mesh};
use crate::obj::Mesh;
use crate::shading::{DEFAULT_AMBIENT, DEFAULT_LIGHT, normalize, shade_two_sided_unit};

pub const DEFAULT_TILE_SIZE: usize = 900;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderOptions {
    /// Tile edge length in pixels; the image is square.
    pub size: usize,
    /// Light direction, normalized on use, expressed relative to the camera.
    pub light: [f64; 3],
    /// Constant ambient floor for the Lambert term, 0..1.
    pub ambient: f64,
    /// Background gray level; 255 (white) in the Python original.
    pub background: u8,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            size: DEFAULT_TILE_SIZE,
            light: DEFAULT_LIGHT,
            ambient: DEFAULT_AMBIENT,
            background: 255,
        }
    }
}

/// A square grayscale pixel buffer plus the explicit coverage mask.
#[derive(Clone, Debug, PartialEq)]
pub struct Raster {
    size: usize,
    gray: Vec<u8>,
    covered: Vec<bool>,
}

impl Raster {
    pub fn size(&self) -> usize {
        self.size
    }

    #[inline]
    pub fn pixel(&self, x: usize, y: usize) -> u8 {
        self.gray[y * self.size + x]
    }

    #[inline]
    pub fn is_covered(&self, x: usize, y: usize) -> bool {
        self.covered[y * self.size + x]
    }

    pub fn covered_count(&self) -> usize {
        self.covered.iter().filter(|c| **c).count()
    }

    /// The pixels as an 8-bit grayscale image.
    pub fn gray_image(&self) -> image::GrayImage {
        image::GrayImage::from_raw(self.size as u32, self.size as u32, self.gray.clone())
            .expect("buffer sized to dimensions")
    }

    /// The coverage mask as a grayscale image: 255 where geometry covers,
    /// 0 where there is no surface, independent of shading.
    pub fn mask_image(&self) -> image::GrayImage {
        let data: Vec<u8> = self.covered.iter().map(|&c| if c { 255 } else { 0 }).collect();
        image::GrayImage::from_raw(self.size as u32, self.size as u32, data)
            .expect("buffer sized to dimensions")
    }
}

/// Barycentric hit tolerance, in normalized barycentric units.
const EPS: f64 = -1e-7;

/// Shade `mesh` into a grayscale tile under `cam`.
pub fn render(mesh: &Mesh, cam: &Camera, opts: &RenderOptions) -> Raster {
    let size = opts.size;
    if size == 0 {
        return Raster {
            size: 0,
            gray: Vec::new(),
            covered: Vec::new(),
        };
    }
    let pr = project_mesh(cam, mesh, size);
    let sx = &pr.sx;
    let sy = &pr.sy;
    let depth = &pr.depth;
    let nview = &pr.nview;
    let l = normalize(opts.light);

    let mut img = vec![opts.background; size * size];
    let mut zbuf = vec![f64::NEG_INFINITY; size * size];
    let mut covered = vec![false; size * size];

    // Cheap window reject: sort only the faces whose bbox touches the
    // viewport. Mean depth ascending — far faces first on a strict `z >`
    // z-buffer; stable, so exact ties resolve by face order deterministically.
    let sizef = size as f64;
    let mut jobs: Vec<(u32, f64)> = Vec::with_capacity(mesh.indices.len());
    for (t, tri) in mesh.indices.iter().enumerate() {
        let (a, b, c) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        let (fx0, fx1, fx2) = (sx[a], sx[b], sx[c]);
        let (fy0, fy1, fy2) = (sy[a], sy[b], sy[c]);
        let inside = fx0.max(fx1).max(fx2) >= 0.0
            && fx0.min(fx1).min(fx2) < sizef
            && fy0.max(fy1).max(fy2) >= 0.0
            && fy0.min(fy1).min(fy2) < sizef;
        if inside {
            let mean = (depth[a] + depth[b] + depth[c]) * (1.0 / 3.0);
            jobs.push((t as u32, mean));
        }
    }
    jobs.sort_by(|a, b| a.1.total_cmp(&b.1));

    for &(t, _) in &jobs {
        let tri = mesh.indices[t as usize];
        let (i0, i1, i2) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        let scx0 = sx[i0];
        let scy0 = sy[i0];
        let ax = sx[i1] - scx0;
        let ay = sy[i1] - scy0;
        let bx = sx[i2] - scx0;
        let by = sy[i2] - scy0;
        let den = ax * by - ay * bx;
        if den.abs() < 1e-9 {
            continue;
        }
        let inv_den = 1.0 / den;

        // Pixel-center bbox, clamped into the tile.
        let minx = scx0.min(sx[i1]).min(sx[i2]);
        let maxx = scx0.max(sx[i1]).max(sx[i2]);
        let miny = scy0.min(sy[i1]).min(sy[i2]);
        let maxy = scy0.max(sy[i1]).max(sy[i2]);
        let x0 = minx.floor().max(0.0) as usize;
        let y0 = miny.floor().max(0.0) as usize;
        let x1 = (ceil_usize(maxx)).min(size - 1);
        let y1 = (ceil_usize(maxy)).min(size - 1);

        let d0 = depth[i0];
        let d1 = depth[i1];
        let d2 = depth[i2];
        let n0 = nview[i0];
        let n1 = nview[i1];
        let n2 = nview[i2];

        // w1 and w2 are affine in the integer pixel column p:
        // w_i(p) = a_i·p + q_i, and w0 = 1 − w1 − w2. Solving w_i ≥ EPS
        // per row yields a conservative span (one to two pixels wide of the
        // true one per edge); every pixel inside it is still tested exactly.
        let a1 = by * inv_den;
        let a2 = -ay * inv_den;
        let a0 = -(a1 + a2);
        let kx = 0.5 - scx0;

        for py in y0..=y1 {
            let pyc = (py as f64 + 0.5) - scy0;
            let q1 = a1 * kx - bx * pyc * inv_den;
            let q2 = a2 * kx + ax * pyc * inv_den;
            let q0 = 1.0 - q1 - q2;
            let mut lo = x0;
            let mut hi = x1;
            if !span_half_line(a1, EPS - q1, &mut lo, &mut hi)
                || !span_half_line(a2, EPS - q2, &mut lo, &mut hi)
                || !span_half_line(a0, EPS - q0, &mut lo, &mut hi)
            {
                continue;
            }
            let base = py * size;
            for px in lo..=hi {
                let pxc = (px as f64 + 0.5) - scx0;
                let w1 = (by * pxc - bx * pyc) * inv_den;
                let w2 = (-ay * pxc + ax * pyc) * inv_den;
                let w0 = 1.0 - w1 - w2;
                if w0 < EPS || w1 < EPS || w2 < EPS {
                    continue;
                }
                let z = w1 * d1 + w2 * d2 + w0 * d0;
                let idx = base + px;
                if z <= zbuf[idx] {
                    continue;
                }
                zbuf[idx] = z;
                covered[idx] = true;
                let nx = w1 * n1[0] + w2 * n2[0] + w0 * n0[0];
                let ny = w1 * n1[1] + w2 * n2[1] + w0 * n0[1];
                let nz = w1 * n1[2] + w2 * n2[2] + w0 * n0[2];
                let nl = (nx * nx + ny * ny + nz * nz).sqrt();
                let inv_n = 1.0 / nl.max(1e-12);
                let shade = shade_two_sided_unit(
                    [nx * inv_n, ny * inv_n, nz * inv_n],
                    l,
                    opts.ambient,
                );
                img[idx] = (255.0 * shade).clamp(0.0, 255.0) as u8;
            }
        }
    }

    Raster {
        size,
        gray: img,
        covered,
    }
}

/// `ceil(x)` saturating into usize without panicking outside range.
#[inline]
fn ceil_usize(x: f64) -> usize {
    let c = x.ceil();
    if c <= 0.0 { 0 } else { c.min(u32::MAX as f64) as usize }
}

/// Intersect the running integer span `[lo, hi]` with the half-line
/// `{ p : slope * p >= thresh }` for one barycentric coordinate along the row.
/// Returns false when the span becomes empty. The result is then padded by one
/// pixel before exact per-pixel tests, so rounding here may be sloppy in the
/// conservative direction but must not drop the interval.
#[inline]
fn span_half_line(slope: f64, thresh: f64, lo: &mut usize, hi: &mut usize) -> bool {
    const BIG: f64 = 1e300;
    if slope.abs() < 1e-300 {
        // Constant along the row: keep or drop the whole span.
        return thresh <= 0.0;
    }
    // Solve slope * p >= thresh.
    let bound = thresh / slope;
    if slope > 0.0 {
        let b = (bound.floor() - 1.0).clamp(0.0, BIG) as usize;
        if b > *lo {
            *lo = b;
        }
    } else {
        let b = (bound.ceil() + 1.0).clamp(0.0, BIG) as usize;
        if b < *hi {
            *hi = b;
        }
    }
    *lo <= *hi
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_size_returns_empty() {
        let mesh = Mesh {
            positions: vec![[0.0; 3]],
            normals: vec![[0.0, 0.0, 1.0]],
            indices: vec![],
        };
        let r = render(&mesh, &Camera::new([0.0; 3], 1.0), &RenderOptions { size: 0, ..Default::default() });
        assert_eq!(r.size(), 0);
    }
}
