//! Compare sheets: one shared camera, per-tile supersampling, labelled layout
//! and a deterministic PNG encoder.
//!
//! `focus` and `zoom` are shared by every mesh in a run; with no `focus`, the
//! first mesh's centroid is used for all. Meshes are never auto-fitted to
//! their own bounding boxes — that silently varies magnification between
//! panels, which is how a comparison starts lying.

use crate::camera::Camera;
use crate::font::draw_text;
use crate::obj::Mesh;
use crate::raster::{self, RenderOptions};
use image::ImageEncoder as _;
use image::imageops::FilterType;
use image::{GrayImage, Rgba, RgbaImage};
use thiserror::Error;

pub const DEFAULT_SHEET_SIZE: usize = 620;
pub const SHEET_PAD: usize = 12;
pub const SHEET_HEADER: usize = 26;
pub const LABEL_COLOR: Rgba<u8> = Rgba([200, 0, 0, 255]);
const LABEL_SCALE: usize = 2;

#[derive(Clone, Debug, PartialEq)]
pub struct CompareOptions {
    /// Pixels per world unit, shared by all tiles.
    pub zoom: f64,
    /// World point at every tile centre. `None` uses the first mesh centroid.
    pub focus: Option<[f64; 3]>,
    /// Rendered tile edge length in pixels.
    pub size: usize,
    /// Supersampling factor: tiles render at `size * aa` then filter down.
    pub aa: usize,
    /// Grid columns; 0 means one row.
    pub per_row: usize,
    /// One label per mesh, used positionally. `None` labels from item names.
    pub labels: Option<Vec<String>>,
    pub yaw: f64,
    pub pitch: f64,
    pub ambient: f64,
    pub light: [f64; 3],
    /// Background colour behind every tile and the sheet canvas.
    pub background: [u8; 3],
    /// Background opacity, 0..1: the alpha of uncovered pixels in the output
    /// PNG. 0 gives a fully transparent background; surface pixels are always
    /// opaque. Edge pixels inherit antialiased alpha because colourisation
    /// happens before the supersample downscale.
    pub background_opacity: f64,
}

impl Default for CompareOptions {
    fn default() -> Self {
        Self {
            zoom: 100.0,
            focus: None,
            size: DEFAULT_SHEET_SIZE,
            aa: 2,
            per_row: 0,
            labels: None,
            yaw: 0.0,
            pitch: 0.0,
            ambient: crate::shading::DEFAULT_AMBIENT,
            light: crate::shading::DEFAULT_LIGHT,
            background: crate::raster::DEFAULT_BACKGROUND,
            background_opacity: 1.0,
        }
    }
}

/// One panel's input: a display name (usually the file stem) and its mesh.
pub struct CompareItem<'a> {
    pub name: &'a str,
    pub mesh: &'a Mesh,
}

/// One textured panel's input: a display name and a [`TexturedMesh`] from
/// [`load_textured`](crate::load_textured).
pub struct TexturedCompareItem<'a> {
    pub name: &'a str,
    pub mesh: &'a crate::obj::TexturedMesh,
}

#[derive(Debug, Error)]
pub enum SheetError {
    #[error("{path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("no meshes to render")]
    Empty,
    #[error("{count} --label values for {want} meshes; provide one per mesh")]
    LabelCount { count: usize, want: usize },
    #[error("PNG encode failed: {0}")]
    Encode(String),
}

/// The composed RGBA sheet plus, per tile, a `size`×`size` coverage mask at
/// 0/255. The sheet carries the background's alpha channel; surface pixels
/// are always opaque.
pub struct CompareResult {
    pub sheet: RgbaImage,
    pub masks: Vec<GrayImage>,
}

/// Render every mesh under one identical camera into a labelled contact sheet.
pub fn render_compare(
    items: &[CompareItem],
    opts: &CompareOptions,
) -> Result<CompareResult, SheetError> {
    let names = validate_and_names(items.len(), opts, |k| items[k].name)?;
    let cam = camera_for(opts, items[0].mesh.centroid());
    let (aa, render_opts) = sheet_render_opts(opts);
    let rasters: Vec<raster::Raster> = items
        .iter()
        .map(|item| raster::render(item.mesh, &cam, &render_opts))
        .collect();
    compose_sheet(&rasters, &names, aa, opts)
}

/// Like [`render_compare`], but each panel's covered pixels are colored from
/// their material textures (`map_Kd` × Kd × Lambert). Geometry, camera and the
/// coverage masks follow exactly the untextured rules — texturing changes
/// presentation only.
pub fn render_compare_textured(
    items: &[TexturedCompareItem],
    opts: &CompareOptions,
) -> Result<CompareResult, SheetError> {
    let names = validate_and_names(items.len(), opts, |k| items[k].name)?;
    let cam = camera_for(opts, items[0].mesh.centroid());
    let (aa, render_opts) = sheet_render_opts(opts);
    let rasters: Vec<raster::Raster> = items
        .iter()
        .map(|item| raster::render_textured(item.mesh, &cam, &render_opts))
        .collect();
    compose_sheet(&rasters, &names, aa, opts)
}

fn validate_and_names<'a>(
    n: usize,
    opts: &'a CompareOptions,
    name_of: impl Fn(usize) -> &'a str,
) -> Result<Vec<&'a str>, SheetError> {
    if n == 0 {
        return Err(SheetError::Empty);
    }
    if let Some(labels) = &opts.labels
        && labels.len() < n
    {
        return Err(SheetError::LabelCount {
            count: labels.len(),
            want: n,
        });
    }
    Ok((0..n)
        .map(|k| match &opts.labels {
            Some(labels) => labels[k].as_str(),
            None => name_of(k),
        })
        .collect())
}

fn camera_for(opts: &CompareOptions, default_focus: [f64; 3]) -> Camera {
    Camera {
        focus: opts.focus.unwrap_or(default_focus),
        zoom: opts.zoom,
        yaw: opts.yaw,
        pitch: opts.pitch,
        roll: 0.0,
    }
}

fn sheet_render_opts(opts: &CompareOptions) -> (usize, RenderOptions) {
    let aa = opts.aa.max(1);
    (
        aa,
        RenderOptions {
            size: opts.size * aa,
            light: opts.light,
            ambient: opts.ambient,
        },
    )
}

/// Colorize, downscale, lay out and label — shared by both compare entry
/// points so sheets differ only by their surface colors, never by framing.
fn compose_sheet(
    rasters: &[raster::Raster],
    names: &[&str],
    aa: usize,
    opts: &CompareOptions,
) -> Result<CompareResult, SheetError> {
    let bg_alpha = (opts.background_opacity.clamp(0.0, 1.0) * 255.0).round() as u8;

    let mut tiles: Vec<RgbaImage> = Vec::with_capacity(rasters.len());
    let mut masks: Vec<GrayImage> = Vec::with_capacity(rasters.len());
    for raster in rasters {
        // Colourise at supersample resolution so both colour and alpha get
        // antialiased edges once filtered down.
        let big = raster.colorize(opts.background, bg_alpha);
        if aa == 1 {
            tiles.push(big);
            masks.push(raster.mask_image());
        } else {
            tiles.push(image::imageops::resize(
                &big,
                opts.size as u32,
                opts.size as u32,
                FilterType::Lanczos3,
            ));
            masks.push(downsample_mask(raster, opts.size, aa));
        }
    }

    let n = tiles.len();
    let cols = if opts.per_row == 0 { n } else { opts.per_row };
    let rows = n.div_ceil(cols);
    let sheet_w = cols * opts.size + (cols + 1) * SHEET_PAD;
    let sheet_h = rows * (opts.size + SHEET_HEADER) + SHEET_PAD;
    let mut sheet = RgbaImage::from_pixel(
        sheet_w as u32,
        sheet_h as u32,
        Rgba([
            opts.background[0],
            opts.background[1],
            opts.background[2],
            bg_alpha,
        ]),
    );

    for (k, tile) in tiles.iter().enumerate() {
        let (r, c) = ((k / cols), k % cols);
        let x = SHEET_PAD + c * (opts.size + SHEET_PAD);
        let y = SHEET_HEADER + SHEET_PAD + r * (opts.size + SHEET_HEADER);
        image::imageops::overlay(&mut sheet, tile, x as i64, y as i64);
        draw_text(
            &mut sheet,
            x + 4,
            y.saturating_sub(18),
            names[k],
            LABEL_COLOR,
            LABEL_SCALE,
        );
    }

    Ok(CompareResult { sheet, masks })
}

/// Collapse a supersampled coverage mask to `size`: a pixel is covered when a
/// strict majority of its `aa×aa` subsamples were rasterized. Majority is the
/// honest reduction — the mask says "geometry is here", antialiasing says
/// nothing about whether the surface exists.
fn downsample_mask(raster: &raster::Raster, size: usize, aa: usize) -> GrayImage {
    let mut data = vec![0u8; size * size];
    for ty in 0..size {
        for tx in 0..size {
            let mut hits = 0usize;
            for sy in 0..aa {
                for sx in 0..aa {
                    if raster.is_covered(tx * aa + sx, ty * aa + sy) {
                        hits += 1;
                    }
                }
            }
            data[ty * size + tx] = if hits * 2 > aa * aa { 255 } else { 0 };
        }
    }
    GrayImage::from_raw(size as u32, size as u32, data).expect("buffer sized to dimensions")
}

/// Deterministic PNG writer: fixed compression and filter settings, no
/// metadata or time chunks, so identical inputs produce identical bytes.
/// The sheet is RGBA, so transparent backgrounds survive the round trip.
pub fn write_png_rgba(path: &str, img: &RgbaImage) -> Result<(), SheetError> {
    write_png(
        path,
        img.as_raw(),
        img.width(),
        img.height(),
        image::ExtendedColorType::Rgba8,
    )
}

/// Deterministic grayscale PNG writer (coverage masks). See [`write_png_rgba`].
pub fn write_png_gray(path: &str, img: &GrayImage) -> Result<(), SheetError> {
    write_png(
        path,
        img.as_raw(),
        img.width(),
        img.height(),
        image::ExtendedColorType::L8,
    )
}

fn write_png(
    path: &str,
    data: &[u8],
    width: u32,
    height: u32,
    color: image::ExtendedColorType,
) -> Result<(), SheetError> {
    let file = std::fs::File::create(path).map_err(|source| SheetError::Io {
        path: path.to_string(),
        source,
    })?;
    let writer = std::io::BufWriter::new(file);
    let encoder = image::codecs::png::PngEncoder::new_with_quality(
        writer,
        image::codecs::png::CompressionType::Default,
        image::codecs::png::FilterType::Adaptive,
    );
    encoder
        .write_image(data, width, height, color)
        .map_err(|e| SheetError::Encode(format!("{path}: {e}")))
}
