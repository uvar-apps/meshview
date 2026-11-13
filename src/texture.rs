//! Texture decoding and sampling for the opt-in textured mode.
//!
//! Texturing colors pixels the z-buffer has already resolved — it never
//! touches visibility or coverage (that is the third honesty rule), and the
//! grayscale surface buffer is still written identically, so a textured and
//! an untextured render of the same mesh share one coverage mask bit for bit.
//!
//! Sampling is deterministic float64 math: UVs wrap by fract (repeat), `v`
//! is flipped once up front (`t = 1 − v`) so the conventional OBJ
//! bottom-left origin maps onto the image's top-left row zero, and texels
//! are bilinearly interpolated.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum TextureError {
    #[error("{path}: {source}")]
    Decode {
        path: String,
        source: image::ImageError,
    },
}

/// A decoded RGB8 texture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Texture {
    width: u32,
    height: u32,
    rgb: Vec<u8>,
}

impl Texture {
    /// Decode a texture file (PNG or JPEG; whatever the `image` crate's
    /// enabled features read).
    pub fn load(path: &str) -> Result<Self, TextureError> {
        let img = image::open(path).map_err(|source| TextureError::Decode {
            path: path.to_string(),
            source,
        })?;
        let rgb = img.to_rgb8();
        let (width, height) = rgb.dimensions();
        Ok(Self {
            width,
            height,
            rgb: rgb.into_raw(),
        })
    }

    /// Build from raw bytes (tests, procedural fixtures).
    pub fn from_rgb(width: u32, height: u32, rgb: Vec<u8>) -> Self {
        debug_assert_eq!(rgb.len(), (width * height * 3) as usize, "buffer size");
        Self { width, height, rgb }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Wrapped bilinear sample. `u`/`v` are texture coordinates in OBJ
    /// convention (v up, unbounded); the wrap is `fract` and the v-flip
    /// happens here, so callers pass raw interpolated UV. Result channels
    /// are 0.0..=255.0 floats for exact shading math.
    #[must_use]
    pub fn sample(&self, u: f64, v: f64) -> [f64; 3] {
        let w = self.width as i64;
        let h = self.height as i64;
        // texel-center sampling: coordinate c in [0,1) lands at c*W − 0.5
        let fx = fract(u) * w as f64 - 0.5;
        let fy = fract(1.0 - v) * h as f64 - 0.5;
        let x0 = fx.floor();
        let y0 = fy.floor();
        let tx = fx - x0;
        let ty = fy - y0;
        let px = |ix: i64, iy: i64| -> [f64; 3] {
            let cx = ix.rem_euclid(w) as usize;
            let cy = iy.rem_euclid(h) as usize;
            let i = (cy * self.width as usize + cx) * 3;
            [
                self.rgb[i] as f64,
                self.rgb[i + 1] as f64,
                self.rgb[i + 2] as f64,
            ]
        };
        let x0i = x0 as i64;
        let y0i = y0 as i64;
        let c00 = px(x0i, y0i);
        let c10 = px(x0i + 1, y0i);
        let c01 = px(x0i, y0i + 1);
        let c11 = px(x0i + 1, y0i + 1);
        let mut out = [0.0f64; 3];
        for k in 0..3 {
            let top = c00[k] + (c10[k] - c00[k]) * tx;
            let bottom = c01[k] + (c11[k] - c01[k]) * tx;
            out[k] = top + (bottom - top) * ty;
        }
        out
    }
}
/// `x − floor(x)` for any finite x (wraps negatives into [0,1)).
#[inline]
fn fract(x: f64) -> f64 {
    x - x.floor()
}
