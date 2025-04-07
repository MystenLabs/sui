// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub(crate) fn apply_range_bounds(
    mut read_options: rocksdb::ReadOptions,
    lower_bound: Option<Vec<u8>>,
    upper_bound: Option<Vec<u8>>,
) -> rocksdb::ReadOptions {
    if let Some(lower_bound) = lower_bound {
        read_options.set_iterate_lower_bound(lower_bound);
    }
    if let Some(upper_bound) = upper_bound {
        read_options.set_iterate_upper_bound(upper_bound);
    }
    read_options
}
