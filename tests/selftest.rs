//! All 13 checks from the Python `selftest.py`, ported 1:1.
//!
//! They assert properties (silhouette within tolerance of πr², no enclosed
//! uncovered pixel inside a closed surface, byte-identical re-render, …),
//! never stored images — goldens would bake in tomorrow's bug.

mod common;

use common::{enclosed_uncovered, uv_sphere};
use meshview::{Camera, RenderOptions, load_obj_from_str, render};

fn sphere_render(size: usize) -> meshview::Raster {
    let sphere = uv_sphere(5.0, 96, 48);
    let cam = Camera::new([0.0; 3], 30.0);
    render(
        &sphere,
        &cam,
        &RenderOptions {
            size,
            ..Default::default()
        },
    )
}

fn std_pop(data: &[f64]) -> f64 {
    let n = data.len() as f64;
    let mean = data.iter().sum::<f64>() / n;
    let var = data.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / n;
    var.sqrt()
}

// 1. Silhouette matches the analytic disc: proves projection + winding.
#[test]
fn silhouette_area_matches_analytic_disc() {
    let (size, zoom) = (400usize, 30.0f64);
    let out = sphere_render(size);
    let want = std::f64::consts::PI * (5.0 * zoom).powi(2);
    let got = out.covered_count() as f64;
    assert!(
        (got - want).abs() / want < 0.05,
        "silhouette {got} vs analytic {want}"
    );
}

// 2. THE REGRESSION TEST: no isolated uncovered pixel inside a closed surface.
//    This is the checkerboard failure mode.
#[test]
fn no_enclosed_uncovered_pixels() {
    let out = sphere_render(400);
    let holes = enclosed_uncovered(&out);
    assert_eq!(holes, 0, "{holes} speckles inside a closed sphere");
}

// 3. Shading must vary smoothly across the sphere, not in steps.
#[test]
fn interior_shading_varies_smoothly() {
    let size = 400usize;
    let out = sphere_render(size);
    let mut core = Vec::with_capacity(120 * 120);
    for y in size / 2 - 60..size / 2 + 60 {
        for x in size / 2 - 60..size / 2 + 60 {
            core.push(out.pixel(x, y) as f64);
        }
    }
    let s = std_pop(&core);
    assert!(s > 2.0 && s < 60.0, "interior std {s}");
}

// 4. (second assertion of test 3 in Python) no single-pixel vertical shading
//    discontinuity inside the core.
#[test]
fn no_single_pixel_shading_discontinuity() {
    let size = 400usize;
    let out = sphere_render(size);
    let w = 120usize;
    let y0 = size / 2 - 60;
    let x0 = size / 2 - 60;
    let mut max_step = 0.0f64;
    for dy in 0..w - 1 {
        for dx in 0..w {
            let a = out.pixel(x0 + dx, y0 + dy) as f64;
            let b = out.pixel(x0 + dx, y0 + dy + 1) as f64;
            max_step = max_step.max((a - b).abs());
        }
    }
    assert!(max_step < 40.0, "max row step {max_step}");
}

// 5. Coverage must not depend on normal orientation (real risk when
//    comparing meshes whose normals came from different passes): flipped
//    normals cover identically…
#[test]
fn flipped_normals_cover_identically() {
    let sphere = uv_sphere(5.0, 96, 48);
    let cam = Camera::new([0.0; 3], 30.0);
    let opts = RenderOptions {
        size: 400,
        ..Default::default()
    };
    let good = render(&sphere, &cam, &opts);
    let mut flipped = sphere.clone();
    for n in &mut flipped.normals {
        for c in n {
            *c = -*c;
        }
    }
    let bad = render(&flipped, &cam, &opts);
    for y in 0..400 {
        for x in 0..400 {
            assert_eq!(
                good.is_covered(x, y),
                bad.is_covered(x, y),
                "coverage differs at ({x},{y}) when normals are flipped"
            );
        }
    }
}

// 6. …and still has no enclosed uncovered pixels.
#[test]
fn enclosed_uncovered_zero_with_flipped_normals() {
    let sphere = uv_sphere(5.0, 96, 48);
    let cam = Camera::new([0.0; 3], 30.0);
    let opts = RenderOptions {
        size: 400,
        ..Default::default()
    };
    let mut flipped = sphere.clone();
    for n in &mut flipped.normals {
        for c in n {
            *c = -*c;
        }
    }
    let bad = render(&flipped, &cam, &opts);
    assert_eq!(enclosed_uncovered(&bad), 0);
}

// 7. Determinism: identical inputs, identical bytes.
#[test]
fn render_is_deterministic() {
    let sphere = uv_sphere(5.0, 96, 48);
    let cam = Camera::new([0.0; 3], 30.0);
    let opts = RenderOptions {
        size: 400,
        ..Default::default()
    };
    let a = render(&sphere, &cam, &opts);
    let b = render(&sphere, &cam, &opts);
    assert_eq!(a, b);
}

// Fixture for checks 8–11: explicit v//vn indices and a polygon face.
const FIXTURE: &str = "\
# fixture
v 0 0 0
v 1 0 0
v 1 1 0
v 0 1 0
v 0 1 1
vn 0 0 1
vn 0 -1 0
vn 1 0 0
f 1//1 2//1 3//1 4//1
f 4//2 5//2 1//2
";

// 8. Loader reads 5 vertices.
#[test]
fn loader_reads_five_vertices() {
    let m = load_obj_from_str("fixture.obj", FIXTURE).unwrap();
    assert_eq!(m.positions.len(), 5);
}

// 9. Loader fans a quad into triangles (2 from the quad + 1 triangle = 3).
#[test]
fn loader_fans_quad_into_triangles() {
    let m = load_obj_from_str("fixture.obj", FIXTURE).unwrap();
    assert_eq!(m.indices.len(), 3);
}

// 10. Loader keeps one normal per vertex.
#[test]
fn loader_keeps_one_normal_per_vertex() {
    let m = load_obj_from_str("fixture.obj", FIXTURE).unwrap();
    assert_eq!(m.normals.len(), 5);
}

// 11. Loader normals are unit length.
#[test]
fn loader_normals_are_unit_length() {
    let m = load_obj_from_str("fixture.obj", FIXTURE).unwrap();
    for n in &m.normals {
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        assert!((len - 1.0).abs() < 1e-9, "norm {len}");
    }
}

// 12. v//vn indices that differ from v indices must bind by reference, not by
//     arrival order. Flat-shaded exports do this constantly.
#[test]
fn vn_binds_by_index_not_arrival_order() {
    let perm = "\
v 0 0 0
v 1 0 0
v 0 1 0
vn 0 0 1
vn 0 1 0
vn 1 0 0
f 1//3 2//1 3//2
";
    let m = load_obj_from_str("perm.obj", perm).unwrap();
    let expected: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]];
    for (got, want) in m.normals.iter().zip(&expected) {
        for (g, w) in got.iter().zip(want) {
            assert!(
                (g - w).abs() < 1e-9,
                "normals {:?} vs {:?}",
                m.normals,
                expected
            );
        }
    }
}

// 13. vt-only corners must not be mistaken for normals: v/vt falls back to
//     geometric normals (the triangle lies in z = 0, so |nz| == 1).
#[test]
fn vt_corners_fall_back_to_geometric_normals() {
    let with_vt = "\
v 0 0 0
v 1 0 0
v 0 1 0
vt 0 0
f 1/1 2/1 3/1
";
    let m = load_obj_from_str("vt.obj", with_vt).unwrap();
    for n in &m.normals {
        assert!(
            n[2].abs() > 1.0 - 1e-9,
            "geometric fallback expected, got {:?}",
            m.normals[0]
        );
    }
}
