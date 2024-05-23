// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_causality_log::event;
use telemetry_subscribers;

fn main() {
    let _guard = telemetry_subscribers::TelemetryConfig::default()
        .with_log_level("debug")
        .with_log_file("/tmp/sui-causality-log.log")
        .init();

    event!("test_event" {
        source = "local",
        tag = 42
    }
        caused_by "no_such_event" {
            source = "local",
            tag = 42,
        }
    );

    event!(
        "recieved_test_event" {
            source = "local",
            tag = 42,
        }
        caused_by "test_event" {
            source = "local",
            tag = 42,
        }
    );

    event!(
        "processed_test_event" {
            source = "local",
            tag = 42
        }
        caused_by "recieved_test_event" {
            source = "local",
            tag = 42,
        }
    );

    std::thread::sleep(std::time::Duration::from_millis(100));

    // find all files starting with "sui-causality-log.log" in /tmp
    // and pick the most recent
    let files = std::fs::read_dir("/tmp").unwrap();
    let mut log_file: Option<(std::fs::DirEntry, std::time::SystemTime)> = None;
    for file in files {
        let file = file.unwrap();
        let file_name = file.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        if file_name.starts_with("sui-causality-log.log") {
            let timestamp = file.metadata().unwrap().modified().unwrap();
            if log_file.is_none() || timestamp > log_file.as_ref().unwrap().1 {
                log_file = Some((file, timestamp));
            }
        }
    }

    // open log file as BufReader

    let log_file = std::fs::File::open(log_file.unwrap().0.path()).unwrap();
    // open output file
    let output = std::fs::File::create("/tmp/sui-causality-log.output").unwrap();
    sui_causality_log::analyze_log(log_file, output, None);
}
