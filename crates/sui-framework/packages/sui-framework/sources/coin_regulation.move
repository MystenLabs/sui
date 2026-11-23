// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Protocol-level address freezing, coin pausing, and seizing for regulated coins.
///
/// Seizing from an address is allowed only in an epoch after it was frozen. An address can be
/// unfrozen only in an epoch after the last `SeizeAuth` for it was destroyed.
module sui::coin_regulation;

use sui::coin::{Self, DenyCapV2};
use sui::deny_list::DenyList;
use sui::dynamic_field as df;
use sui::event;
use sui::vec_set::{Self, VecSet};

#[error(code = 0)]
const ENotSystemAddress: vector<u8> = b"Only the system can create the freeze list.";
#[error(code = 1)]
const EAlreadyFrozen: vector<u8> = b"The address is already frozen for this coin type.";
#[error(code = 2)]
const ENotFrozen: vector<u8> = b"The address is not frozen for this coin type.";
#[error(code = 3)]
const ESeizeAuthsOutstanding: vector<u8> =
    b"The address cannot be unfrozen while any SeizeAuth for it exists.";
#[error(code = 4)]
const EPermanentlyFrozen: vector<u8> = b"The address is permanently frozen for this coin type.";
#[error(code = 5)]
const EGlobalPauseNotAllowed: vector<u8> = b"The ComplianceCap does not allow global pause.";
#[error(code = 6)]
const EUnfreezeTooEarly: vector<u8> =
    b"The address cannot be unfrozen until the epoch after its last SeizeAuth was destroyed.";
#[error(code = 7)]
const ECoinTypeStateExists: vector<u8> = b"The coin type already has freeze list state.";
#[error(code = 8)]
const ETooManyFreezes: vector<u8> = b"Too many addresses frozen in this transaction.";

/// The maximum number of addresses newly frozen by `freeze_address` or `permanently_freeze_address`
/// in one transaction.
const MAX_FREEZES_PER_TRANSACTION: u64 = 8;

/// Singleton shared object holding the freeze and pause state of regulated coins.
public struct FreezeList has key {
    id: UID,
}

/// Authorizes freezing, seizing, and pausing coin type `T`. A cap migrated from a `DenyCapV2`
/// keeps its object ID.
public struct ComplianceCap<phantom T> has key, store {
    id: UID,
    /// Whether this cap may globally pause `T`.
    allow_global_pause: bool,
}

/// Authorizes seizing coin type `T` from `frozen_address`.
public struct SeizeAuth<phantom T> has key {
    id: UID,
    /// The address this authorizes seizing from.
    frozen_address: address,
    /// The epoch in which `frozen_address` was frozen.
    frozen_epoch: u64,
}

/// Dynamic field key on `FreezeList`. A missing key means `T` is not paused and has no frozen
/// addresses.
public struct CoinTypeKey<phantom T>() has copy, drop, store;

/// The value under `CoinTypeKey<T>`: the freeze and pause state of `T`.
public struct CoinTypeState has store {
    /// Whether every transaction touching `T` is rejected.
    paused: bool,
    /// The number of addresses frozen for `T`.
    num_frozen_addresses: u64,
}

/// Dynamic field key on `FreezeList`, holding the frozen address. Exists iff that address is
/// frozen for `T`.
public struct FrozenAddressKey<phantom T>(address) has copy, drop, store;

/// The value under `FrozenAddressKey<T>`: the state of one frozen address.
public struct FrozenAddressState has store {
    /// The IDs of the outstanding `SeizeAuth`s for this address.
    seize_auths: VecSet<ID>,
    /// Whether this address can never be unfrozen.
    permanent: bool,
    /// The epoch in which this address was frozen.
    frozen_epoch: u64,
    /// The epoch of the last `destroy_seize_auth` for this address, if any.
    unfreeze_after_epoch: Option<u64>,
}

/// Scratch key counting the addresses newly frozen in the current transaction.
public struct FreezeCountKey() has copy, drop;

/// Event: the address was frozen for `T`.
public struct AddressFrozen<phantom T>(address) has copy, drop;

/// Event: the address was unfrozen for `T`.
public struct AddressUnfrozen<phantom T>(address) has copy, drop;

/// Event: the address was permanently frozen for `T`.
public struct AddressPermanentlyFrozen<phantom T>(address) has copy, drop;

/// Event: a `SeizeAuth<T>` was issued for `frozen_address`.
public struct SeizeAuthIssued<phantom T> has copy, drop {
    /// The ID of the issued `SeizeAuth`.
    id: ID,
    /// The address it authorizes seizing from.
    frozen_address: address,
}

/// Event: a `SeizeAuth<T>` for `frozen_address` was destroyed.
public struct SeizeAuthDestroyed<phantom T> has copy, drop {
    /// The ID of the destroyed `SeizeAuth`.
    id: ID,
    /// The address it authorized seizing from.
    frozen_address: address,
}

/// Event: `T` was globally paused.
public struct CoinTypePaused<phantom T>() has copy, drop;

/// Event: the global pause of `T` was lifted.
public struct CoinTypeUnpaused<phantom T>() has copy, drop;

/// Event: the `DenyCapV2` for `T` was replaced by the `ComplianceCap` `cap`, keeping its ID.
public struct DenyCapMigrated<phantom T> has copy, drop {
    /// The ID of the `ComplianceCap`, which is also the ID of the replaced `DenyCapV2`.
    cap: ID,
}

// === Freezing ===

/// Freeze `addr` for `T`.
/// Aborts if `addr` is already frozen.
/// Aborts if the transaction already froze `MAX_FREEZES_PER_TRANSACTION` addresses.
public fun freeze_address<T>(
    list: &mut FreezeList,
    _cap: &mut ComplianceCap<T>,
    addr: address,
    ctx: &mut TxContext,
) {
    count_freeze(ctx);
    list.add_frozen_address<T>(addr, false, ctx);
}

/// Unfreeze `addr` for `T`. Aborts unless `addr` is frozen, not permanently, has no outstanding
/// `SeizeAuth`, and the current epoch is later than that of its last `SeizeAuth` destroy, if any.
public fun unfreeze_address<T>(
    list: &mut FreezeList,
    _cap: &mut ComplianceCap<T>,
    addr: address,
    ctx: &mut TxContext,
) {
    let key = FrozenAddressKey<T>(addr);
    assert!(df::exists(&list.id, key), ENotFrozen);
    let FrozenAddressState {
        seize_auths,
        permanent,
        frozen_epoch: _,
        unfreeze_after_epoch,
    } = df::remove(&mut list.id, key);
    assert!(seize_auths.is_empty(), ESeizeAuthsOutstanding);
    assert!(!permanent, EPermanentlyFrozen);
    unfreeze_after_epoch.do!(|epoch| assert!(ctx.epoch() > epoch, EUnfreezeTooEarly));
    let state: &mut CoinTypeState = df::borrow_mut(&mut list.id, CoinTypeKey<T>());
    state.num_frozen_addresses = state.num_frozen_addresses - 1;
    event::emit(AddressUnfrozen<T>(addr));
}

/// Freeze `addr` for `T` if it is not frozen, and prevent it from ever being unfrozen.
/// Aborts if `addr` is newly frozen and the transaction already froze
/// `MAX_FREEZES_PER_TRANSACTION` addresses.
public fun permanently_freeze_address<T>(
    list: &mut FreezeList,
    _cap: &mut ComplianceCap<T>,
    addr: address,
    ctx: &mut TxContext,
) {
    let key = FrozenAddressKey<T>(addr);
    if (df::exists(&list.id, key)) {
        df::borrow_mut<_, FrozenAddressState>(&mut list.id, key).permanent = true;
    } else {
        count_freeze(ctx);
        list.add_frozen_address<T>(addr, true, ctx);
    };
    event::emit(AddressPermanentlyFrozen<T>(addr));
}

// === Seizing ===

/// Issue a `SeizeAuth` for the frozen `addr`. Aborts if `addr` is not frozen.
public fun issue_seize_auth<T>(
    list: &mut FreezeList,
    _cap: &mut ComplianceCap<T>,
    addr: address,
    ctx: &mut TxContext,
): SeizeAuth<T> {
    let key = FrozenAddressKey<T>(addr);
    assert!(df::exists(&list.id, key), ENotFrozen);
    let state: &mut FrozenAddressState = df::borrow_mut(&mut list.id, key);
    let auth = SeizeAuth<T> {
        id: object::new(ctx),
        frozen_address: addr,
        frozen_epoch: state.frozen_epoch,
    };
    let id = object::id(&auth);
    state.seize_auths.insert(id);
    event::emit(SeizeAuthIssued<T> { id, frozen_address: addr });
    auth
}

/// Destroy `auth` and remove it from its address's outstanding `SeizeAuth`s.
public fun destroy_seize_auth<T>(list: &mut FreezeList, auth: SeizeAuth<T>, ctx: &mut TxContext) {
    let SeizeAuth { id, frozen_address, frozen_epoch: _ } = auth;
    let auth_id = id.to_inner();
    let state: &mut FrozenAddressState = df::borrow_mut(
        &mut list.id,
        FrozenAddressKey<T>(frozen_address),
    );
    state.seize_auths.remove(&auth_id);
    state.unfreeze_after_epoch = option::some(ctx.epoch());
    id.delete();
    event::emit(SeizeAuthDestroyed<T> { id: auth_id, frozen_address });
}

/// Transfer `auth` to `recipient`.
public fun transfer_seize_auth<T>(auth: SeizeAuth<T>, recipient: address) {
    transfer::transfer(auth, recipient)
}

// === Global pause ===

/// Globally pause `T`. Aborts if `cap` does not allow global pause.
public fun enable_global_pause<T>(
    list: &mut FreezeList,
    cap: &mut ComplianceCap<T>,
    _ctx: &mut TxContext,
) {
    assert!(cap.allow_global_pause, EGlobalPauseNotAllowed);
    list.coin_type_state_mut_or_add<T>().paused = true;
    event::emit(CoinTypePaused<T>());
}

/// Lift the global pause of `T`. Aborts if `cap` does not allow global pause.
public fun disable_global_pause<T>(
    list: &mut FreezeList,
    cap: &mut ComplianceCap<T>,
    _ctx: &mut TxContext,
) {
    assert!(cap.allow_global_pause, EGlobalPauseNotAllowed);
    df::get_mut_do!(&mut list.id, CoinTypeKey<T>(), |state: &mut CoinTypeState| {
        state.paused = false
    });
    event::emit(CoinTypeUnpaused<T>());
}

// === Accessors ===

/// Whether `addr` is frozen for `T`.
public fun is_frozen<T>(list: &FreezeList, addr: address): bool {
    df::exists(&list.id, FrozenAddressKey<T>(addr))
}

/// Whether `addr` is permanently frozen for `T`.
public fun is_permanently_frozen<T>(list: &FreezeList, addr: address): bool {
    df::get_fold!(&list.id, FrozenAddressKey<T>(addr), false, |state: &FrozenAddressState| {
        state.permanent
    })
}

/// Whether `T` is globally paused.
public fun is_global_pause_enabled<T>(list: &FreezeList): bool {
    df::get_fold!(&list.id, CoinTypeKey<T>(), false, |state: &CoinTypeState| state.paused)
}

/// The IDs of the outstanding `SeizeAuth`s for `addr`, empty if `addr` is not frozen.
public fun seize_auths<T>(list: &FreezeList, addr: address): vector<ID> {
    df::get_fold!(&list.id, FrozenAddressKey<T>(addr), vector[], |state: &FrozenAddressState| {
        *state.seize_auths.keys()
    })
}

/// Whether `cap` may globally pause `T`.
public fun allow_global_pause<T>(cap: &ComplianceCap<T>): bool {
    cap.allow_global_pause
}

/// The address `auth` authorizes seizing from.
public fun frozen_address<T>(auth: &SeizeAuth<T>): address {
    auth.frozen_address
}

/// The epoch in which `frozen_address` was frozen. Seizing is only allowed in a later epoch.
public fun frozen_epoch<T>(auth: &SeizeAuth<T>): u64 {
    auth.frozen_epoch
}

// === Package ===

/// A new `ComplianceCap` for `T`, for a coin created through the coin registry.
public(package) fun new_compliance_cap<T>(
    allow_global_pause: bool,
    ctx: &mut TxContext,
): ComplianceCap<T> {
    ComplianceCap { id: object::new(ctx), allow_global_pause }
}

/// Replace `cap` with a `ComplianceCap` that keeps its object ID, carrying over `T`'s legacy
/// next-epoch global pause and freezing each of `frozen_addresses` denied in the legacy next-epoch
/// list.
public(package) fun migrate_from_deny_list<T>(
    list: &mut FreezeList,
    deny_list: &mut DenyList,
    cap: DenyCapV2<T>,
    frozen_addresses: vector<address>,
    ctx: &TxContext,
): ComplianceCap<T> {
    let key = CoinTypeKey<T>();
    assert!(!df::exists(&list.id, key), ECoinTypeStateExists);
    let paused = coin::deny_list_v2_is_global_pause_enabled_next_epoch<T>(deny_list);
    df::add(&mut list.id, key, CoinTypeState { paused, num_frozen_addresses: 0 });
    let (id, allow_global_pause) = coin::destroy_deny_cap_v2(cap);
    let cap = ComplianceCap<T> { id, allow_global_pause };
    frozen_addresses.do!(|addr| {
        if (
            coin::deny_list_v2_contains_next_epoch<T>(deny_list, addr)
                && !list.is_frozen<T>(addr)
        ) list.add_frozen_address<T>(addr, false, ctx)
    });
    coin::deny_list_v2_mark_migrated<T>(deny_list);
    event::emit(DenyCapMigrated<T> { cap: object::id(&cap) });
    cap
}

// === Internals ===

#[allow(unused_function)]
/// Create and share the `FreezeList`. Only callable by genesis or change_epoch transactions.
fun create(ctx: &TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);

    transfer::share_object(FreezeList { id: object::freeze_list() });
}

/// Freeze `addr` for `T` and count it in `T`'s state. Aborts if `addr` is already frozen.
fun add_frozen_address<T>(list: &mut FreezeList, addr: address, permanent: bool, ctx: &TxContext) {
    let key = FrozenAddressKey<T>(addr);
    assert!(!df::exists(&list.id, key), EAlreadyFrozen);
    let state = list.coin_type_state_mut_or_add<T>();
    state.num_frozen_addresses = state.num_frozen_addresses + 1;
    df::add(
        &mut list.id,
        key,
        FrozenAddressState {
            seize_auths: vec_set::empty(),
            permanent,
            frozen_epoch: ctx.epoch(),
            unfreeze_after_epoch: option::none(),
        },
    );
    event::emit(AddressFrozen<T>(addr));
}

/// Count a newly frozen address against this transaction's limit.
fun count_freeze(ctx: &mut TxContext) {
    let count =
        ctx.scratch_internal_remove_opt!<FreezeCountKey, u64>(FreezeCountKey())
        .destroy_with_default(0);
    assert!(count < MAX_FREEZES_PER_TRANSACTION, ETooManyFreezes);
    ctx.scratch_internal_add!(FreezeCountKey(), count + 1);
}

/// The state of `T`, added if `T` has none.
fun coin_type_state_mut_or_add<T>(list: &mut FreezeList): &mut CoinTypeState {
    df::borrow_mut_or_add!(
        &mut list.id,
        CoinTypeKey<T>(),
        CoinTypeState { paused: false, num_frozen_addresses: 0 },
    )
}

// === Test-only ===

#[test_only]
/// Create and share the `FreezeList` in a test.
public fun create_for_testing(ctx: &TxContext) {
    create(ctx);
}

#[test_only]
/// Whether `T` has state in `list`.
public fun has_coin_type_state<T>(list: &FreezeList): bool {
    df::exists(&list.id, CoinTypeKey<T>())
}

#[test_only]
/// The number of addresses frozen for `T`.
public fun num_frozen_addresses<T>(list: &FreezeList): u64 {
    let key = CoinTypeKey<T>();
    if (!df::exists(&list.id, key)) return 0;
    df::borrow<_, CoinTypeState>(&list.id, key).num_frozen_addresses
}
