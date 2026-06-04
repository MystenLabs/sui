// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Registry for forwarding addresses.
///
/// A forwarding address is an off-chain-derived alias that forwards deposits to a
/// registered master address at resolution time. This module currently defines only the
/// singleton registry object; registration and resolution APIs are added in later steps.
module sui::forwarding_address;

#[error(code = 0)]
const ENotSystemAddress: vector<u8> =
    b"Only the system can create the forwarding address registry.";

/// Singleton shared object which will hold forwarding address registrations.
public struct ForwardingAddressRegistry has key {
    id: UID,
}

#[allow(unused_function)]
/// Create and share the `ForwardingAddressRegistry` object. This function is called exactly
/// once, when the registry object is first created. Can only be called by genesis or
/// change_epoch transactions.
fun create(ctx: &TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);

    let self = ForwardingAddressRegistry {
        id: object::forwarding_address_registry(),
    };
    transfer::share_object(self);
}
