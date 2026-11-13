//! Two-sided Lambert shading, done in view space.
//!
//! The shading normal is `M·n` (world normal through the camera matrix), so
//! the light direction is relative to the camera, not the world — two panels
//! with different geometry but the same camera are directly comparable under
//! it.
//!
//! Before lighting, the interpolated normal is turned toward the viewer
//! (two-sided): an inverted stored normal renders dark instead of punching a
//! hole at background colour. Visibility belongs to the z-buffer alone;
//! orientation belongs to shading.

pub const DEFAULT_LIGHT: [f64; 3] = [0.35, 0.45, 0.82];
pub const DEFAULT_AMBIENT: f64 = 0.30;

/// Normalize with the same guard the rasterizer uses: a zero-length vector
/// maps to itself, never to NaN.
pub(crate) fn normalize(v: [f64; 3]) -> [f64; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    let d = len.max(1e-12);
    [v[0] / d, v[1] / d, v[2] / d]
}

/// `ambient + (1 − ambient) · max(0, n̂·l̂)` after turning `n` toward the
/// camera (+z is out of the screen toward the viewer). `n` must be unit.
#[inline]
pub(crate) fn shade_two_sided_unit(n: [f64; 3], l: [f64; 3], ambient: f64) -> f64 {
    let sgn = if n[2] < 0.0 { -1.0 } else { 1.0 };
    let d = (n[0] * l[0] + n[1] * l[1] + n[2] * l[2]) * sgn;
    ambient + (1.0 - ambient) * d.max(0.0)
}
