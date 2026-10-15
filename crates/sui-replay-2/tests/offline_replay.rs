// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    fs,
    path::Path,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[allow(dead_code)]
#[path = "../examples/offline_replay_corpus.rs"]
mod offline_replay_corpus;

#[test]
#[cfg_attr(msim, ignore)]
fn offline_replay_corpus_readme_is_in_sync() {
    let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/offline-replay");
    let previous_store_root = std::env::var_os("SUI_DATA_STORE");
    // FileSystemStore only accepts its cache root through the environment. This test executable
    // has a single table test, so restoring the previous value afterwards keeps it isolated.
    unsafe {
        std::env::set_var("SUI_DATA_STORE", &fixture_root);
    }
    let result = if std::env::var_os("SUI_REPLAY_2_UPDATE_OFFLINE_CORPUS").is_some() {
        offline_replay_corpus::sync(&fixture_root)
    } else {
        offline_replay_corpus::check(&fixture_root)
    };
    unsafe {
        if let Some(value) = previous_store_root {
            std::env::set_var("SUI_DATA_STORE", value);
        } else {
            std::env::remove_var("SUI_DATA_STORE");
        }
    }
    result.expect(
        "offline replay corpus README must match digests.txt and its local cache; set \
         SUI_REPLAY_2_UPDATE_OFFLINE_CORPUS=1 to regenerate the table",
    );
}

#[test]
#[cfg_attr(msim, ignore)]
fn replays_mainnet_corpus_from_filesystem_cache_without_executor_cache() {
    replay_mainnet_corpus_from_filesystem_cache(false);
}

#[test]
#[cfg_attr(msim, ignore)]
fn replays_mainnet_corpus_from_filesystem_cache_with_executor_cache() {
    replay_mainnet_corpus_from_filesystem_cache(true);
}

fn replay_mainnet_corpus_from_filesystem_cache(cache_executor: bool) {
    let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/offline-replay");
    let digests_path = fixture_root.join("digests.txt");
    let digest_contents =
        fs::read_to_string(&digests_path).expect("offline replay digest list should be readable");
    let digests = digest_contents
        .lines()
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    assert!(!digests.is_empty(), "offline replay digest list is empty");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after the Unix epoch")
        .as_nanos();
    let output_dir = std::env::temp_dir().join(format!(
        "sui-replay-2-offline-replay-{}-{nonce}",
        std::process::id()
    ));

    let mut command = Command::new(env!("CARGO_BIN_EXE_sui-replay-2"));
    command
        .args([
            "--digests-path",
            digests_path
                .to_str()
                .expect("fixture path should be valid UTF-8"),
            "--store-mode",
            "fs-only",
            "--output-dir",
            output_dir
                .to_str()
                .expect("temporary output path should be valid UTF-8"),
            "--terminate-early",
        ])
        // FileSystemStore appends `.sui_data_store` to this root. Supplying the environment to
        // the child process keeps this test isolated from the developer's cache and guarantees
        // that the `fs-only` invocation has no remote fallback.
        .env("SUI_DATA_STORE", &fixture_root);
    if cache_executor {
        command.arg("--cache-executor");
    }

    let output = command
        .output()
        .expect("offline replay binary should start");

    assert!(
        output.status.success(),
        "offline replay failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    for digest in digests {
        let transaction_dir = output_dir.join(digest);
        assert!(
            transaction_dir.join("transaction_effects.json").is_file(),
            "{digest} did not produce transaction effects"
        );
        assert!(
            !transaction_dir
                .join("forked_transaction_effects.json")
                .exists(),
            "{digest} replayed with effects different from its cached on-chain effects"
        );
    }

    fs::remove_dir_all(output_dir).expect("offline replay artifacts should be removable");
}
