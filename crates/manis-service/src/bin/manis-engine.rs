//! Headless Manis engine executable.

use std::process::ExitCode;

fn main() -> ExitCode {
    match manis_service::run_engine() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("manis-engine: {error}");
            ExitCode::FAILURE
        }
    }
}
