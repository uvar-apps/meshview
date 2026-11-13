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
use image::{GrayImage, Rgb, RgbImage};
use thiserror::Error;

pub const DEFAULT_SHEET_SIZE: usize = 620;
pub const SHEET_PAD: usize = 12;
pub const SHEET_HEADER: usize = 26;
pub const SHEET_BG: Rgb<u8> = Rgb([248, 248, 248]);
pub const LABEL_COLOR: Rgb<u8> = Rgb([200, 0, 0]);
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
        }
    }
}

/// One panel's input: a display name (usually the file stem) and its mesh.
pub struct CompareItem<'a> {
    pub name: &'a str,
    pub mesh: &'a Mesh,
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

/// The composed sheet plus, per tile, a `size`×`size` coverage mask at 0/255.
pub struct CompareResult {
    pub sheet: RgbImage,
    pub masks: Vec<GrayImage>,
}

/// Render every mesh under one identical camera into a labelled contact sheet.
pub fn render_compare(items: &[CompareItem], opts: &CompareOptions) -> Result<CompareResult, SheetError> {
    if items.is_empty() {
        return Err(SheetError::Empty);
    }
    if let Some(labels) = &opts.labels
        && labels.len() < items.len()
    {
        return Err(SheetError::LabelCount {
            count: labels.len(),
            want: items.len(),
        });
    }
    let focus = opts.focus.unwrap_or_else(|| items[0].mesh.centroid());
    let cam = Camera {
        focus,
        zoom: opts.zoom,
        yaw: opts.yaw,
        pitch: opts.pitch,
        roll: 0.0,
    };

    let aa = opts.aa.max(1);
    let render_opts = RenderOptions {
        size: opts.size * aa,
        light: opts.light,
        ambient: opts.ambient,
        background: 255,
    };

    let mut tiles: Vec<GrayImage> = Vec::with_capacity(items.len());
    let mut masks: Vec<GrayImage> = Vec::with_capacity(items.len());
    for item in items {
        let raster = raster::render(item.mesh, &cam, &render_opts);
        if aa == 1 {
            tiles.push(raster.gray_image());
            masks.push(raster.mask_image());
        } else {
            let big = raster.gray_image();
            tiles.push(image::imageops::resize(
                &big,
                opts.size as u32,
                opts.size as u32,
                FilterType::Lanczos3,
            ));
            masks.push(downsample_mask(
                &raster,
                opts.size,
                aa,
            ));
        }
    }

    let n = tiles.len();
    let cols = if opts.per_row == 0 { n } else { opts.per_row };
    let rows = n.div_ceil(cols);
    let sheet_w = cols * opts.size + (cols + 1) * SHEET_PAD;
    let sheet_h = rows * (opts.size + SHEET_HEADER) + SHEET_PAD;
    let mut sheet = RgbImage::from_pixel(sheet_w as u32, sheet_h as u32, SHEET_BG);

    for (k, tile) in tiles.iter().enumerate() {
        let (r, c) = ((k / cols), k % cols);
        let x = SHEET_PAD + c * (opts.size + SHEET_PAD);
        let y = SHEET_HEADER + SHEET_PAD + r * (opts.size + SHEET_HEADER);
        let rgb = RgbImage::from_fn(tile.width(), tile.height(), |x, y| {
            let v = tile.get_pixel(x, y)[0];
            Rgb([v, v, v])
        });
        image::imageops::overlay(&mut sheet, &rgb, x as i64, y as i64);
        let name = match &opts.labels {
            Some(labels) => &labels[k],
            None => item_name(items, k),
        };
        draw_text(
            &mut sheet,
            x + 4,
            y.saturating_sub(18),
            name,
            LABEL_COLOR,
            LABEL_SCALE,
        );
    }

    Ok(CompareResult { sheet, masks })
}

fn item_name<'a>(items: &'a [CompareItem<'a>], k: usize) -> &'a str {
    items[k].name
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
pub fn write_png_rgb(path: &str, img: &RgbImage) -> Result<(), SheetError> {
    write_png(path, img.as_raw(), img.width(), img.height(), image::ExtendedColorType::Rgb8)
}

/// Deterministic grayscale PNG writer. See [`write_png_rgb`].
pub fn write_png_gray(path: &str, img: &GrayImage) -> Result<(), SheetError> {
    write_png(path, img.as_raw(), img.width(), img.height(), image::ExtendedColorType::L8)
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
    encoder.write_image(data, width, height, color).map_err(|e| {
        SheetError::Encode(format!("{path}: {e}"))
    })
}
