// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// # Root Utils
///
/// Utility module in the root package — same module name as `dep_addr::utils`.
module root_addr::utils {
    use dep_addr::utils as dep_utils;

    /// Calls into the dependency's identically-named module.
    public fun combined(): u64 {
        dep_utils::dep_helper() + 1
    }
}
