// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(unused_field)]
module sui::object {
    /// A test version of the UID type to allow us to have types with
    /// `key` in these test packages. It has a different structure to
    /// the real UID, but that is not relevant.
    struct UID has store {
        id: address,
    }
}
