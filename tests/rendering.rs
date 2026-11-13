//! Integration properties: shared camera framing, self-compare equality,
//! mask round-trips, byte-level determinism, and the "255-on-255" coverage
//! lesson encoded as an assertion.

mod common;

use common::sample;
use meshview::{
    Camera, CompareItem, CompareOptions, Mesh, RenderOptions, load_obj, render, render_compare,
    write_png_gray, write_png_rgba,
};

fn tmp(name: &str) -> String {
    format!(
        "{}/meshview_it_{}_{}",
        std::env::temp_dir().display(),
        std::process::id(),
        name
    )
}

// --focus puts the given world point within one pixel of the tile centre.
#[test]
fn focus_lands_on_tile_centre() {
    let focus = [4.29, 5.05, 4.55];
    let cam = Camera {
        focus,
        zoom: 110.0,
        ..Camera::new([0.0; 3], 1.0)
    };
    let size = 620usize;
    let (x, y, _) = cam.project_point(size, &focus);
    assert!(
        (x - size as f64 / 2.0).abs() < 1.0 && (y - size as f64 / 2.0).abs() < 1.0,
        "focus projected to ({x},{y}), want ({},{})",
        size / 2,
        size / 2
    );
    // And stays centred under yaw/pitch too — the focus maps through M.
    let cam = Camera {
        focus,
        zoom: 40.0,
        yaw: 0.7,
        pitch: -1.2,
        roll: 0.3,
    };
    let (x, y, _) = cam.project_point(size, &focus);
    assert!(
        (x - size as f64 / 2.0).abs() < 1e-9 && (y - size as f64 / 2.0).abs() < 1e-9,
        "rotated focus drifted to ({x},{y})"
    );
}

// Compare-with-itself: two panels of the same mesh must be pixel-identical.
#[test]
fn compare_with_itself_is_identical() {
    let mesh = load_obj(&sample("1crn_ses_0.5A.obj")).unwrap();
    let items = vec![
        CompareItem {
            name: "left",
            mesh: &mesh,
        },
        CompareItem {
            name: "right",
            mesh: &mesh,
        },
    ];
    let opts = CompareOptions {
        zoom: 30.0,
        size: 160,
        aa: 1,
        ..Default::default()
    };
    let res = render_compare(&items, &opts).unwrap();
    let (w, h) = res.sheet.dimensions();
    assert_eq!(h as usize, 160 + 26 + 12);
    // tiles at x = pad and x = pad + size + pad, y = header + pad
    let (x0l, y0) = (12usize, 38usize);
    let x0r = 12 + 160 + 12;
    for dy in 0..160 {
        for dx in 0..160 {
            let l = res.sheet.get_pixel((x0l + dx) as u32, (y0 + dy) as u32);
            let r = res.sheet.get_pixel((x0r + dx) as u32, (y0 + dy) as u32);
            assert_eq!(l, r, "panels differ at tile ({dx},{dy})");
        }
    }
    let _ = w;
}

// The coverage mask is honest: a fully lit surface pixel reads exactly 255,
// which is the background — only the mask tells them apart.
#[test]
fn covered_pixel_can_equal_background_byte() {
    let mesh = Mesh {
        positions: vec![[-0.3, -0.3, 0.0], [0.3, -0.3, 0.0], [0.0, 0.3, 0.0]],
        // Inverted normals: two-sided shading must light the face to full.
        normals: vec![[0.0, 0.0, -1.0]; 3],
        indices: vec![[0, 1, 2]],
    };
    let cam = Camera::new([0.0, 0.0, 0.0], 50.0);
    let out = render(
        &mesh,
        &cam,
        &RenderOptions {
            size: 64,
            ..Default::default()
        },
    );
    // Centre of the triangle: lit to 255 with the default light (z=0.82…) —
    // the interpolated normal is exactly (0,0,−1) everywhere here, flipped to
    // (0,0,+1), dot with light z-tilted… use pure +z light for full white.
    let lit = render(
        &mesh,
        &cam,
        &RenderOptions {
            size: 64,
            light: [0.0, 0.0, 1.0],
            ..Default::default()
        },
    );
    for (name, r) in [("tilted", &out), ("upright", &lit)] {
        let covered = r.is_covered(32, 32);
        assert!(covered, "{name}: centre must be covered");
    }
    // With upright light the fully lit pixel reads exactly 255 — and over a
    // white background it is byte-identical to "no surface here". Only the
    // mask distinguishes them.
    assert_eq!(lit.pixel(32, 32), 255, "lit pixel must saturate to 255");
    assert!(lit.is_covered(32, 32));
    assert!(!lit.is_covered(2, 2), "corner outside triangle uncovered");
    let on_white = lit.colorize([255, 255, 255], 255);
    assert_eq!(on_white.get_pixel(32, 32).0, [255, 255, 255, 255]);
    assert_eq!(on_white.get_pixel(2, 2).0, [255, 255, 255, 255]);
    assert!(
        lit.is_covered(32, 32) && !lit.is_covered(2, 2),
        "mask is the only truth when surface and background share a byte value"
    );
}

// Background colour and opacity: black default, arbitrary colour, and a
// transparent canvas in which surface stays fully opaque.
#[test]
fn background_colour_and_alpha() {
    let mesh = mesh_with_triangle();
    let cam = Camera::new([0.0, 0.0, 0.0], 50.0);
    let lit = render(
        &mesh,
        &cam,
        &RenderOptions {
            size: 64,
            light: [0.0, 0.0, 1.0],
            ..Default::default()
        },
    );
    // Opaque black (the default): corner carries the colour, centre the surface.
    let black = lit.colorize(crate_default_bg(), 255);
    assert_eq!(black.get_pixel(2, 2).0, [0, 0, 0, 255]);
    assert_eq!(black.get_pixel(32, 32).0, [255, 255, 255, 255]);
    // Fully transparent: uncovered pixels hold alpha 0, surface alpha 255.
    let glass = lit.colorize([10, 20, 30], 0);
    assert_eq!(glass.get_pixel(2, 2).0, [10, 20, 30, 0]);
    assert_eq!(glass.get_pixel(32, 32).0, [255, 255, 255, 255]);
    // A compare sheet at opacity 0 embeds that alpha in the PNG itself.
    let items = vec![CompareItem {
        name: "t",
        mesh: &mesh,
    }];
    let res = render_compare(
        &items,
        &CompareOptions {
            zoom: 50.0,
            focus: Some([0.0, 0.0, 0.0]),
            size: 64,
            aa: 1,
            light: [0.0, 0.0, 1.0],
            background: [10, 20, 30],
            background_opacity: 0.0,
            ..Default::default()
        },
    )
    .unwrap();
    // Sheet margin pixel: same transparent background.
    assert_eq!(res.sheet.get_pixel(1, 1).0, [10, 20, 30, 0]);
    // Deep inside the triangle (well away from antialiased edges): opaque
    // white surface.
    assert_eq!(
        res.sheet.get_pixel(12 + 32, 38 + 40).0,
        [255, 255, 255, 255]
    );
    // Deep background inside the tile: fully transparent in both channels.
    assert_eq!(res.sheet.get_pixel(12 + 2, 38 + 2).0, [10, 20, 30, 0]);
    // And a round trip through the encoder preserves alpha.
    let path = tmp("alpha.png");
    meshview::write_png_rgba(&path, &res.sheet).unwrap();
    let back = image::open(&path).unwrap().to_rgba8();
    assert_eq!(back.get_pixel(1, 1).0, [10, 20, 30, 0]);
    std::fs::remove_file(&path).ok();
}

fn mesh_with_triangle() -> meshview::Mesh {
    meshview::Mesh {
        positions: vec![[-0.3, -0.3, 0.0], [0.3, -0.3, 0.0], [0.0, 0.3, 0.0]],
        normals: vec![[0.0, 0.0, -1.0]; 3],
        indices: vec![[0, 1, 2]],
    }
}

fn crate_default_bg() -> [u8; 3] {
    meshview::raster::DEFAULT_BACKGROUND
}

// The mask PNG round-trips and agrees with the renderer's coverage.
#[test]
fn mask_png_round_trip() {
    let mesh = load_obj(&sample("stanford-bunny.obj")).unwrap();
    let cam = Camera::new(mesh.centroid(), 2200.0);
    let r = render(
        &mesh,
        &cam,
        &RenderOptions {
            size: 120,
            ..Default::default()
        },
    );
    let path = tmp("mask_roundtrip.png");
    write_png_gray(&path, &r.mask_image()).unwrap();
    let back = image::open(&path).unwrap().to_luma8();
    let mask = r.mask_image();
    assert_eq!(back.dimensions(), mask.dimensions());
    for (a, b) in back.pixels().zip(mask.pixels()) {
        assert_eq!(a.0, b.0, "round-trip changed mask bytes");
    }
    // 0/255 only, and consistent with is_covered.
    for y in [0usize, 60, 119] {
        for x in [0usize, 60, 119] {
            let v = r.mask_image().get_pixel(x as u32, y as u32)[0];
            assert_eq!(v, if r.is_covered(x, y) { 255 } else { 0 });
        }
    }
    std::fs::remove_file(&path).ok();
}

// Two full render + encode runs produce byte-identical PNGs.
#[test]
fn sheet_png_is_byte_deterministic() {
    let mesh = load_obj(&sample("1crn_ses_0.5A.obj")).unwrap();
    let items = vec![CompareItem {
        name: "twice",
        mesh: &mesh,
    }];
    let opts = CompareOptions {
        zoom: 90.0,
        size: 150,
        aa: 2,
        ..Default::default()
    };
    let a = tmp("det_a.png");
    let b = tmp("det_b.png");
    let r1 = render_compare(&items, &opts).unwrap();
    write_png_rgba(&a, &r1.sheet).unwrap();
    let r2 = render_compare(&items, &opts).unwrap();
    write_png_rgba(&b, &r2.sheet).unwrap();
    assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
}

// The compare camera is shared and never auto-fits: translating a mesh moves
// its geometry away from the fixed view region, and recentering the focus by
// the same translation reproduces the original panel exactly.
#[test]
fn shared_camera_does_not_auto_fit() {
    let mesh = load_obj(&sample("1crn_ses_0.5A.obj")).unwrap();
    let t = 80.0f64;
    let mut translated = mesh.clone();
    for p in &mut translated.positions {
        for c in p.iter_mut() {
            *c += t;
        }
    }
    // A stray far vertex an auto-fitter would chase; a shared camera rejects
    // its triangles with the window test.
    translated.positions.push([0.0, 0.0, 5_000.0]);
    translated.normals.push([0.0, 0.0, 1.0]);

    let focus = mesh.centroid();
    let small = CompareOptions {
        zoom: 30.0,
        focus: Some(focus),
        size: 160,
        aa: 1,
        ..Default::default()
    };
    let moved = CompareOptions {
        zoom: 30.0,
        focus: Some([focus[0] + t, focus[1] + t, focus[2] + t]),
        size: 160,
        aa: 1,
        ..Default::default()
    };

    let a = render_compare(
        &[CompareItem {
            name: "a",
            mesh: &mesh,
        }],
        &small,
    )
    .unwrap();
    let b = render_compare(
        &[CompareItem {
            name: "b",
            mesh: &translated,
        }],
        &small,
    )
    .unwrap();
    let c = render_compare(
        &[CompareItem {
            name: "c",
            mesh: &translated,
        }],
        &moved,
    )
    .unwrap();

    let tile = |res: &meshview::CompareResult| {
        let mut out = Vec::with_capacity(160 * 160);
        for dy in 0..160usize {
            for dx in 0..160usize {
                out.push(*res.sheet.get_pixel((12 + dx) as u32, (38 + dy) as u32));
            }
        }
        out
    };
    let ta = tile(&a);
    let tb = tile(&b);
    let tc = tile(&c);
    let nonbg = |tile: &Vec<image::Rgba<u8>>| tile.iter().filter(|p| p.0 != [0, 0, 0, 255]).count();
    // Panel A shows surface; panel B (same fixed region, mesh far away) is
    // empty — no auto-fit re-framed it.
    assert!(nonbg(&ta) > 5_000, "reference panel should show protein");
    assert!(nonbg(&tb) < 200, "shifted mesh must not be auto-fitted");
    // Recentering by the same translation reproduces the panel byte-exactly.
    assert_eq!(
        ta, tc,
        "shared camera at matched focus must be pixel-identical"
    );
}
