#![allow(unused_imports)]

use super::*;
use std::fs;
use std::path::PathBuf;

#[path = "tests/benchmark_policy.rs"]
mod benchmark_policy;
#[path = "tests/controller_io.rs"]
mod controller_io;
#[path = "tests/live.rs"]
mod live;
#[path = "tests/preview.rs"]
mod preview;
#[path = "tests/runtime_build.rs"]
mod runtime_build;
#[path = "tests/store.rs"]
mod store;

fn test_temp_dir(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
    if path.exists() {
        fs::remove_dir_all(&path).expect("remove stale test directory");
    }
    fs::create_dir(&path).expect("create test directory");
    path
}
