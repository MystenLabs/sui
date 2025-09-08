// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::registry;

use sui::object;

#[error(code = 0)]
const ENotSystemAddress: vector<u8> = b"Only the system can create the registry.";

public struct RegistryRoot has key {
    id: UID,
}

#[allow(unused_function)]
/// Create and share the singleton Registry -- this function is
/// called exactly once, during the upgrade epoch.
/// Only the system address (0x0) can create the registry.
fun create(ctx: &mut TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);

    transfer::share_object(RegistryRoot {
        id: object::sui_registry_root_object_id(),
    });
}
