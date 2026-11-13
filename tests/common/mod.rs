//! Shared helpers for the ported self-tests and integration tests.

#![allow(dead_code)]

use meshview::Mesh;

/// Closed UV sphere with outward analytic normals and consistent winding —
/// the same construction as selftest.py's `uv_sphere`.
pub fn uv_sphere(radius: f64, nu: usize, nv: usize) -> Mesh {
    let count = (nv + 1) * nu;
    let mut positions = Vec::with_capacity(count);
    let mut normals = Vec::with_capacity(count);
    for j in 0..=nv {
        let theta = std::f64::consts::PI * j as f64 / nv as f64;
        let (st, ct) = (theta.sin(), theta.cos());
        for i in 0..nu {
            let phi = 2.0 * std::f64::consts::PI * i as f64 / nu as f64;
            let n = [st * phi.cos(), st * phi.sin(), ct];
            normals.push(n);
            positions.push([radius * n[0], radius * n[1], radius * n[2]]);
        }
    }
    let mut indices = Vec::with_capacity(nv * nu * 2);
    for j in 0..nv {
        for i in 0..nu {
            let a = j * nu + i;
            let b = j * nu + (i + 1) % nu;
            let c = (j + 1) * nu + i;
            let d = (j + 1) * nu + (i + 1) % nu;
            indices.push([a as u32, c as u32, b as u32]);
            indices.push([b as u32, c as u32, d as u32]);
        }
    }
    Mesh {
        positions,
        normals,
        indices,
    }
}

/// Count uncovered pixels the outside cannot reach — the speckle signal,
/// ported from selftest.py's `enclosed_uncovered`. Takes the renderer's
/// coverage mask, never the pixel values, and floods over 4-neighbours from
/// every border pixel.
pub fn enclosed_uncovered(covered: &meshview::Raster) -> usize {
    let size = covered.size();
    let is_bg = |x: usize, y: usize| !covered.is_covered(x, y);
    let mut seen = vec![false; size * size];
    let mut queue: std::collections::VecDeque<(usize, usize)> = std::collections::VecDeque::new();
    let seed = |x: usize,
                y: usize,
                seen: &mut Vec<bool>,
                q: &mut std::collections::VecDeque<(usize, usize)>| {
        if is_bg(x, y) && !seen[y * size + x] {
            seen[y * size + x] = true;
            q.push_back((x, y));
        }
    };
    for x in 0..size {
        seed(x, 0, &mut seen, &mut queue);
        seed(x, size - 1, &mut seen, &mut queue);
    }
    for y in 0..size {
        seed(0, y, &mut seen, &mut queue);
        seed(size - 1, y, &mut seen, &mut queue);
    }
    while let Some((x, y)) = queue.pop_front() {
        if x > 0 {
            seed(x - 1, y, &mut seen, &mut queue);
        }
        if x + 1 < size {
            seed(x + 1, y, &mut seen, &mut queue);
        }
        if y > 0 {
            seed(x, y - 1, &mut seen, &mut queue);
        }
        if y + 1 < size {
            seed(x, y + 1, &mut seen, &mut queue);
        }
    }
    (0..size)
        .flat_map(|y| (0..size).map(move |x| (x, y)))
        .filter(|(x, y)| is_bg(*x, *y) && !seen[y * size + x])
        .count()
}

pub fn sample(name: &str) -> String {
    format!("{}/assets/samples/{name}", env!("CARGO_MANIFEST_DIR"),)
}

/// True when a sample asset exists. The crates.io package omits the Stanford
/// scan models (research/educational license), so their tests skip there.
pub fn have(name: &str) -> bool {
    std::path::Path::new(&sample(name)).exists()
}
