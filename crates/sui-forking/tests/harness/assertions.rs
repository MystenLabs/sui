// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use sui_forking::ForkingStatus;

pub fn assert_monotonic_status(before: &ForkingStatus, after: &ForkingStatus) {
    assert!(
        after.checkpoint >= before.checkpoint,
        "checkpoint regressed: before={}, after={}",
        before.checkpoint,
        after.checkpoint
    );
    assert!(
        after.epoch >= before.epoch,
        "epoch regressed: before={}, after={}",
        before.epoch,
        after.epoch
    );
}

pub fn assert_data_dir_contains_forking_namespace(data_dir: &Path) {
    let forking_root = data_dir.join("forking");
    assert!(
        forking_root.exists(),
        "expected '{}' to exist",
        forking_root.display()
    );

    let has_namespace_entry = std::fs::read_dir(&forking_root)
        .ok()
        .and_then(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .find(|path| path.is_dir())
        })
        .is_some();

    assert!(
        has_namespace_entry,
        "expected at least one namespace directory under '{}'",
        forking_root.display()
    );
}
