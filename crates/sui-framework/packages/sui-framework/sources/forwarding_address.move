// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Registry of forwarding addresses.
///
/// Address layout: `[u48 master_id, little-endian][9 bytes of 0xfa][u8 variant][16 payload bytes]`.
/// The master ID, magic and variant positions are fixed; the variant alone decides what the
/// payload bytes mean. Variant 0 gives them no on-chain meaning.
///
/// Resolution happens outside Move: at the end of every transaction, the adapter reroutes objects
/// and funds whose recipient is a forwarding address to the registered master, following the
/// chain when the master is itself a forwarding address, and emits the events declared here.
///
/// Two keys control an id. The master receives the funds and is expected to be hot. The
/// `MasterCap` is expected to be cold and is the only thing that can move funds elsewhere, through
/// a rotation that takes effect after the id's delay. Either key can pause the id, which makes
/// deposits fail until the cap unpauses it, so a leaked master key is contained by pausing and
/// then rotating, and a leaked cap is caught by the master cancelling the rotation during the
/// delay.
module sui::forwarding_address;

use sui::dynamic_field;
use sui::event;

#[error(code = 0)]
const ENotSystemAddress: vector<u8> =
    b"Only the system can create the forwarding address registry.";

#[error(code = 3)]
const EMasterIdsExhausted: vector<u8> = b"All master IDs have been allocated.";

#[error(code = 4)]
const EInvalidRotationDelay: vector<u8> =
    b"The rotation delay must be between 1 and 30 epochs and can only be increased.";

#[error(code = 5)]
const ENotMaster: vector<u8> = b"Only the current master can do this without the MasterCap.";

#[error(code = 6)]
const EForwardingAddressMaster: vector<u8> =
    b"A forwarding address cannot be the master of another forwarding address.";

#[error(code = 7)]
const ENoPendingRotation: vector<u8> = b"No rotation is pending for this master ID.";

#[error(code = 8)]
const ERotationNotDue: vector<u8> = b"The pending rotation's delay has not elapsed.";

/// Master ids are 48 bits: the address layout stores them in six bytes.
const MAX_MASTER_ID: u64 = 0xFFFF_FFFF_FFFF;

const MIN_ROTATION_DELAY_EPOCHS: u64 = 1;
const MAX_ROTATION_DELAY_EPOCHS: u64 = 30;

/// Where the forwarding magic sits in an address: bytes 6..15 are all `0xfa`.
const MAGIC_START: u64 = 6;
const MAGIC_END: u64 = 15;
const MAGIC_BYTE: u8 = 0xfa;

/// Singleton shared object whose UID owns the master ID records and the allocation counter.
public struct ForwardingAddressRegistry has key {
    id: UID,
}

/// Ownership of a master ID, handed to the registrant. Keep it cold: it is the only key that can
/// redirect funds (rotation) or resume deposits (unpause).
public struct MasterCap has key, store {
    id: UID,
    master_id: u64,
}

/// Dynamic field on the registry, keyed by master ID.
public struct MasterRecord has store {
    master: address,
    /// Deposits to the id fail while paused.
    paused: bool,
    pending: Option<PendingRotation>,
    /// Epochs between proposing a rotation and being able to finalize it.
    rotation_delay_epochs: u64,
}

public struct PendingRotation has copy, drop, store {
    new_master: address,
    effective_epoch: u64,
}

/// Dynamic field key for the next master ID counter (a `u64`; the last 48-bit id is allocatable).
public struct MasterIdCounter has copy, drop, store {}

/// Emitted by the adapter when funds deposited to a forwarding address are rerouted to its
/// master, once per hop. `T` is the coin type of the `Balance` deposited.
#[allow(unused_field)]
public struct ForwardingDeposit<phantom T> has copy, drop {
    forwarding_address: address,
    master: address,
    amount: u64,
}

/// Emitted by the adapter when an object sent to a forwarding address is rerouted to its master,
/// once per hop.
#[allow(unused_field)]
public struct ForwardingTransfer has copy, drop {
    forwarding_address: address,
    master: address,
    object_id: ID,
}

/// Emitted when a master ID is allocated.
public struct MasterRegistered has copy, drop {
    master_id: u64,
    master: address,
    cap_id: ID,
    rotation_delay_epochs: u64,
}

public struct Paused has copy, drop {
    master_id: u64,
}

public struct Unpaused has copy, drop {
    master_id: u64,
}

public struct RotationProposed has copy, drop {
    master_id: u64,
    new_master: address,
    effective_epoch: u64,
}

public struct RotationCancelled has copy, drop {
    master_id: u64,
}

public struct RotationFinalized has copy, drop {
    master_id: u64,
    master: address,
}

public struct RotationDelayIncreased has copy, drop {
    master_id: u64,
    rotation_delay_epochs: u64,
}

/// Allocate a fresh master ID for `ctx.sender()` and return the capability for it.
/// `rotation_delay_epochs` is how long a proposed rotation waits before it can be finalized; it
/// must be between 1 and 30 epochs and can later only be increased.
///
/// Charges a deliberately high gas fee, since every registration permanently grows the registry.
/// Aborts once every master ID has been allocated; IDs are never reused.
public fun register(
    registry: &mut ForwardingAddressRegistry,
    rotation_delay_epochs: u64,
    ctx: &mut TxContext,
): MasterCap {
    assert!(
        MIN_ROTATION_DELAY_EPOCHS <= rotation_delay_epochs &&
            rotation_delay_epochs <= MAX_ROTATION_DELAY_EPOCHS,
        EInvalidRotationDelay,
    );
    charge_registration_fee();
    let master_id = allocate_master_id(registry);
    let master = ctx.sender();
    dynamic_field::add(
        &mut registry.id,
        master_id,
        MasterRecord { master, paused: false, pending: option::none(), rotation_delay_epochs },
    );
    let cap = MasterCap { id: object::new(ctx), master_id };
    event::emit(MasterRegistered {
        master_id,
        master,
        cap_id: object::id(&cap),
        rotation_delay_epochs,
    });
    cap
}

public fun master_id(cap: &MasterCap): u64 {
    cap.master_id
}

// === Pause ===

/// Stop deposits to the id. Takes effect at the end of this transaction.
public fun pause(registry: &mut ForwardingAddressRegistry, cap: &MasterCap) {
    pause_impl(registry, cap.master_id);
}

/// The current master can pause without the cap, so a hot key can hit the brake.
public fun pause_by_master(
    registry: &mut ForwardingAddressRegistry,
    master_id: u64,
    ctx: &TxContext,
) {
    assert_master(registry, master_id, ctx);
    pause_impl(registry, master_id);
}

/// Only the cap can resume deposits, so a leaked master key cannot undo a pause.
public fun unpause(registry: &mut ForwardingAddressRegistry, cap: &MasterCap) {
    let record = record_mut(registry, cap.master_id);
    if (record.paused) {
        record.paused = false;
        event::emit(Unpaused { master_id: cap.master_id });
    }
}

fun pause_impl(registry: &mut ForwardingAddressRegistry, master_id: u64) {
    let record = record_mut(registry, master_id);
    if (!record.paused) {
        record.paused = true;
        event::emit(Paused { master_id });
    }
}

// === Rotation ===

/// Propose a new master. It takes effect once `finalize_rotation` is called in an epoch at least
/// `rotation_delay_epochs` after this one. Deposits keep going to the current master meanwhile;
/// pause first if they should stop. A new proposal replaces a pending one.
public fun propose_rotation(
    registry: &mut ForwardingAddressRegistry,
    cap: &MasterCap,
    new_master: address,
    ctx: &TxContext,
) {
    assert!(!is_forwarding_address(new_master), EForwardingAddressMaster);
    let record = record_mut(registry, cap.master_id);
    let effective_epoch = ctx.epoch() + record.rotation_delay_epochs;
    record.pending = option::some(PendingRotation { new_master, effective_epoch });
    event::emit(RotationProposed { master_id: cap.master_id, new_master, effective_epoch });
}

public fun cancel_rotation(registry: &mut ForwardingAddressRegistry, cap: &MasterCap) {
    cancel_rotation_impl(registry, cap.master_id);
}

/// The current master can cancel without the cap, which is what stops a rotation proposed with a
/// stolen cap.
public fun cancel_rotation_by_master(
    registry: &mut ForwardingAddressRegistry,
    master_id: u64,
    ctx: &TxContext,
) {
    assert_master(registry, master_id, ctx);
    cancel_rotation_impl(registry, master_id);
}

fun cancel_rotation_impl(registry: &mut ForwardingAddressRegistry, master_id: u64) {
    let record = record_mut(registry, master_id);
    assert!(record.pending.is_some(), ENoPendingRotation);
    record.pending = option::none();
    event::emit(RotationCancelled { master_id });
}

/// Anyone can finalize a due rotation, so the cap can stay cold once it has proposed.
public fun finalize_rotation(
    registry: &mut ForwardingAddressRegistry,
    master_id: u64,
    ctx: &TxContext,
) {
    let record = record_mut(registry, master_id);
    assert!(record.pending.is_some(), ENoPendingRotation);
    let pending = record.pending.extract();
    assert!(ctx.epoch() >= pending.effective_epoch, ERotationNotDue);
    record.master = pending.new_master;
    event::emit(RotationFinalized { master_id, master: pending.new_master });
}

/// Lengthen the rotation delay. Shortening it is not allowed: a stolen cap could otherwise
/// shorten it and rotate before the master notices.
public fun increase_rotation_delay(
    registry: &mut ForwardingAddressRegistry,
    cap: &MasterCap,
    rotation_delay_epochs: u64,
) {
    let record = record_mut(registry, cap.master_id);
    assert!(
        record.rotation_delay_epochs < rotation_delay_epochs &&
            rotation_delay_epochs <= MAX_ROTATION_DELAY_EPOCHS,
        EInvalidRotationDelay,
    );
    record.rotation_delay_epochs = rotation_delay_epochs;
    event::emit(RotationDelayIncreased { master_id: cap.master_id, rotation_delay_epochs });
}

fun assert_master(registry: &ForwardingAddressRegistry, master_id: u64, ctx: &TxContext) {
    let record = dynamic_field::borrow<u64, MasterRecord>(&registry.id, master_id);
    assert!(record.master == ctx.sender(), ENotMaster);
}

fun record_mut(registry: &mut ForwardingAddressRegistry, master_id: u64): &mut MasterRecord {
    dynamic_field::borrow_mut<u64, MasterRecord>(&mut registry.id, master_id)
}

/// Whether `addr` carries the forwarding magic, whatever its id or variant.
public fun is_forwarding_address(addr: address): bool {
    let bytes = addr.to_bytes();
    let mut i = MAGIC_START;
    while (i < MAGIC_END) {
        if (bytes[i] != MAGIC_BYTE) return false;
        i = i + 1;
    };
    true
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
public fun is_paused_for_testing(registry: &ForwardingAddressRegistry, master_id: u64): bool {
    dynamic_field::borrow<u64, MasterRecord>(&registry.id, master_id).paused
}

#[test_only]
public fun pending_rotation_for_testing(
    registry: &ForwardingAddressRegistry,
    master_id: u64,
): Option<address> {
    let record = dynamic_field::borrow<u64, MasterRecord>(&registry.id, master_id);
    record.pending.map!(|pending| pending.new_master)
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
