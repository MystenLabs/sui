// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{fs, path::PathBuf};
use sui_replay::ReplayToolCommand;

/// This test exists to make sure that the feature gating for all the code under `gas-profiler`
/// remains fully connected such that if and only if we enable the feature here, the `gas-profiler`
/// feature gets enabled anywhere.
///
/// If this test fails, check for the following.
///
/// Any crate that has code decorated with #[cfg(feature = "gas-profiler")] needs to have
/// a feature declared in its Cargo.toml named `gas-profiler`. If moving / refactoring code with
/// this decorator from a crate to a different crate, it is likely needed to copy over some of the
/// feature declaration defined in the original crate. Also ensure we do not include the feature in
/// any dependency of the dependencies section so that the feature won't get partially enabled as
/// default.
///
/// Each crate defines its own version of the feature with the same name. We can think of these
/// features as a tree structure where the root is defined here in this crate. Enabling the feature
/// here should continue to transitively enable all the other features in the other crates, and the
/// specific list of other crates' features that any given crate enables should include all features
/// defined in all the other crates that the decorated code in the current crate depends on.
///
/// Additionally, if the new crate is outside of the main workspace,the new crate may need to get
/// added to the list of traversal exclusions in `./config/hakari.toml` to prevent the feature
/// from automatically getting added to a dependency in the `workspace-hack/Cargo.toml`.
///
/// Note this crate will always have the feature enabled in testing due to the addition of
/// `sui = { path = ".", features = ["gas-profiler"] }` to our dev-dependencies.
#[cfg(feature = "gas-profiler")]
#[tokio::test(flavor = "multi_thread")]
async fn test_profiler() {
    let output_dir = "./profile_testing_temp";
    _ = fs::remove_dir_all(output_dir);
    fs::create_dir(output_dir).unwrap();
    let mut profile_output = PathBuf::from(output_dir);
    profile_output.push("profile.json");

    let testnet_url = "https://fullnode.testnet.sui.io:443".to_string();
    let tx_digest = "98KxVD14f2JgceKx4X27HaVAA2YGJ3Aazf6Y4tabpHa8".to_string();

    let cmd = ReplayToolCommand::ProfileTransaction {
        tx_digest,
        executor_version: None,
        protocol_version: None,
        profile_output: Some(profile_output),
    };

    let command_result =
        sui_replay::execute_replay_command(Some(testnet_url), false, false, None, cmd).await;

    assert!(command_result.is_ok());

    // check that the profile was written
    let mut found = false;
    for entry in fs::read_dir(output_dir).unwrap().flatten() {
        if entry
            .file_name()
            .into_string()
            .unwrap()
            .starts_with("profile")
        {
            found = true;
        }
    }
    assert!(found);
    fs::remove_dir_all(output_dir).unwrap();
}
