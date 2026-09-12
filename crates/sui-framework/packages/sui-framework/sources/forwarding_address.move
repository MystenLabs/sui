// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Registry and resolution for forwarding addresses.
///
/// The prototype layout is `[8-byte master ID][8 bytes of 0xfd][16-byte tag]`.
/// Integer fields use little-endian BCS encoding.
module sui::forwarding_address;

#[error(code = 0)]
const ENotSystemAddress: vector<u8> =
    b"Only the system can create the forwarding address registry.";

#[allow(unused_const)]
#[error(code = 1)]
const EForwardingAddressUnregistered: vector<u8> =
    b"The forwarding address master ID is not registered.";

/// Singleton shared object whose UID owns immutable master ID registrations.
public struct ForwardingAddressRegistry has key {
    id: UID,
}

/// Emitted when a balance deposit is redirected from a forwarding address to its master.
public struct ForwardingDeposit<phantom T> has copy, drop {
    forwarding_address: address,
    master: address,
    amount: u64,
    tag: u128,
}

/// Claim an unregistered `master_id` for `ctx.sender()`.
///
/// Master IDs are a first-come namespace; they have no external owner. Senders must construct
/// forwarding addresses only after confirming the intended master registered the ID.
///
/// Aborts if `master_id` is already registered.
public fun register(registry: &mut ForwardingAddressRegistry, master_id: u64, ctx: &TxContext) {
    sui::dynamic_field::add(&mut registry.id, master_id, ctx.sender());
}

/// Resolve `recipient` and emit an attribution event when it is a forwarding address.
public(package) fun resolve<T>(recipient: address, amount: u64): address {
    let (master, tag, forwarded) = resolve_impl(recipient);
    if (forwarded) {
        sui::event::emit(ForwardingDeposit<T> {
            forwarding_address: recipient,
            master,
            amount,
            tag,
        });
    };
    master
}

native fun resolve_impl(recipient: address): (address, u128, bool);

#[allow(unused_function)]
/// Create and share the singleton registry at genesis or protocol upgrade.
fun create(ctx: &TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);

    transfer::share_object(ForwardingAddressRegistry {
        id: object::forwarding_address_registry(),
    });
}
