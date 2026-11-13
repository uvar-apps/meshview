//! CLI-level checks: flags are wired, two separate runs of the binary emit
//! byte-identical PNGs, and --mask drops a per-tile coverage PNG.

mod common;

use common::sample;
use std::path::PathBuf;
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_meshview")
}

fn tmp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("meshview_cli_{}_{}", std::process::id(), name))
}

#[test]
fn cli_renders_mask_and_is_deterministic_across_processes() {
    let obj = sample("1crn_ses_0.5A.obj");
    let out1 = tmp("cli1.png");
    let out2 = tmp("cli2.png");
    let m1 = tmp("mask.png");

    let base = |out: &PathBuf, mask: bool| {
        let mut cmd = Command::new(bin());
        cmd.arg(&obj)
            .arg("-o")
            .arg(out)
            .args(["--zoom", "90", "--size", "120", "--aa", "1"]);
        if mask {
            cmd.arg("--mask").arg(&m1);
        }
        cmd
    };

    let r1 = base(&out1, true).output().expect("binary runs");
    assert!(
        r1.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&r1.stderr)
    );
    let stdout1 = String::from_utf8(r1.stdout).unwrap();
    assert!(
        stdout1.contains(&format!("wrote {}", out1.display())),
        "{stdout1}"
    );
    assert!(
        stdout1.contains(&format!("wrote {}", m1.display())),
        "--mask path reported: {stdout1}"
    );

    let r2 = base(&out2, false)
        .output()
        .expect("binary runs a second time");
    assert!(r2.status.success());
    assert_eq!(
        std::fs::read(&out1).unwrap(),
        std::fs::read(&out2).unwrap(),
        "two CLI runs of identical inputs must produce byte-identical PNGs"
    );

    // The mask PNG loaded back is 0/255 and shaped like the surface.
    let mask = image::open(&m1).unwrap().to_luma8();
    let histogram: Vec<u32> = {
        let mut h = vec![0u32; 256];
        for p in mask.pixels() {
            h[p.0[0] as usize] += 1;
        }
        h
    };
    assert_eq!(
        histogram
            .iter()
            .enumerate()
            .filter(|(v, c)| **c > 0 && *v != 0 && *v != 255)
            .count(),
        0,
        "mask must be strictly 0/255"
    );
    assert!(histogram[255] > 1_000, "mask should show covered surface");

    // --focus rejects malformed input with a message, nonzero exit.
    let bad = Command::new(bin())
        .arg(&obj)
        .arg("-o")
        .arg(tmp("never.png"))
        .args(["--focus", "1,2", "--size", "8"])
        .output()
        .unwrap();
    assert!(!bad.status.success());
    assert!(
        String::from_utf8_lossy(&bad.stderr).contains("--focus expects x,y,z"),
        "clear focus error"
    );

    for f in [&out1, &out2, &m1] {
        std::fs::remove_file(f).ok();
    }
}
