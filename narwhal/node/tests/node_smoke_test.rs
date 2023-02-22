// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use config::Export;
use std::{
    thread,
    time::{Duration, Instant},
};
use test_utils::{temp_dir, CommitteeFixture};

const TEST_DURATION: Duration = Duration::from_secs(3);

#[test]
fn test_primary_no_consensus() {
    let db_path = temp_dir().into_os_string().into_string().unwrap();
    let config_path = temp_dir().into_os_string().into_string().unwrap();
    let now = Instant::now();
    let duration = TEST_DURATION;

    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();
    let primary_keys_file_path = format!("{config_path}/smoke_test_primary_keys.json");
    fixture
        .authorities()
        .next()
        .unwrap()
        .keypair()
        .export(&primary_keys_file_path)
        .unwrap();
    let primary_network_keys_file_path =
        format!("{config_path}/smoke_test_network_primary_keys.json");
    fixture
        .authorities()
        .next()
        .unwrap()
        .network_keypair()
        .export(&primary_network_keys_file_path)
        .unwrap();
    let worker_keys_file_path = format!("{config_path}/smoke_test_worker_keys.json");
    fixture
        .authorities()
        .next()
        .unwrap()
        .worker(0)
        .keypair()
        .export(&worker_keys_file_path)
        .unwrap();

    let committee_file_path = format!("{config_path}/smoke_test_committee.json");
    committee.export(&committee_file_path).unwrap();

    let workers_file_path = format!("{config_path}/smoke_test_workers.json");
    worker_cache.export(&workers_file_path).unwrap();

    thread::sleep(Duration::from_millis(500)); // no idea why this is now needed :-/

    let mut child = std::process::Command::new("cargo")
        .current_dir("..")
        .args(["run", "--bin", "narwhal-node", "--"])
        .args([
            "run",
            "--committee",
            &committee_file_path,
            "--workers",
            &workers_file_path,
            "--primary-keys",
            &primary_keys_file_path,
            "--primary-network-keys",
            &primary_network_keys_file_path,
            "--worker-keys",
            &worker_keys_file_path,
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

    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();
    let primary_keys_file_path = format!("{config_path}/smoke_test_primary_keys.json");
    fixture
        .authorities()
        .next()
        .unwrap()
        .keypair()
        .export(&primary_keys_file_path)
        .unwrap();
    let primary_network_keys_file_path =
        format!("{config_path}/smoke_test_network_primary_keys.json");
    fixture
        .authorities()
        .next()
        .unwrap()
        .network_keypair()
        .export(&primary_network_keys_file_path)
        .unwrap();
    let worker_keys_file_path = format!("{config_path}/smoke_test_worker_keys.json");
    fixture
        .authorities()
        .next()
        .unwrap()
        .worker(0)
        .keypair()
        .export(&worker_keys_file_path)
        .unwrap();

    let committee_file_path = format!("{config_path}/smoke_test_committee.json");
    committee.export(&committee_file_path).unwrap();

    let workers_file_path = format!("{config_path}/smoke_test_workers.json");
    worker_cache.export(&workers_file_path).unwrap();

    thread::sleep(Duration::from_millis(500)); // no idea why this is now needed :-/

    let mut child = std::process::Command::new("cargo")
        .current_dir("..")
        .args(["run", "--bin", "narwhal-node", "--"])
        .args([
            "run",
            "--committee",
            &committee_file_path,
            "--workers",
            &workers_file_path,
            "--primary-keys",
            &primary_keys_file_path,
            "--primary-network-keys",
            &primary_network_keys_file_path,
            "--worker-keys",
            &worker_keys_file_path,
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
