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
            println!("tasks:\n  model    regenerate {MODEL_PATH}");
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
        Some(other) => {
            eprintln!("unknown task: {other}");
            ExitCode::from(2)
        }
    }
}
