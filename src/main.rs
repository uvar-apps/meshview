//! CLI: render one or more OBJ files into a labelled, camera-shared PNG.

use clap::Parser;
use meshview::{
    CompareItem, CompareOptions, load_obj, render_compare, write_png_gray, write_png_rgb,
};
use std::path::Path;
use std::process::ExitCode;

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

    /// Write each tile's coverage mask as a grayscale PNG beside the sheet:
    /// PATH for a single mesh, PATH_<k>.png per tile for several
    #[arg(long)]
    mask: Option<String>,
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

fn run(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    if args.meshes.is_empty() {
        return Err("no OBJ files given".into());
    }
    let focus = match &args.focus {
        Some(text) => {
            Some(parse_focus(text).ok_or(format!("--focus expects x,y,z, got `{text}`"))?)
        }
        None => None,
    };
    let mut loaded = Vec::with_capacity(args.meshes.len());
    for path in &args.meshes {
        loaded.push(load_obj(path)?);
    }
    let names: Vec<String> = args.meshes.iter().map(|p| file_stem(p)).collect();
    let items: Vec<CompareItem> = loaded
        .iter()
        .zip(&names)
        .map(|(mesh, name)| CompareItem { name, mesh })
        .collect();
    let opts = CompareOptions {
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
        ..Default::default()
    };
    let result = render_compare(&items, &opts)?;
    write_png_rgb(&args.out, &result.sheet)?;
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
