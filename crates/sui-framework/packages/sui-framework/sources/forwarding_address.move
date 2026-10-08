// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Registry of forwarding addresses.
///
/// Address layout: `[u48 master_id, little-endian][9 bytes of 0xfa][u8 variant][16 payload bytes]`.
/// The master ID, magic and variant positions are fixed; the variant alone decides what the
/// payload bytes mean. Variant 0 gives them no on-chain meaning.
///
/// Resolution happens outside Move: at the end of every transaction, the adapter reroutes funds
/// deposited to a forwarding address to the registered master and emits `ForwardingDeposit`.
/// Objects cannot be sent to a forwarding address.
module sui::forwarding_address;

use sui::dynamic_field;
use sui::event;

#[error(code = 0)]
const ENotSystemAddress: vector<u8> =
    b"Only the system can create the forwarding address registry.";

#[error(code = 3)]
const EMasterIdsExhausted: vector<u8> = b"All master IDs have been allocated.";

/// Master ids are 48 bits: the address layout stores them in six bytes.
const MAX_MASTER_ID: u64 = 0xFFFF_FFFF_FFFF;

/// Singleton shared object whose UID owns the master ID records and the allocation counter.
public struct ForwardingAddressRegistry has key {
    id: UID,
}

/// Ownership of a master ID, handed to the registrant. Keep it cold; it is what a later
/// rotation or pause will require.
public struct MasterCap has key, store {
    id: UID,
    master_id: u64,
}

// FIXME(forwarding-addresses): before this reaches production, add policy control over the
// mapping: pause/unpause an id (deposits abort while paused) and two-step rotation of the master
// with a delay, both gated on `MasterCap` or the current master. Without them a leaked master key
// keeps every published address paying the attacker. See design_docs/forwarding_addresses.md.
/// Dynamic field on the registry, keyed by master ID. The layout is frozen once published, so
/// lifecycle state goes into separate dynamic fields keyed by master ID.
public struct MasterRecord has store {
    master: address,
}

/// Dynamic field key for the next master ID counter (a `u64`; the last 48-bit id is allocatable).
public struct MasterIdCounter has copy, drop, store {}

/// Emitted by the adapter when funds deposited to a forwarding address are rerouted to its
/// master. `T` is the coin type of the `Balance` deposited.
#[allow(unused_field)]
public struct ForwardingDeposit<phantom T> has copy, drop {
    forwarding_address: address,
    master: address,
    amount: u64,
}

/// Emitted when a master ID is allocated.
public struct MasterRegistered has copy, drop {
    master_id: u64,
    master: address,
    cap_id: ID,
}

/// Allocate a fresh master ID for `ctx.sender()` and return the capability for it.
///
/// Charges a deliberately high gas fee, since every registration permanently grows the registry.
/// Aborts once every master ID has been allocated; IDs are never reused.
public fun register(registry: &mut ForwardingAddressRegistry, ctx: &mut TxContext): MasterCap {
    charge_registration_fee();
    let master_id = allocate_master_id(registry);
    let master = ctx.sender();
    dynamic_field::add(&mut registry.id, master_id, MasterRecord { master });
    let cap = MasterCap { id: object::new(ctx), master_id };
    event::emit(MasterRegistered { master_id, master, cap_id: object::id(&cap) });
    cap
}

public fun master_id(cap: &MasterCap): u64 {
    cap.master_id
}

native fun charge_registration_fee();

fun allocate_master_id(registry: &mut ForwardingAddressRegistry): u64 {
    if (!dynamic_field::exists(&registry.id, MasterIdCounter {})) {
        // Counter 0 is never allocated so that master ID 0 stays reserved.
        dynamic_field::add(&mut registry.id, MasterIdCounter {}, 1u64);
    };
    let next = dynamic_field::borrow_mut<MasterIdCounter, u64>(
        &mut registry.id,
        MasterIdCounter {},
    );
    assert!(*next <= MAX_MASTER_ID, EMasterIdsExhausted);
    let counter = *next;
    *next = *next + 1;
    mix_master_id(counter)
}

/// A permutation of the 48-bit id space built from xor-shifts and odd multiplications (the
/// lowbias32 construction widened to 48 bits), so distinct counters always give distinct IDs and
/// 0 is the only preimage of 0. IDs look mixed but are not secret; the counter is public and the
/// function is invertible. A shift of 24 on a 48-bit value is its own inverse.
fun mix_master_id(x: u64): u64 {
    let x = x ^ (x >> 24);
    let x = mul_mod_2_48(x, 0x9e3779b97f4b);
    let x = x ^ (x >> 24);
    let x = mul_mod_2_48(x, 0x5851f42d4c95);
    x ^ (x >> 24)
}

fun mul_mod_2_48(a: u64, b: u64): u64 {
    (((a as u128) * (b as u128)) & (MAX_MASTER_ID as u128)) as u64
}

#[allow(unused_function)]
/// Create and share the singleton registry at genesis or protocol upgrade.
fun create(ctx: &TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);
    transfer::share_object(ForwardingAddressRegistry {
        id: object::forwarding_address_registry(),
    });
}

/// Shares the registry at its system object ID, where deposit resolution looks it up. Must be
/// called by the system address, like `create`.
#[test_only]
public fun create_for_testing(ctx: &TxContext) {
    create(ctx);
}

#[test_only]
public fun registered_master_for_testing(
    registry: &ForwardingAddressRegistry,
    master_id: u64,
): Option<address> {
    if (dynamic_field::exists(&registry.id, master_id)) {
        option::some(dynamic_field::borrow<u64, MasterRecord>(&registry.id, master_id).master)
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
public fun mix_master_id_for_testing(x: u64): u64 {
    mix_master_id(x)
}

#[test_only]
public fun unmix_master_id_for_testing(x: u64): u64 {
    let x = x ^ (x >> 24);
    let x = mul_mod_2_48(x, 0x0b9ce3011ebd);
    let x = x ^ (x >> 24);
    let x = mul_mod_2_48(x, 0x393dee219263);
    x ^ (x >> 24)
}
