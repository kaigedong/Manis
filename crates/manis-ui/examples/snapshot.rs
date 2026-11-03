#[cfg(target_os = "macos")]
#[path = "snapshot/driver.rs"]
mod driver;
#[cfg(target_os = "macos")]
#[path = "snapshot/fixtures.rs"]
mod fixtures;
#[cfg(target_os = "macos")]
#[path = "snapshot/scenarios.rs"]
mod scenarios;

#[cfg(all(test, target_os = "macos"))]
use scenarios::validate_live_output;

#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    driver::run()
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("native snapshot capture is currently available on macOS only");
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    #[test]
    fn live_output_path_must_resolve_inside_system_temp() -> Result<(), Box<dyn std::error::Error>>
    {
        let temp = std::env::temp_dir();
        let safe = temp.join(format!("manis-live-test-{}.png", std::process::id()));
        super::validate_live_output(&safe)?;

        let escaped = temp.join("..").join("manis-live-escaped.png");
        assert!(super::validate_live_output(&escaped).is_err());
        Ok(())
    }
}
