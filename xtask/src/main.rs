//! Development tasks. Run with `cargo xtask <task>`.
//!
//! The default laptop model and test fixtures are generated here from Phase 2.

fn main() {
    let task = std::env::args().nth(1);
    match task.as_deref() {
        None | Some("help") => println!("tasks: (none yet)"),
        Some(other) => {
            eprintln!("unknown task: {other}");
            std::process::exit(2);
        }
    }
}
