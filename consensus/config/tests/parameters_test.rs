// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test]
#[cfg(not(msim))]
fn parameters_snapshot_matches() {
    let parameters = consensus_config::Parameters::default();
    insta::assert_yaml_snapshot!("parameters", parameters)
}
