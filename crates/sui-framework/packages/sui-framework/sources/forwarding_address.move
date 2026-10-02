// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Registry and resolution for forwarding addresses.
///
/// Address layout: `[u32 master_id, little-endian][10 bytes of 0xfa][u8 variant][17 payload bytes]`.
/// The master ID, magic and variant positions are fixed; the variant alone decides what the
/// payload bytes mean. Variant 0 gives them no on-chain meaning.
module sui::forwarding_address;

use sui::dynamic_field;
use sui::event;

#[error(code = 0)]
const ENotSystemAddress: vector<u8> =
    b"Only the system can create the forwarding address registry.";

#[allow(unused_const)]
#[error(code = 1)]
const EForwardingAddressUnregistered: vector<u8> =
    b"The forwarding address master ID is not registered.";

#[allow(unused_const)]
#[error(code = 2)]
const EForwardingAddressVariantUnsupported: vector<u8> =
    b"The forwarding address variant is not supported by this protocol version.";

#[error(code = 3)]
const EMasterIdsExhausted: vector<u8> = b"All master IDs have been allocated.";

const MAX_MASTER_ID: u64 = 0xFFFF_FFFF;

/// Singleton shared object whose UID owns the master ID records and the allocation counter.
public struct ForwardingAddressRegistry has key {
    id: UID,
}

/// Ownership of a master ID, handed to the registrant. Keep it cold; it is what a later
/// rotation or pause will require.
public struct MasterCap has key, store {
    id: UID,
    master_id: u32,
}

/// Dynamic field on the registry, keyed by master ID.
public struct MasterRecord has store {
    master: address,
}

/// Dynamic field key for the next master ID counter (a `u64`, so the last `u32` is allocatable).
public struct MasterIdCounter has copy, drop, store {}

/// Emitted when a balance deposit is redirected from a forwarding address to its master.
public struct ForwardingDeposit<phantom T> has copy, drop {
    forwarding_address: address,
    master: address,
    amount: u64,
}

/// Emitted when a master ID is allocated.
public struct MasterRegistered has copy, drop {
    master_id: u32,
    master: address,
    cap_id: ID,
}

/// Allocate a fresh master ID for `ctx.sender()` and return the capability for it.
///
/// Aborts once every master ID has been allocated; IDs are never reused.
public fun register(registry: &mut ForwardingAddressRegistry, ctx: &mut TxContext): MasterCap {
    let master_id = allocate_master_id(registry);
    let master = ctx.sender();
    dynamic_field::add(&mut registry.id, master_id, MasterRecord { master });
    let cap = MasterCap { id: object::new(ctx), master_id };
    event::emit(MasterRegistered { master_id, master, cap_id: object::id(&cap) });
    cap
}

public fun master_id(cap: &MasterCap): u32 {
    cap.master_id
}

/// Resolve `recipient` and emit an attribution event when it is a forwarding address.
public(package) fun resolve<T>(recipient: address, amount: u64): address {
    let (master, forwarded) = resolve_impl(recipient);
    if (forwarded) {
        event::emit(ForwardingDeposit<T> { forwarding_address: recipient, master, amount });
    };
    master
}

native fun resolve_impl(recipient: address): (address, bool);

fun allocate_master_id(registry: &mut ForwardingAddressRegistry): u32 {
    if (!dynamic_field::exists(&registry.id, MasterIdCounter {})) {
        // Counter 0 is never allocated so that master ID 0 stays reserved.
        dynamic_field::add(&mut registry.id, MasterIdCounter {}, 1u64);
    };
    let next = dynamic_field::borrow_mut<MasterIdCounter, u64>(
        &mut registry.id,
        MasterIdCounter {},
    );
    assert!(*next <= MAX_MASTER_ID, EMasterIdsExhausted);
    let counter = (*next as u32);
    *next = *next + 1;
    mix_master_id(counter)
}

/// lowbias32: a permutation of `u32` built from xor-shifts and odd multiplications, so distinct
/// counters always give distinct IDs and 0 is the only preimage of 0. IDs look mixed but are not
/// secret; the counter is public and the function is invertible.
fun mix_master_id(x: u32): u32 {
    let x = x ^ (x >> 16);
    let x = mul_mod_2_32(x, 0x7feb352d);
    let x = x ^ (x >> 15);
    let x = mul_mod_2_32(x, 0x846ca68b);
    x ^ (x >> 16)
}

fun mul_mod_2_32(a: u32, b: u32): u32 {
    (((a as u64) * (b as u64)) & MAX_MASTER_ID) as u32
}

#[allow(unused_function)]
/// Create and share the singleton registry at genesis or protocol upgrade.
fun create(ctx: &TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);
    transfer::share_object(ForwardingAddressRegistry {
        id: object::forwarding_address_registry(),
    });
}

#[test_only]
public fun share_for_testing(ctx: &mut TxContext) {
    transfer::share_object(ForwardingAddressRegistry { id: object::new(ctx) });
}

#[test_only]
public fun registered_master_for_testing(
    registry: &ForwardingAddressRegistry,
    master_id: u32,
): Option<address> {
    if (dynamic_field::exists(&registry.id, master_id)) {
        option::some(dynamic_field::borrow<u32, MasterRecord>(&registry.id, master_id).master)
    } else {
        option::none()
    }
}

#[test_only]
public fun set_next_counter_for_testing(registry: &mut ForwardingAddressRegistry, next: u64) {
    if (dynamic_field::exists(&registry.id, MasterIdCounter {})) {
        *dynamic_field::borrow_mut<MasterIdCounter, u64>(&mut registry.id, MasterIdCounter {}) =
            next;
    } else {
        dynamic_field::add(&mut registry.id, MasterIdCounter {}, next);
    }
}

#[test_only]
public fun mix_master_id_for_testing(x: u32): u32 {
    mix_master_id(x)
}

#[test_only]
public fun unmix_master_id_for_testing(x: u32): u32 {
    let x = x ^ (x >> 16);
    let x = mul_mod_2_32(x, 0x43021123);
    let x = x ^ (x >> 15) ^ (x >> 30);
    let x = mul_mod_2_32(x, 0x1d69e2a5);
    x ^ (x >> 16)
}
