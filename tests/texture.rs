//! Textured-mode tests on a fully self-generated fixture (quad + mtl +
//! checkerboard PNG, all authored here → unambiguously MIT/Apache like the
//! crate itself). The third honesty rule gets its textured form: color may
//! change, coverage and visibility may not — asserted byte-for-byte.

use image::{Rgb, RgbImage};
use meshview::texture::Texture;
use meshview::{Camera, RenderOptions, load_obj, load_textured, render, render_textured};

fn tmp_dir(tag: &str) -> std::path::PathBuf {
    let p = std::path::Path::new(&std::env::temp_dir()).join(format!(
        "meshview_tex_{}_{}",
        std::process::id(),
        tag
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// 2×2 texture: (0,0)=red, (1,0)=green, (0,1)=blue, (1,1)=white.
fn checker() -> Texture {
    Texture::from_rgb(2, 2, vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255])
}

#[test]
fn sampler_math_is_exact() {
    let t = checker();
    // Texel centers: u=0.25 lands on column 0, v=0.75 (flipped → row 0).
    assert_eq!(t.sample(0.25, 0.75), [255.0, 0.0, 0.0]);
    assert_eq!(t.sample(0.75, 0.75), [0.0, 255.0, 0.0]);
    assert_eq!(t.sample(0.25, 0.25), [0.0, 0.0, 255.0]);
    assert_eq!(t.sample(0.75, 0.25), [255.0, 255.0, 255.0]);
    // Horizontal midpoint between red and green texels.
    assert_eq!(t.sample(0.5, 0.75), [127.5, 127.5, 0.0]);
    // Wrap (repeat): +1.0 and whole negatives are invisible to the sample.
    assert_eq!(t.sample(1.25, 0.75), t.sample(0.25, 0.75));
    assert_eq!(t.sample(-0.75, 0.75), t.sample(0.25, 0.75));
}

fn quad_fixture() -> std::path::PathBuf {
    let dir = tmp_dir("quad");
    std::fs::write(
        dir.join("quad.obj"),
        "mtllib quad.mtl\n\
         v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\n\
         vt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\n\
         vn 0 0 1\n\
         usemtl check\nf 1/1/1 2/2/1 3/3/1 4/4/1\n",
    )
    .unwrap();
    // Windows-style map path spelling — must still resolve.
    std::fs::write(
        dir.join("quad.mtl"),
        "newmtl check\nKd 1 1 1\nmap_Kd .\\check.png\n",
    )
    .unwrap();
    let tex = RgbImage::from_pixel(2, 2, Rgb([0, 0, 0]));
    let mut tex = tex;
    tex.put_pixel(0, 0, Rgb([255, 0, 0]));
    tex.put_pixel(1, 0, Rgb([0, 255, 0]));
    tex.put_pixel(0, 1, Rgb([0, 0, 255]));
    tex.put_pixel(1, 1, Rgb([255, 255, 255]));
    tex.save(dir.join("check.png")).unwrap();
    dir
}

fn upright_flat_opts(size: usize) -> RenderOptions {
    RenderOptions {
        size,
        light: [0.0, 0.0, 1.0],
        ambient: 1.0, // shade is exactly 1 everywhere: what you see is the tint
    }
}

#[test]
fn textured_quad_maps_the_checker() {
    let dir = quad_fixture();
    let path = dir.join("quad.obj");
    let path_str = path.to_str().unwrap();
    let tm = load_textured(path_str).unwrap();
    assert_eq!(tm.textures.len(), 1, "the Windows-style path must resolve");
    assert_eq!(tm.tri_tex.iter().filter(|t| t.texture.is_some()).count(), 2);

    // Quad fills the tile: focus centre, zoom maps world 0..1 onto 0..64.
    let cam = Camera::new([0.5, 0.5, 0.0], 64.0);
    let r = render_textured(&tm, &cam, &upright_flat_opts(64));
    assert!(r.is_covered(4, 4), "quad corner covered");
    // World (u,v) of a pixel: u = (x+0.5)/64, v = 1 − (y+0.5)/64 (screen y-down).
    // Interior texel-centre pixel of blue quadrant: u≈0.25, v≈0.25.
    // u = 0.25 → x = 15.5 → px 15; v = 0.25 → y ≈ 47.5 → px 47.
    let p = r.pixel(15, 47);
    let _ = p; // gray stays the untextured level
    // Colorized view must carry the texture, not the gray ramp:
    let img = r.colorize([0, 0, 0], 255);
    let px = img.get_pixel(15, 47).0;
    assert!(
        px[2] as i32 - px[0] as i32 > 150 && px[1] < 40,
        "blue quadrant expected near (15,47), got {px:?}"
    );
    // Red quadrant (top-left on screen): u≈0.25, v≈0.75 → same x, y≈16.
    let px = img.get_pixel(15, 16).0;
    assert!(
        px[0] as i32 - px[2] as i32 > 150 && px[1] < 40,
        "red quadrant expected near (15,16), got {px:?}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn texturing_cannot_open_or_close_a_pixel() {
    let dir = quad_fixture();
    let path = dir.join("quad.obj");
    let path_str = path.to_str().unwrap();
    let tm = load_textured(path_str).unwrap();
    let cam = Camera::new([0.5, 0.5, 0.0], 64.0);
    let opts = upright_flat_opts(64);
    let plain = render(&tm.mesh, &cam, &opts);
    let tex = render_textured(&tm, &cam, &opts);
    for y in 0..64 {
        for x in 0..64 {
            assert_eq!(
                plain.is_covered(x, y),
                tex.is_covered(x, y),
                "coverage diverged at ({x},{y})"
            );
            assert_eq!(
                plain.pixel(x, y),
                tex.pixel(x, y),
                "gray diverged ({x},{y})"
            );
        }
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn untextured_mesh_through_textured_path_is_a_no_op() {
    // crambin has no vt and no material: its textured render's colors must
    // equal the gray ramp exactly.
    let path = format!(
        "{}/assets/samples/1crn_ses_0.5A.obj",
        env!("CARGO_MANIFEST_DIR")
    );
    let tm = load_textured(&path).unwrap();
    assert!(tm.textures.is_empty());
    let cam = Camera::new(tm.mesh.centroid(), 30.0);
    let opts = RenderOptions {
        size: 64,
        ..Default::default()
    };
    let plain = render(&tm.mesh, &cam, &opts).colorize([1, 2, 3], 255);
    let tex = render_textured(&tm, &cam, &opts).colorize([1, 2, 3], 255);
    assert_eq!(
        plain.as_raw(),
        tex.as_raw(),
        "material-less mesh must render identically through the textured path"
    );
}

#[test]
fn textured_sheet_is_deterministic_bytes() {
    let dir = quad_fixture();
    let path = dir.join("quad.obj");
    let path_str = path.to_str().unwrap();
    let tm = load_textured(path_str).unwrap();
    let items = vec![meshview::TexturedCompareItem {
        name: "quad",
        mesh: &tm,
    }];
    let opts = meshview::CompareOptions {
        zoom: 64.0,
        focus: Some([0.5, 0.5, 0.0]),
        size: 64,
        aa: 2,
        ..Default::default()
    };
    let mk = |name: &str| {
        let out = tmp_dir(name).join("sheet.png");
        let res = meshview::render_compare_textured(&items, &opts).unwrap();
        meshview::write_png_rgba(out.to_str().unwrap(), &res.sheet).unwrap();
        std::fs::read(&out).unwrap()
    };
    let a = mk("deta");
    let b = mk("detb");
    assert_eq!(a, b, "textured sheet bytes changed between runs");
    std::fs::remove_dir_all(&dir).ok();
}

/// Real-model smoke check, skipped unless a textured OBJ is pointed at:
/// `MESHVIEW_TEXTURED_MODEL=/path/model.obj cargo test --release --test texture -- --ignored`
#[test]
#[ignore = "requires MESHVIEW_TEXTURED_MODEL pointing at a local textured OBJ"]
fn external_model_renders_colored() {
    let Some(path) = std::env::var_os("MESHVIEW_TEXTURED_MODEL") else {
        panic!("set MESHVIEW_TEXTURED_MODEL to run");
    };
    let path = path.to_string_lossy().into_owned();
    let tm = load_textured(&path).unwrap();
    assert!(!tm.textures.is_empty(), "model should bring textures");
    let cam = Camera::new(tm.mesh.centroid(), 4.0);
    let r = render_textured(
        &tm,
        &cam,
        &RenderOptions {
            size: 240,
            ..Default::default()
        },
    );
    let img = r.colorize([0, 0, 0], 255);
    let chromatic = img
        .pixels()
        .filter(|p| p[3] == 255 && (p[0] as i32 - p[1] as i32).abs() > 16)
        .count();
    assert!(chromatic > 1000, "expected real colors, got {chromatic} px");
    let plain = load_obj(&path).unwrap();
    let _ = plain;
}
