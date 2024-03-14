// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use camino::Utf8PathBuf;
use std::env;
use std::fs;
use telemetry_subscribers::TelemetryConfig;
use tracing::{debug, info};

#[test]
fn reload() {
    if std::env::var("RUST_LOG").is_ok() {
        println!("RUST_LOG is set, this test may fail to capture logs. Skipping ...");
        return;
    }

    let log_file_prefix = "out.log";
    let mut config = TelemetryConfig::new();
    config.log_file = Some(log_file_prefix.to_owned());
    config.panic_hook = false;

    let (guard, reload_handle) = config.init();

    info!("Should be able to see this");
    debug!("This won't be captured");
    reload_handle.update_log("debug").unwrap();
    debug!("Now you can see this!");

    debug!("{}", reload_handle.get_log().unwrap());

    drop(guard);

    let current_dir = Utf8PathBuf::from_path_buf(env::current_dir().unwrap()).unwrap();

    for entry in current_dir.read_dir_utf8().unwrap() {
        let entry = entry.unwrap();

        if entry.file_name().starts_with(log_file_prefix) {
            let logs = fs::read_to_string(entry.path()).unwrap();

            assert!(
                logs.contains("Should be able to see this"),
                "logs: {}",
                logs
            );
            assert!(!logs.contains("This won't be captured"), "logs: {}", logs);
            assert!(logs.contains("Now you can see this!"), "logs: {}", logs);

            fs::remove_file(entry.path()).unwrap();
            return;
        }
    }

    panic!("could not find log file");
}
