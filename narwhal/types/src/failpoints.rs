use std::env::{self, VarError};
use tracing::{info, warn};

pub fn initialize_failpoints() {
    if fail::has_failpoints() {
        warn!("Failpoints are enabled");
        let failpoints = match env::var("FAILPOINTS") {
            Ok(s) => s,
            Err(VarError::NotPresent) => {
                // Panic okay here because failpoints feature is set and env var is required to utilize it.
                panic!("FAILPOINTS environment variable is not set")
            }
            Err(e) => {
                // Panic okay here because failpoints feature is set and valid failpoints are required.
                panic!("Invalid failpoints: {:?}", e)
            }
        };
        for mut cfg in failpoints.trim().split(';') {
            cfg = cfg.trim();
            if cfg.is_empty() {
                continue;
            }
            let (name, action) = partition(cfg, '=');
            match action {
                None => {
                    // Panic okay here because failpoints feature is set and valid failpoints are required.
                    panic!("Invalid failpoint: {:?}", cfg)
                }
                Some(action) => {
                    fail::cfg(name, action).expect("Failed to set actions for failpoints");
                }
            }
        }
    } else {
        info!("Failpoints are not enabled");
    }
}

fn partition(s: &str, pattern: char) -> (&str, Option<&str>) {
    let mut splits = s.splitn(2, pattern);
    (splits.next().unwrap(), splits.next())
}
