//! CLI: render one or more OBJ files into a labelled, camera-shared PNG.

use clap::Parser;
use meshview::{
    CompareItem, CompareOptions, TexturedCompareItem, load_obj, load_textured, render_compare,
    render_compare_textured, write_png_gray, write_png_rgba,
};
use std::path::Path;
use std::process::ExitCode;

/// Parse a background colour: `black`/`white`/`gray`, a gray level `0..=255`,
/// or hex `#rgb` / `#rrggbb`.
fn parse_bg(text: &str) -> Result<[u8; 3], String> {
    let lowered = text.trim().to_ascii_lowercase();
    match lowered.as_str() {
        "black" => return Ok([0, 0, 0]),
        "white" => return Ok([255, 255, 255]),
        "gray" | "grey" => return Ok([128, 128, 128]),
        _ => {}
    }
    if let Some(hex) = lowered.strip_prefix('#') {
        let component = |c: &str| -> Result<u8, String> {
            u8::from_str_radix(c, 16).map_err(|_| format!("`{text}` is not a valid color"))
        };
        return match hex.len() {
            3 => Ok([
                component(&hex[0..1])? * 17,
                component(&hex[1..2])? * 17,
                component(&hex[2..3])? * 17,
            ]),
            6 => Ok([
                component(&hex[0..2])?,
                component(&hex[2..4])?,
                component(&hex[4..6])?,
            ]),
            _ => Err(format!("`{text}`: hex colors need 3 or 6 digits")),
        };
    }
    if let Ok(v) = lowered.parse::<u8>() {
        return Ok([v, v, v]);
    }
    Err(format!(
        "`{text}`: expected black/white/gray, a gray level 0..=255, or #rgb/#rrggbb"
    ))
}

fn parse_opacity(text: &str) -> Result<f64, String> {
    let v: f64 = text
        .parse()
        .map_err(|_| format!("`{text}` is not a number"))?;
    if !(0.0..=1.0).contains(&v) {
        return Err(format!("--bg-opacity must be within 0..=1, got {text}"));
    }
    Ok(v)
}

#[derive(Parser, Debug)]
#[command(
    name = "meshview",
    version,
    about = "Render meshes into a labelled contact sheet under one shared orthographic camera."
)]
struct Args {
    /// OBJ files (positions, optional vn, faces)
    meshes: Vec<String>,

    /// Output PNG path
    #[arg(short, long, default_value = "render.png")]
    out: String,

    /// Pixels per world unit
    #[arg(long, default_value_t = 100.0)]
    zoom: f64,

    /// World point placed at every tile centre, as x,y,z
    #[arg(long)]
    focus: Option<String>,

    /// Tile edge length in pixels
    #[arg(long, default_value_t = 620)]
    size: usize,

    /// Supersampling factor (0 or 1 = off)
    #[arg(long, default_value_t = 2)]
    aa: usize,

    /// Grid columns (0 = one row)
    #[arg(long = "per-row", default_value_t = 0)]
    per_row: usize,

    /// Tile label, repeatable, one per mesh
    #[arg(long)]
    label: Vec<String>,

    /// Rotation about z in radians
    #[arg(long, default_value_t = 0.0)]
    yaw: f64,

    /// Rotation about x in radians
    #[arg(long, default_value_t = 0.0)]
    pitch: f64,

    /// Constant ambient, 0..1
    #[arg(long, default_value_t = 0.30)]
    ambient: f64,

    /// Background colour: black|white|gray, a gray level 0..=255, or
    /// #rgb/#rrggbb (default black)
    #[arg(long, default_value = "black", value_parser = parse_bg)]
    bg: [u8; 3],

    /// Background opacity 0..1; 0 makes the background fully transparent.
    /// Rendered surface stays opaque.
    #[arg(long, default_value_t = 1.0, value_parser = parse_opacity)]
    bg_opacity: f64,

    /// Write each tile's coverage mask as a grayscale PNG beside the sheet:
    /// PATH for a single mesh, PATH_<k>.png per tile for several
    #[arg(long)]
    mask: Option<String>,

    /// Color covered pixels from material textures (`map_Kd` × Kd, modulated
    /// by the same Lambert shading). Geometry, camera and coverage masks are
    /// unchanged: texturing is presentation only.
    #[arg(long)]
    textured: bool,
}

fn parse_focus(text: &str) -> Option<[f64; 3]> {
    let parts: Vec<f64> = text
        .replace(' ', "")
        .split(',')
        .filter_map(|v| v.parse::<f64>().ok())
        .collect();
    if parts.len() == 3 && text.replace(' ', "").split(',').count() == 3 {
        Some([parts[0], parts[1], parts[2]])
    } else {
        None
    }
}

fn file_stem(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_string()
}

fn mask_paths(path: &str, n: usize) -> Vec<String> {
    if n <= 1 {
        return vec![path.to_string()];
    }
    let p = Path::new(path);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("mask");
    let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("png");
    (0..n)
        .map(|k| {
            let name = format!("{stem}_{k}.{ext}");
            match p.parent() {
                Some(dir) if !dir.as_os_str().is_empty() => {
                    dir.join(name).to_string_lossy().into_owned()
                }
                _ => name,
            }
        })
        .collect()
}

fn compare_opts(args: &Args) -> Result<CompareOptions, Box<dyn std::error::Error>> {
    let focus = match &args.focus {
        Some(text) => {
            Some(parse_focus(text).ok_or(format!("--focus expects x,y,z, got `{text}`"))?)
        }
        None => None,
    };
    Ok(CompareOptions {
        zoom: args.zoom,
        focus,
        size: args.size,
        aa: args.aa,
        per_row: args.per_row,
        labels: if args.label.is_empty() {
            None
        } else {
            Some(args.label.clone())
        },
        yaw: args.yaw,
        pitch: args.pitch,
        ambient: args.ambient,
        background: args.bg,
        background_opacity: args.bg_opacity,
        ..Default::default()
    })
}

fn finish(args: &Args, result: meshview::CompareResult) -> Result<(), Box<dyn std::error::Error>> {
    write_png_rgba(&args.out, &result.sheet)?;
    println!("wrote {}", args.out);
    if let Some(mask) = &args.mask {
        for (mask_path, m) in mask_paths(mask, result.masks.len())
            .into_iter()
            .zip(&result.masks)
        {
            write_png_gray(&mask_path, m)?;
            println!("wrote {mask_path}");
        }
    }
    Ok(())
}

fn run(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    if args.meshes.is_empty() {
        return Err("no OBJ files given".into());
    }
    let names: Vec<String> = args.meshes.iter().map(|p| file_stem(p)).collect();
    let opts = compare_opts(args)?;
    if args.textured {
        let loaded: Vec<meshview::TexturedMesh> = args
            .meshes
            .iter()
            .map(|path| load_textured(path))
            .collect::<Result<_, _>>()?;
        let items: Vec<TexturedCompareItem> = loaded
            .iter()
            .zip(&names)
            .map(|(mesh, name)| TexturedCompareItem { name, mesh })
            .collect();
        let result = render_compare_textured(&items, &opts)?;
        finish(args, result)
    } else {
        let loaded: Vec<meshview::Mesh> = args
            .meshes
            .iter()
            .map(|path| load_obj(path))
            .collect::<Result<_, _>>()?;
        let items: Vec<CompareItem> = loaded
            .iter()
            .zip(&names)
            .map(|(mesh, name)| CompareItem { name, mesh })
            .collect();
        let result = render_compare(&items, &opts)?;
        finish(args, result)
    }
}

fn main() -> ExitCode {
    let args = Args::parse();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("meshview: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bg_parsing() {
        assert_eq!(parse_bg("black").unwrap(), [0, 0, 0]);
        assert_eq!(parse_bg("white").unwrap(), [255, 255, 255]);
        assert_eq!(parse_bg("GRAY").unwrap(), [128, 128, 128]);
        assert_eq!(parse_bg("40").unwrap(), [40, 40, 40]);
        assert_eq!(parse_bg("#ff8800").unwrap(), [255, 136, 0]);
        assert_eq!(parse_bg("#f80").unwrap(), [255, 136, 0]);
        assert!(parse_bg("#ff88").is_err());
        assert!(parse_bg("256").is_err());
        assert!(parse_bg("chartreuse").is_err());
        assert!(parse_opacity("0.5").is_ok());
        assert!(parse_opacity("0").is_ok());
        assert!(parse_opacity("1").is_ok());
        assert!(parse_opacity("1.5").is_err());
        assert!(parse_opacity("-0.1").is_err());
        assert!(parse_opacity("x").is_err());
    }

    #[test]
    fn focus_parsing() {
        assert_eq!(parse_focus("1,2,3"), Some([1.0, 2.0, 3.0]));
        assert_eq!(parse_focus(" 4.29, 5.05 , 4.55 "), Some([4.29, 5.05, 4.55]));
        assert_eq!(parse_focus("1,2"), None);
        assert_eq!(parse_focus("1,2,3,4"), None);
        assert_eq!(parse_focus("a,b,c"), None);
    }

    #[test]
    fn mask_naming() {
        assert_eq!(mask_paths("m.png", 1), vec!["m.png".to_string()]);
        let two = mask_paths("out/mask.png", 2);
        assert_eq!(
            two,
            vec!["out/mask_0.png".to_string(), "out/mask_1.png".to_string()]
        );
    }
}
