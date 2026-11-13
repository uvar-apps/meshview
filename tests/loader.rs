//! Loader contract: sample files load with exact counts on the documented
//! paths (vn binding for 1CRN, geometric fallback for bunny/dragon), and the
//! three error cases fire with actionable messages.

mod common;

use common::sample;
use meshview::{ObjError, load_obj, load_obj_from_str};

#[test]
fn bunny_loads_with_exact_counts_and_unit_normals() {
    let m = load_obj(&sample("stanford-bunny.obj")).unwrap();
    assert_eq!(m.vertex_count(), 35_947);
    assert_eq!(m.face_count(), 69_451);
    // No vn in the file: every referenced vertex gets a geometric normal.
    let referenced = referenced(&m);
    let geo = geometric(&m);
    let mut min_len = f64::MAX;
    for i in referenced.iter().copied() {
        let n = m.normals[i];
        for k in 0..3 {
            assert!(
                (n[k] - geo[i][k]).abs() < 1e-9,
                "vertex {i}: renderer normal {:?} != normalized geometric {:?}",
                n,
                geo[i]
            );
        }
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        min_len = min_len.min(len);
    }
    assert!((min_len - 1.0).abs() < 1e-9, "min referenced |n| {min_len}");
}

#[test]
fn dragon_loads_with_exact_counts_and_unit_normals() {
    let m = load_obj(&sample("xyzrgb_dragon.obj")).unwrap();
    assert_eq!(m.vertex_count(), 125_066);
    assert_eq!(m.face_count(), 249_882);
    // Scan data contains folded/zero-area corners: the dragon's own geometric
    // normals are the reference, including the vertices that legitimately
    // accumulate to a zero vector (they stay zero exactly as the loader
    // guards them, never NaN).
    let geo = geometric(&m);
    let referenced = referenced(&m);
    let mut zeros = 0usize;
    for i in referenced.iter().copied() {
        let n = m.normals[i];
        for k in 0..3 {
            assert!(
                (n[k] - geo[i][k]).abs() < 1e-9,
                "dragon vertex {i}: {:?} vs geometric {:?}",
                n,
                geo[i]
            );
        }
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        assert!(
            (len - 1.0).abs() < 1e-9 || len < 1e-12,
            "dragon vertex {i} |n| {len} is neither unit nor zero"
        );
        if len == 0.0 {
            zeros += 1;
        }
    }
    eprintln!("dragon referenced vertices with zero geometric normal: {zeros}");
}

#[test]
fn crn_keeps_file_vn_normals_renormalized() {
    let path = sample("1crn_ses_0.5A.obj");
    let m = load_obj(&path).unwrap();
    assert_eq!(m.vertex_count(), 37_930);
    assert_eq!(m.face_count(), 75_852);
    // `f i//i` form over a watertight mesh: vertex i binds to vn line i.
    let mut vns: Vec<[f64; 3]> = Vec::new();
    for line in std::fs::read_to_string(&path).unwrap().lines() {
        if let Some(rest) = line.strip_prefix("vn ") {
            let mut it = rest.split_whitespace();
            vns.push([
                it.next().unwrap().parse().unwrap(),
                it.next().unwrap().parse().unwrap(),
                it.next().unwrap().parse().unwrap(),
            ]);
        }
    }
    assert_eq!(vns.len(), m.vertex_count());
    let mut worst_renorm = 0.0f64;
    for (i, (n, v)) in m.normals.iter().zip(&vns).enumerate() {
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        worst_renorm = worst_renorm.max((len - 1.0).abs());
        for k in 0..3 {
            let want = v[k] / len.max(1e-12);
            assert!(
                (n[k] - want).abs() < 1e-12,
                "vertex {i}: stored vn {:?} not kept (got {:?})",
                v,
                n
            );
        }
    }
    // The mesh ships nearly-unit normals; if this ever fails, the
    // re-normalization is doing real work worth calling out.
    eprintln!("worst |vn|-1 in file: {worst_renorm:.3e}");
}

#[test]
fn vertex_indices_and_corner_forms_all_parse() {
    let src = "\
v 0 0 0 1.0 0.2 0.3
vn 0 0 1 7
vt 0.5 0.5
f 1 1 1
f 1//1 1//1 1//1
f 1/1/1 1/1/1 1/1/1
f 1/1 1/1 1/1
";
    let m = load_obj_from_str("forms.obj", src).unwrap();
    assert_eq!(m.vertex_count(), 1);
    assert_eq!(m.face_count(), 4);
    let n = m.normals[0];
    assert!(
        (n[2] - 1.0).abs() < 1e-9,
        "vn bound from corner forms: {n:?}"
    );
}

#[test]
fn empty_file_with_comments_errors() {
    let err = load_obj_from_str("empty.obj", "# nothing\nvt 0 0\nvnotavertex\n").unwrap_err();
    assert!(matches!(err, ObjError::NoVertices(_)), "{err}");
    assert!(err.to_string().contains("empty.obj: no vertices"));
}

#[test]
fn malformed_number_names_the_line() {
    let src = "v 0 0 0\nv 1 x 0\n";
    let err = load_obj_from_str("bad.obj", src).unwrap_err();
    match err {
        ObjError::BadNumber { line, .. } => assert_eq!(line, 2),
        other => panic!("expected BadNumber, got {other}"),
    }
    let src = "v 0 0 0\nf 1 2 zzz\n";
    let err = load_obj_from_str("bad_f.obj", src).unwrap_err();
    match err {
        ObjError::BadNumber { line, .. } => assert_eq!(line, 2),
        other => panic!("expected BadNumber, got {other}"),
    }
}

#[test]
fn face_index_out_of_range_errors() {
    let src = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 9\n";
    let err = load_obj_from_str("oor.obj", src).unwrap_err();
    match err {
        ObjError::FaceIndexOutOfRange {
            line, value, count, ..
        } => {
            assert_eq!(line, 4);
            assert_eq!(value, 9);
            assert_eq!(count, 3);
        }
        other => panic!("expected FaceIndexOutOfRange, got {other}"),
    }
}

#[test]
fn negative_face_index_rejected_with_clear_error() {
    let src = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 -1\n";
    let err = load_obj_from_str("neg.obj", src).unwrap_err();
    match err {
        ObjError::NegativeFaceIndex { line, value, .. } => {
            assert_eq!(line, 4);
            assert_eq!(value, -1);
        }
        other => panic!("expected NegativeFaceIndex, got {other}"),
    }
}

/// Vertices actually referenced by at least one corner.
fn referenced(m: &meshview::Mesh) -> Vec<usize> {
    let mut seen = vec![false; m.vertex_count()];
    for tri in &m.indices {
        for &i in tri {
            seen[i as usize] = true;
        }
    }
    seen.iter()
        .enumerate()
        .filter(|(_, s)| **s)
        .map(|(i, _)| i)
        .collect()
}

/// Area-weighted geometric normals, recomputed independently in the test.
fn geometric(m: &meshview::Mesh) -> Vec<[f64; 3]> {
    let mut out = vec![[0.0f64; 3]; m.vertex_count()];
    for tri in &m.indices {
        let a = m.positions[tri[0] as usize];
        let b = m.positions[tri[1] as usize];
        let c = m.positions[tri[2] as usize];
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let cr = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        for &i in tri {
            for (slot, x) in out[i as usize].iter_mut().zip(&cr) {
                *slot += x;
            }
        }
    }
    for n in &mut out {
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if len > 1e-12 {
            for c in n {
                *c /= len;
            }
        }
    }
    out
}
