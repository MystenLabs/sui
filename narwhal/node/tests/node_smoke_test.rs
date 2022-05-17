// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use config::Export;
use std::time::{Duration, Instant};
use test_utils::{committee, keys, temp_dir};

const TEST_DURATION: Duration = Duration::from_secs(3);

#[test]
fn test_primary_no_consensus() {
    let db_path = temp_dir().into_os_string().into_string().unwrap();
    let config_path = temp_dir().into_os_string().into_string().unwrap();
    let now = Instant::now();
    let duration = TEST_DURATION;

    let keys = keys(None);
    let keys_file_path = format!("{config_path}/smoke_test_keys.json");
    keys[0].export(&keys_file_path).unwrap();

    let committee = committee(None);
    let committee_file_path = format!("{config_path}/smoke_test_committee.json");
    committee.export(&committee_file_path).unwrap();

    let mut child = std::process::Command::new("cargo")
        .current_dir("..")
        .args(&["run", "--bin", "node", "--"])
        .args(&[
            "run",
            "--committee",
            &committee_file_path,
            "--keys",
            &keys_file_path,
            "--store",
            &db_path,
            "primary",
            "--consensus-disabled",
        ])
        .spawn()
        .expect("failed to launch primary process w/o consensus");

    while now.elapsed() < duration {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    panic!("node panicked with: {:?}", child.stderr.take().unwrap());
                }
                assert!(status.success());
                break;
            }
            Ok(None) => continue,
            Err(e) => {
                panic!("error waiting for child process: {}", e);
            }
        }
    }
    let _ = child.kill();
}

#[test]
fn test_primary_with_consensus() {
    let db_path = temp_dir().into_os_string().into_string().unwrap();
    let config_path = temp_dir().into_os_string().into_string().unwrap();
    let now = Instant::now();
    let duration = TEST_DURATION;

    let keys = keys(None);
    let keys_file_path = format!("{config_path}/smoke_test_keys.json");
    keys[0].export(&keys_file_path).unwrap();

    let committee = committee(None);
    let committee_file_path = format!("{config_path}/smoke_test_committee.json");
    committee.export(&committee_file_path).unwrap();

    let mut child = std::process::Command::new("cargo")
        .current_dir("..")
        .args(&["run", "--bin", "node", "--"])
        .args(&[
            "run",
            "--committee",
            &committee_file_path,
            "--keys",
            &keys_file_path,
            "--store",
            &db_path,
            "primary",
            //no arg : default of with_consensus
        ])
        .spawn()
        .expect("failed to launch primary process w/o consensus");

    while now.elapsed() < duration {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    panic!("node panicked with: {:?}", child.stderr.take().unwrap());
                }
                assert!(status.success());
                break;
            }
            // This is expected to run indefinitely => will hit the timeout
            Ok(None) => continue,
            Err(e) => {
                panic!("error waiting for child process: {}", e);
            }
        }
    }
    let _ = child.kill();
}
