# meshview

Headless, CPU-only rasterizer for **comparing triangle meshes side by side**.
Load OBJ files, render each under one identical orthographic camera into a
labelled contact-sheet PNG. No GPU, no window system, no interactivity, no
scene graph.

It is the Rust port of a debug tool written for
[molses](https://github.com/uvar-apps/molses)'s solvent-excluded-surface
meshes, where the whole point is that the same world-space region of several
meshes appears at the same scale and orientation — so a pixel difference means
a geometry difference **and nothing else**. It reads any OBJ. The Python
original renders a 1M-vertex mesh in about 5 s; the Rust port does it in
under 1 s (see [Performance](#performance)) with identical behavior, which is
the only reason two implementations are allowed to coexist.

```sh
cargo install meshview

meshview ref.obj ours.obj -o cmp.png --zoom 110 --focus 4.29,5.05,4.55 --aa 2 \
    --label reference --label ours
```

## Why the camera is the whole point

Every mesh in one run shares a single camera: `--focus x,y,z` (a world-space
point, put at the centre of every tile) and `--zoom` (pixels per world unit)
select the *same region* at the *same magnification* everywhere. That is what
makes a picture difference a geometry difference. Framing each mesh on its own
centroid or bounding box would silently vary magnification between panels —
exactly how a comparison starts lying to you. With no `--focus`, the first
mesh's centroid is used for all of them.

One subtlety inherited from the original, now documented: tiles are rendered
at `size × aa` and filtered down, so *on-screen* magnification is
`zoom / aa` pixels per unit. The same `--zoom` value frames the same world
region at any `--aa`; the antialias factor then buys smooth edges at the cost
of view extent, not resolution.

## Four honesty rules (and the bugs that earned them)

Each of these exists because breaking it produced a wrong conclusion once.
They are enforced by the ported self-tests, not by vibes.

1. **Nothing is back-face culled geometrically — visibility is the z-buffer,
   shading is two-sided.** An early version culled triangles with a geometric
   front/back test (the sign of the screen-space cross product). On a creased
   surface that test disagrees with the file's own stored smooth normals:
   edge-on triangles were removed and left a checkerboard of background speckles
   on *every* mesh — **including the untouched reference** it was being
   compared against. The speckles were briefly blamed on the meshes under
   test. Visibility now depends only on depth; orientation belongs to shading:
   the interpolated normal is turned toward the viewer before lighting, so an
   inverted normal renders dark instead of punching a hole. Test: a closed
   sphere (and its fully normal-flipped twin) can contain no uncovered pixel
   the border cannot reach.

2. **Normals bind by the face-corner's `vn` index, not by arrival order.**
   OBJ corners come as `v`, `v/vt`, `v//vn`, `v/vt/vn`. In `a/b/c`, `b` is a
   *texture* coordinate — reading it as a normal silently lights the mesh from
   the UVs. A `vn` is bound to the position it is referenced from (flat-shaded
   exports routinely index `vn` differently from `v`; `f 1//3 2//1 3//2` means
   position 1 carries normal-table entry 3). Vertices that end up with nothing
   bound get area-weighted geometric normals; everything is re-normalized
   because exported normals are not always unit length.

3. **Coverage is an explicit boolean mask, never inferred from pixels.** A
   surface facing the light shades to exactly 255 — the background value. A
   hole-check that reads pixel colours instead of the mask reported 3,208 fake
   holes on a flawless sphere. `render` returns `(image, coverage)`; every
   hole/speckle assertion goes through the mask, and `--mask PATH` writes it
   out as grayscale PNGs (strict 0/255) so you can diff surfaces honestly.

4. **One camera for the whole sheet.** Shared `focus` + `zoom` across all
   panels; no per-mesh auto-fit, ever. (Testing bonus: rendering the same
   input twice is byte-identical, so a diff is never a flake.)

## Library use

molses' dev flow calls it as a crate, not a CLI:

```rust
use meshview::{load_obj, Camera, RenderOptions, render, CompareItem, CompareOptions, render_compare};

let mesh = load_obj("mesh.obj")?;
let cam = Camera::new([4.29, 5.05, 4.55], 110.0);
let out = render(&mesh, &cam, &RenderOptions { size: 900, ..Default::default() });
let holes = /* flood-fill over out's mask — see tests/common/mod.rs */;
let sheet = render_compare(
    &[CompareItem { name: "ref", mesh: &mesh }, CompareItem { name: "ours", mesh: &other }],
    &CompareOptions {
        zoom: 110.0,
        focus: Some([4.29, 5.05, 4.55]),
        background: [0, 0, 0],        // or any colour…
        background_opacity: 0.0,      // …at any alpha (0 = transparent)
        ..Default::default()
    },
)?;
meshview::write_png_rgba("cmp.png", &sheet.sheet)?;  // deterministic bytes
```

## CLI

```
meshview [MESHES]... -o out.png [--zoom F] [--focus x,y,z] [--size N] [--aa N]
         [--per-row N] [--label S]... [--yaw F] [--pitch F] [--ambient F] [--mask PATH]
```

Defaults: `zoom 100`, `size 620`, `aa 2`, `ambient 0.30`, `per-row 0` (one
row), no focus (= first mesh's centroid), `yaw/pitch 0`. Deterministic PNG
bytes for identical inputs and options (fixed encoder settings, no metadata
or timestamps). `--mask PATH` writes each tile's coverage mask beside the
sheet (`PATH.png` for one mesh, `PATH_<k>.png` per tile for several).

The sample assets intentionally span incompatible unit scales, so the correct
per-mesh invocations live in [`examples/render_samples.sh`](examples/render_samples.sh);
`--zoom` values there account for the `zoom / aa` effective-magnification
rule above. Quick reference for `--aa 2`: bunny → `--zoom 10000`, dragon →
`--zoom 8`, whole crambin SES → `--zoom 50`, the crevice it was built for →
`--zoom 110 --focus 4.29,5.05,4.55`.

## Self-tests and the port's parity with Python

`selftest.py`'s 13 property checks are ported 1:1 (silhouette within 5% of
`πr²` — actually within 0.07% — no enclosed uncovered pixel in a closed
surface, smooth interior shading with no single-pixel step, flipped-normal
coverage identity, byte-identical re-render, loader fixtures: fan
triangulation, `vn`-index binding, `v/vt` geometric fallback). They assert
properties, never stored images; golden images bake in bugs while property
tests bake in truths. On top: exact load counts and normal paths for the
three samples, error-message contracts, `--focus` centring, shared-camera
no-auto-fit, mask PNG round-trip, and byte-identical sheets across processes.

Against the Python renderer on all three samples (aa 1, size 300): coverage
masks are **byte-for-byte identical in count** and covered-pixel MAD
averages ≤ 0.08/255 in the interior of agreement (≤ 0.45% of pixels differ by
more than 3/255 — all attributable to the known deviations below).

Known, deliberate deviations from Python:

* Python flat-shades faces that hit exactly one pixel inside their bbox using
  the average of the corner normals (un-normalized) — a speed hack for
  sub-pixel triangles. The port always interpolates and normalizes the corner
  normals, which is *more* correct and the main source of the ≤3/255 tail.
* Python silently wraps negative face indices (`-1` becomes the last vertex,
  NumPy-style). The port rejects them with a clear error.
* Python's `--label` list crashes with an `IndexError` if shorter than the
  mesh list; the port errors with an explicit message (surplus labels are
  ignored, as before) and malformed `--focus` gets the same rejection.

## Performance

`cargo build --release`, M-series laptop, single-threaded, wall-clock CLI
runs (load + raster + Lanczos downscale + PNG encode):

| mesh                        | settings                    | time         |
|-----------------------------|-----------------------------|--------------|
| dragon (249,882 faces)      | `--size 900 --aa 2 --zoom 20` (spec framing, silhouette overflows tile) | ~0.19 s |
| dragon (249,882 faces)      | `--size 900 --aa 2 --zoom 8` (full body on frame)                        | ~0.14 s |
| 1CRN SES (75,852 faces)     | `--size 900 --aa 2 --zoom 110` crevice focus                              | ~0.16 s |
| 1,001,112 v / 1,999,396 f proxy sphere (165 MB OBJ) | `--size 900 --aa 2 --zoom 110` | ~0.7 s |

Targets were < 1.5 s and < 2.5 s; both met single-threaded — scalar
scanline raster with conservative per-row half-plane spans plus an exact
per-pixel test, float64 throughout. `rayon` was deliberately *not* added: the
budget is met without risking nondeterministic z-fight resolution. The proxy
sphere (analytic normals, `f i//i` form, molecular-scale extents) stands in
for a 1M-vertex SES mesh, so the number includes parsing 165 MB of OBJ.

## Limits

Read these before drawing a conclusion from an image.

* **Orthographic only.** At these zooms that is the right choice, but it is
  not how a molecule looks in a real viewer.
* **Lambert with constant ambient.** No specular, no shadows, no ambient
  occlusion: crevices never look as deep as they are.
* **Grayscale.** Colour carries no information here; brightness is the signal.
* **Not a measurement tool.** It shows smoothness; it does not quantify it.
  Numbers come from molses' `roughness`/`normal_dev` fields or residual
  checks — a render can look right while metrics disagree, and the metrics
  decide.
* **Never compare shading statistics across mesh resolutions or zoom levels.**
  Per-edge normal angle scales roughly linearly with mesh spacing (and the
  zoom/aa rule changes apparent edge length in pixels). Angle per unit world
  length is the closest scale-free quantity, and even then geometric-normal
  roughness rises at fine spacings as more triangles become near-degenerate
  slivers. Same mesh spacing, same camera, same light — or state the
  difference explicitly.

## Assets and licenses

* Code: MIT OR Apache-2.0 (see `LICENSE-MIT`, `LICENSE-APACHE`), matching
  molses.
* `assets/samples/`: the Stanford bunny and XYZRGB dragon are scan data
  (Turk/Levoy) via [common-3d-test-models](https://github.com/alecjacobson/common-3d-test-models)
  and carry Stanford's research/educational licence — they are distributed in
  the **GitHub repository only**, not in the crates.io package.
  `1crn_ses_0.5A.obj` is generated by molses (MIT OR Apache-2.0) from RCSB PDB
  entry [1CRN](https://www.rcsb.org/structure/1CRN) (free for all uses) and
  does ship with the crate. Per-file provenance: `assets/samples/README.md`.

## Development

```sh
cargo test --release          # 13 ported checks + samples/CLI/properties
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

Run from a `git clone` all sample tests execute; from a crates.io checkout the
two Stanford-model tests skip with a message (those files are not republished
— see [Assets and licenses](#assets-and-licenses)).
