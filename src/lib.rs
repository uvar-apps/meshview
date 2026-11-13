//! `meshview` — a headless, CPU-only rasterizer for **comparing triangle
//! meshes side by side**.
//!
//! Load OBJ files, render each under one identical orthographic camera into a
//! labelled contact-sheet PNG. No GPU, no window system, no interactivity.
//! It exists so that the same world-space region of several meshes appears at
//! the same scale and orientation: a pixel difference means a geometry
//! difference and nothing else. It reads any OBJ.
//!
//! # Four properties this crate refuses to break
//!
//! 1. **Nothing is back-face culled geometrically; visibility is the
//!    z-buffer.** Culling on a geometric orientation test disagrees with the
//!    stored smooth normals of a creased surface — it once removed edge-on
//!    triangles and left a checkerboard of background speckles on *every*
//!    mesh, including the reference being compared against. Orientation
//!    belongs to shading: the interpolated normal is turned toward the camera
//!    before lighting ([`shading`]), so an inverted normal renders dark
//!    instead of punching a hole.
//! 2. **Normals bind by the face-corner's `vn` index, not by arrival order.**
//!    In `a/b/c`, `b` is a texture coordinate and never a normal. A `vn` is
//!    bound to the position it is referenced from (flat-shaded exports
//!    routinely index `vn` differently from `v`); vertices that end up with no
//!    bound normal get area-weighted geometric normals, and everything is
//!    re-normalized because exported normals are not always unit length. See
//!    [`load_obj`].
//! 3. **Coverage is an explicit boolean mask returned by the renderer, never
//!    inferred from pixel values.** With the Python original's white
//!    background, a fully lit surface pixel read exactly 255 — the same byte
//!    as "no surface here"; a hole-check that conflated them once reported
//!    3,208 fake holes on a flawless sphere. The same collision can recur for
//!    *any* background (`--bg`), so every hole claim goes through the mask.
//!    See [`Raster`] and [`Raster::colorize`].
//! 4. **One camera for the whole sheet.** `focus` (the world point placed at
//!    the tile centre) and `zoom` (pixels per world unit) are shared by all
//!    meshes in a compare run; with no focus, the first mesh's centroid is
//!    used for all. Meshes are never auto-fitted to individual bounding
//!    boxes — varying magnification between panels is how a comparison starts
//!    lying. See [`render_compare`].
//!
//! # Library use
//!
//! ```
//! use meshview::{load_obj, Camera, RenderOptions, render};
//!
//! let mesh = load_obj("assets/samples/1crn_ses_0.5A.obj").unwrap();
//! let cam = Camera::new(mesh.centroid(), 30.0);
//! let out = render(&mesh, &cam, &RenderOptions { size: 64, ..Default::default() });
//! assert!(out.covered_count() > 1000, "the protein should fill the small tile");
//! ```

pub mod camera;
mod font;
pub mod obj;
pub mod raster;
pub mod shading;
pub mod sheet;
pub mod texture;

pub use camera::Camera;
pub use obj::{Mesh, ObjError, TexturedMesh, TriTex, load_obj, load_obj_from_str, load_textured};
pub use raster::{Raster, RenderOptions, render, render_textured};
pub use sheet::{
    CompareItem, CompareOptions, CompareResult, SheetError, TexturedCompareItem, render_compare,
    render_compare_textured, write_png_gray, write_png_rgba,
};
pub use texture::{Texture, TextureError};
