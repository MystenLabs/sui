// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This test exists to make sure that the feature gating for all the code under `tracing`
/// remains fully connected such that if and only if we enable the feature here, the `tracing`
/// feature gets enabled anywhere.
///
/// If this test fails, check for the following.
///
/// Any crate that has code decorated with #[cfg(feature = "tracing")] needs to have
/// a feature declared in its Cargo.toml named `tracing`. If moving / refactoring code with
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
/// Note this crate will always have the feature enabled in testing due to the addition of
/// `sui = { path = ".", features = ["tracing"] }` to our dev-dependencies.

#[cfg(feature = "tracing")]
#[test]
fn test_macro_shows_feature_enabled() {
    move_vm_profiler::tracing_feature_disabled! {
        panic!("gas profile feature graph became disconnected");
    }
}

#[ignore]
#[cfg(feature = "tracing")]
#[tokio::test(flavor = "multi_thread")]
async fn test_profiler() {
    use std::fs;
    use sui_replay::ReplayToolCommand;
    use tempfile::tempdir;

    let output_dir = tempdir().unwrap();
    let profile_output = output_dir.path().join("profile.json");

    let testnet_url = "https://fullnode.testnet.sui.io:443".to_string();
    let tx_digest = "98KxVD14f2JgceKx4X27HaVAA2YGJ3Aazf6Y4tabpHa8".to_string();

    let cmd = ReplayToolCommand::ProfileTransaction {
        tx_digest,
        executor_version: None,
        protocol_version: None,
        profile_output: Some(profile_output),
        config_objects: None,
    };

    let command_result =
        sui_replay::execute_replay_command(Some(testnet_url), false, false, None, None, cmd).await;

    assert!(command_result.is_ok());

    // check that the profile was written
    let mut found = false;
    for entry in fs::read_dir(output_dir.into_path()).unwrap().flatten() {
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
}
