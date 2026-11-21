//! `fw-depot`: depot_tools in Rust (run by `fw depot`). See `fw-depot help`.

use std::process::ExitCode;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match fw_depot::cli::run(&arguments) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("fw-depot: {error}");
            ExitCode::FAILURE
        }
    }
}
