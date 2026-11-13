//! Development tasks. Run with `cargo xtask <task>`.

use std::path::PathBuf;
use std::process::ExitCode;

const MODEL_PATH: &str = "crates/ornatus-app/assets/default-laptop.glb";

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        None | Some("help") => {
            println!(
                "tasks:\n  model          regenerate {MODEL_PATH}\n  flawed         write target/flawed.glb, quads with different UV problems, to try the UV audit\n  stress [MB]    build a large .glb (default 100 MB) into target/stress.glb and load it\n  stress load    load target/stress.glb in a fresh process, report time and peak memory"
            );
            ExitCode::SUCCESS
        }
        Some("model") => {
            let path = workspace_root().join(MODEL_PATH);
            let bytes = ornatus_fixtures::laptop_glb();
            let written = path
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(&path, &bytes));
            match written {
                Ok(()) => {
                    println!("wrote {MODEL_PATH} ({} bytes)", bytes.len());
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("could not write {MODEL_PATH}: {error}");
                    ExitCode::FAILURE
                }
            }
        }
        Some("flawed") => {
            let path = workspace_root().join("target/flawed.glb");
            match std::fs::write(&path, ornatus_fixtures::flawed_glb()) {
                Ok(()) => {
                    println!("wrote target/flawed.glb");
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("could not write target/flawed.glb: {error}");
                    ExitCode::FAILURE
                }
            }
        }
        Some("stress") => stress(std::env::args().nth(2)),
        Some(other) => {
            eprintln!("unknown task: {other}");
            ExitCode::from(2)
        }
    }
}

/// Peak resident memory of this process in MB (Linux only).
fn peak_memory_mb() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmHWM:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb / 1024)
}

fn stress(arg: Option<String>) -> ExitCode {
    let path = workspace_root().join("target/stress.glb");
    let bytes = if arg.as_deref() == Some("load") {
        // Load an existing file in a fresh process, so the peak memory
        // number is not inflated by the generator.
        match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                eprintln!(
                    "could not read {}: {error}. Run `stress` first.",
                    path.display()
                );
                return ExitCode::FAILURE;
            }
        }
    } else {
        let megabytes: f64 = arg.and_then(|a| a.parse().ok()).unwrap_or(100.0);
        let side = ((megabytes * 1024.0 * 1024.0 / 56.0).sqrt()) as u32;
        let bytes = ornatus_fixtures::grid_glb(side);
        println!(
            "built grid {side}x{side}: {:.1} MB, {} vertices",
            bytes.len() as f64 / (1024.0 * 1024.0),
            u64::from(side) * u64::from(side)
        );
        if let Err(error) = std::fs::write(&path, &bytes) {
            eprintln!("could not write {}: {error}", path.display());
        }
        bytes
    };
    let before = peak_memory_mb();
    let started = std::time::Instant::now();
    let loaded = match ornatus_core::model::load_glb(&bytes) {
        Ok(loaded) => loaded,
        Err(error) => {
            eprintln!("load failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "load: {:.2?}, warnings: {:?}",
        started.elapsed(),
        loaded.warnings
    );
    drop(bytes);
    let started = std::time::Instant::now();
    let audit = ornatus_core::uv::audit(&loaded.model.meshes[0]);
    println!(
        "uv audit: {:.2?}, {} findings, coverage {:.0}%",
        started.elapsed(),
        audit.findings.len(),
        audit.coverage * 100.0
    );
    let mut model = loaded.model;
    let started = std::time::Instant::now();
    let scene = ornatus_core::pick::PickScene::new(&model);
    println!("bvh build: {:.2?}", started.elapsed());
    let started = std::time::Instant::now();
    let mut hits = 0;
    for i in 0..10_000 {
        let f = i as f32 / 10_000.0;
        let ray = ornatus_core::pick::Ray::new(
            glam::Vec3::new(f, 1.0, 1.0 - f),
            glam::Vec3::new(0.0, -1.0, 0.0),
        );
        hits += usize::from(scene.nearest(&model, &ray).is_some());
    }
    println!("10000 rays: {:.2?} ({hits} hits)", started.elapsed());
    let started = std::time::Instant::now();
    let scale = model.center_and_fit(2.0);
    println!(
        "center_and_fit: {:.2?} (scale {scale:?})",
        started.elapsed()
    );
    if let (Some(before), Some(after)) = (before, peak_memory_mb()) {
        println!("peak memory: {before} MB before load, {after} MB after");
    }
    ExitCode::SUCCESS
}
