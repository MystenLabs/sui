// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::test_scenario;

use sui::vec_map::VecMap;

#[allow(unused_const)]
/// the transaction failed when generating these effects. For example, a circular ownership
/// of objects was created
const ECouldNotGenerateEffects: u64 = 0;

/// Transaction ended without all shared and immutable objects being returned or with those
/// objects being transferred or wrapped
const EInvalidSharedOrImmutableUsage: u64 = 1;

/// Attempted to return an object to the inventory that was not previously removed from the
/// inventory during the current transaction. Can happen if the user attempts to call
/// `return_to_address` on a locally constructed object rather than one returned from a
/// `test_scenario` function such as `take_from_address`.
const ECantReturnObject: u64 = 2;

/// Attempted to retrieve an object of a particular type from the inventory, but it is empty.
/// Can happen if the user already transferred the object or a previous transaction failed to
/// transfer the object to the user.
const EEmptyInventory: u64 = 3;

/// Object of that ID was not found in that inventory. It was possibly already taken
const EObjectNotFound: u64 = 4;

#[allow(unused_const)]
/// Unable to allocate a receiving ticket for the object
const EUnableToAllocateReceivingTicket: u64 = 5;

#[allow(unused_const)]
/// A receiving ticket for the object was already allocated in the transaction
const EReceivingTicketAlreadyAllocated: u64 = 6;

#[allow(unused_const)]
/// Unable to deallocate the receiving ticket
const EUnableToDeallocateReceivingTicket: u64 = 7;

/// Utility for mocking a multi-transaction Sui execution in a single Move procedure.
/// A `Scenario` maintains a view of the global object pool built up by the execution.
/// These objects can be accessed via functions like `take_from_sender`, which gives the
/// transaction sender access to objects in (only) their inventory.
/// Example usage:
/// ```
/// let addr1: address = 0;
/// let addr2: address = 1;
/// // begin a test scenario in a context where addr1 is the sender
/// let scenario = &mut test_scenario::begin(addr1);
/// // addr1 sends an object to addr2
/// {
///     let some_object: SomeObject = ... // construct an object
///     transfer::public_transfer(some_object, copy addr2)
/// };
/// // end the first transaction and begin a new one where addr2 is the sender
/// // Starting a new transaction moves any objects transferred into their respective
/// // inventories. In other words, if you call `take_from_sender` before `next_tx`, `addr2`
/// // will not yet have `some_object`
/// test_scenario::next_tx(scenario, addr2);
/// {
///     // remove the SomeObject value from addr2's inventory
///     let obj = test_scenario::take_from_sender<SomeObject>(scenario);
///     // use it to test some function that needs this value
///     SomeObject::some_function(obj)
/// };
/// ... // more txes
/// test_scenario::end(scenario);
/// ```
public struct Scenario {
    txn_number: u64,
    ctx: TxContext,
}

/// Builder for a `TxContext` to use in a test scenario.
public struct TxContextBuilder has copy, drop {
    sender: address,
    epoch: u64,
    epoch_timestamp_ms: u64,
    ids_created: u64,
    // when rgp is set, the context is used to start the scenario (first time usage)
    // or with an epoch greater than the current epoch in the test scenario
    rgp: Option<u64>,
    gas_price: u64,
    gas_budget: u64,
    sponsor: Option<address>,
}

/// The effects of a transaction
public struct TransactionEffects has drop {
    /// The objects created this transaction
    created: vector<ID>,
    /// The objects written/modified this transaction
    written: vector<ID>,
    /// The objects deleted this transaction
    deleted: vector<ID>,
    /// The objects transferred to an account this transaction
    transferred_to_account: VecMap<ID, /* owner */ address>,
    /// The objects transferred to an object this transaction
    transferred_to_object: VecMap<ID, /* owner */ ID>,
    /// The objects shared this transaction
    shared: vector<ID>,
    /// The objects frozen this transaction
    frozen: vector<ID>,
    /// The number of user events emitted this transaction
    num_user_events: u64,
}

//
// `TxContextBuilder` api
//

/// Create a new `TxContextBuilder` with the given `sender` address.
/// Also provides default for all other fields.
public fun ctx_builder_from_sender(sender: address): TxContextBuilder {
    TxContextBuilder {
        sender,
        epoch: 0,
        epoch_timestamp_ms: 0,
        ids_created: 0,
        rgp: option::some(700),
        gas_price: 1_000,
        gas_budget: 100_000_000,
        sponsor: option::none(),
    }
}

/// Create a `TxContextBuilder` from an existing `TxContext` in a `Scenario`.
public fun ctx_builder(scenario: &Scenario): TxContextBuilder {
    ctx_builder_from_context(&scenario.ctx)
}

/// Create a `TxContextBuilder` from an existing `TxContext`.
public fun ctx_builder_from_context(ctx: &TxContext): TxContextBuilder {
    TxContextBuilder {
        sender: ctx.sender(),
        epoch: ctx.epoch(),
        epoch_timestamp_ms: ctx.epoch_timestamp_ms(),
        ids_created: ctx.ids_created(),
        rgp: option::some(ctx.reference_gas_price()),
        gas_price: ctx.gas_price(),
        gas_budget: ctx.gas_budget(),
        sponsor: ctx.sponsor(),
    }
}

/// Set the epoch for the `TxContextBuilder`.
public fun set_epoch(mut builder: TxContextBuilder, epoch: u64): TxContextBuilder {
    builder.epoch = epoch;
    builder
}

/// Set the epoch timestamp in milliseconds for the `TxContextBuilder`.
public fun set_epoch_timestamp(mut builder: TxContextBuilder, ms: u64): TxContextBuilder {
    builder.epoch_timestamp_ms = ms;
    builder
}

/// Set the ids created for the `TxContextBuilder`.
public fun set_ids_created(mut builder: TxContextBuilder, ids_created: u64): TxContextBuilder {
    builder.ids_created = ids_created;
    builder
}

/// Set the reference gas price for the `TxContextBuilder`.
public fun set_reference_gas_price(mut builder: TxContextBuilder, rgp: u64): TxContextBuilder {
    builder.rgp = option::some(rgp);
    builder
}

/// Set the reference gas price to `option::none()`.
public fun unset_reference_gas_price(mut builder: TxContextBuilder): TxContextBuilder {
    builder.rgp = option::none();
    builder
}

/// Set the gas price for the `TxContextBuilder`.
public fun set_gas_price(mut builder: TxContextBuilder, gas_price: u64): TxContextBuilder {
    builder.gas_price = gas_price;
    builder
}

/// Set the gas budget for the `TxContextBuilder`.
public fun set_gas_budget(mut builder: TxContextBuilder, gas_budget: u64): TxContextBuilder {
    builder.gas_budget = gas_budget;
    builder
}

/// Set the sponsor for the `TxContextBuilder`.
public fun set_sponsor(mut builder: TxContextBuilder, sponsor: address): TxContextBuilder {
    builder.sponsor = option::some(sponsor);
    builder
}

/// Set the sponsor for the `TxContextBuilder` to `option::none()`.
public fun unset_sponsor(mut builder: TxContextBuilder): TxContextBuilder {
    builder.sponsor = option::none();
    builder
}

// Create a `TxContext` from a `TxContextBuilder`.
// This is an internal function called when building a `Scenario`.
fun make_tx_context(builder: TxContextBuilder, tx_hash: vector<u8>): TxContext {
    tx_context::create(
        builder.sender,
        tx_hash,
        builder.epoch,
        builder.epoch_timestamp_ms,
        builder.ids_created,
        // rgp must be set from the caller always
        builder.rgp.destroy_some(),
        builder.gas_price,
        builder.gas_budget,
        builder.sponsor,
    )
}

/// Begin a new multi-transaction test scenario in a context where `sender` is the tx sender
public fun begin(sender: address): Scenario {
    Scenario {
        txn_number: 0,
        ctx: tx_context::new_from_hint(sender, 0, 0, 0, 0),
    }
}

/// Begin a new multi-transaction test scenario with a give `rgp` and `TxContextBuilder`.
public fun begin_with_context(ctx_builder: TxContextBuilder): Scenario {
    let txn_number = 0;
    assert!(ctx_builder.rgp.is_some());
    let hash = tx_context::dummy_tx_hash_with_hint(txn_number);
    let ctx = ctx_builder.make_tx_context(hash);
    Scenario {
        txn_number,
        ctx,
    }
}

/// Creates and shares system objects, allowing `Random`, `Clock`, `DenyList`
/// and other "native" objects, so they are available in the inventory.
///
/// NOTE: make sure to update this call when adding new system objects.
public fun create_system_objects(scenario: &mut Scenario) {
    let sender = scenario.ctx().sender();

    // Force publishing as system - 0x0.
    scenario.next_tx(@0x0);
    sui::clock::create_for_testing(scenario.ctx()).share_for_testing();
    sui::random::create_for_testing(scenario.ctx());
    sui::deny_list::create_for_testing(scenario.ctx());
    scenario.next_tx(sender);
}

/// Advance the scenario to a new transaction where `sender` is the transaction sender
/// All objects transferred will be moved into the inventories of the account or the global
/// inventory. In other words, in order to access an object with one of the various "take"
/// functions below, e.g. `take_from_address_by_id`, the transaction must first be ended via
/// `next_tx` or `next_with_context`.
/// Returns the results from the previous transaction
/// Will abort if shared or immutable objects were deleted, transferred, or wrapped.
/// Will abort if TransactionEffects cannot be generated
public fun next_tx(scenario: &mut Scenario, sender: address): TransactionEffects {
    // create a seed for new transaction digest to ensure that this tx has a different
    // digest (and consequently, different object ID's) than the previous tx
    scenario.txn_number = scenario.txn_number + 1;
    let epoch = scenario.ctx.epoch();
    let epoch_timestamp_ms = scenario.ctx.epoch_timestamp_ms();
    scenario.ctx =
        tx_context::new_from_hint(
            sender,
            scenario.txn_number,
            epoch,
            epoch_timestamp_ms,
            0,
        );
    // end the transaction
    end_transaction()
}

/// Advance the scenario to a new transaction with a given `TxContextBuilder`.
/// Ensures that `epoch` and `epoch_timestamp_ms` are not in the past.
/// If `rgp` is set, `epoch` must be greater than current epoch.
/// If a later epoch is provided, there will be as many transactions as epoch changes needed.
/// All objects transferred will be moved into the inventories of the account or the global
/// inventory. In other words, in order to access an object with one of the various "take"
/// functions below, e.g. `take_from_address_by_id`, the transaction must first be ended via
/// `next_tx` or `next_tx_wißth_context`.
/// Returns the results from the previous transaction
/// Will abort if shared or immutable objects were deleted, transferred, or wrapped.
/// Will abort if TransactionEffects cannot be generated
public fun next_with_context(
    scenario: &mut Scenario,
    ctx_builder: TxContextBuilder,
): TransactionEffects {
    let epoch = ctx_builder.epoch;
    let mut current_epoch = scenario.ctx.epoch();
    assert!(epoch >= current_epoch);
    // if `rgp` is set and it's not what is already there,
    // epoch must be greater than current epoch
    assert!(
        ctx_builder.rgp.is_none() || ctx_builder
            .rgp
            .contains(&scenario.ctx.reference_gas_price()) ||
        epoch > current_epoch,
    );
    assert!(ctx_builder.epoch_timestamp_ms >= scenario.ctx.epoch_timestamp_ms());
    if (epoch == current_epoch) {
        scenario.txn_number = scenario.txn_number + 1;
        let hash = tx_context::dummy_tx_hash_with_hint(scenario.txn_number);
        scenario.ctx = ctx_builder.make_tx_context(hash);
        end_transaction()
    } else {
        loop {
            current_epoch = current_epoch + 1;
            scenario.txn_number = scenario.txn_number + 1;
            let builder = ctx_builder;
            builder.set_epoch(current_epoch);
            let hash = tx_context::dummy_tx_hash_with_hint(scenario.txn_number);
            scenario.ctx = builder.make_tx_context(hash);
            let effects = end_transaction();
            if (current_epoch == epoch) {
                return effects
            }
        }
    }
}

/// Advance the scenario to a new epoch and end the transaction
/// See `next_tx` for further details
public fun next_epoch(scenario: &mut Scenario, sender: address): TransactionEffects {
    scenario.ctx.increment_epoch_number();
    next_tx(scenario, sender)
}

/// Advance the scenario to a new epoch, `delta_ms` milliseconds in the future and end
/// the transaction.
/// See `next_tx` for further details
public fun later_epoch(
    scenario: &mut Scenario,
    delta_ms: u64,
    sender: address,
): TransactionEffects {
    scenario.ctx.increment_epoch_timestamp(delta_ms);
    next_epoch(scenario, sender)
}

/// Advance the scenario to a future `epoch`. Will abort if the `epoch` is in the past.
public fun skip_to_epoch(scenario: &mut Scenario, epoch: u64) {
    assert!(epoch >= scenario.ctx.epoch());
    (epoch - scenario.ctx.epoch()).do!(|_| {
        scenario.ctx.increment_epoch_number();
        end_transaction()
    })
}

/// Ends the test scenario
/// Returns the results from the final transaction
/// Will abort if shared or immutable objects were deleted, transferred, or wrapped.
/// Will abort if TransactionEffects cannot be generated
public fun end(scenario: Scenario): TransactionEffects {
    let Scenario { txn_number: _, ctx: _ } = scenario;
    end_transaction()
}

// == accessors and helpers ==

/// Return the `TxContext` associated with this `scenario`
public fun ctx(scenario: &mut Scenario): &mut TxContext {
    &mut scenario.ctx
}

/// Generate a fresh ID for the current tx associated with this `scenario`
public fun new_object(scenario: &mut Scenario): UID {
    object::new(&mut scenario.ctx)
}

/// Return the sender of the current tx in this `scenario`
public fun sender(scenario: &Scenario): address {
    scenario.ctx.sender()
}

/// Return the number of concluded transactions in this scenario.
/// This does not include the current transaction, e.g. this will return 0 if `next_tx` has
/// not yet been called
public fun num_concluded_txes(scenario: &Scenario): u64 {
    scenario.txn_number
}

/// Accessor for `created` field of `TransactionEffects`
public fun created(effects: &TransactionEffects): vector<ID> {
    effects.created
}

/// Accessor for `written` field of `TransactionEffects`
public fun written(effects: &TransactionEffects): vector<ID> {
    effects.written
}

/// Accessor for `deleted` field of `TransactionEffects`
public fun deleted(effects: &TransactionEffects): vector<ID> {
    effects.deleted
}

/// Accessor for `transferred_to_account` field of `TransactionEffects`
public fun transferred_to_account(effects: &TransactionEffects): VecMap<ID, address> {
    effects.transferred_to_account
}

/// Accessor for `transferred_to_object` field of `TransactionEffects`
public fun transferred_to_object(effects: &TransactionEffects): VecMap<ID, ID> {
    effects.transferred_to_object
}

/// Accessor for `shared` field of `TransactionEffects`
public fun shared(effects: &TransactionEffects): vector<ID> {
    effects.shared
}

/// Accessor for `frozen` field of `TransactionEffects`
public fun frozen(effects: &TransactionEffects): vector<ID> {
    effects.frozen
}

/// Accessor for `num_user_events` field of `TransactionEffects`
public fun num_user_events(effects: &TransactionEffects): u64 {
    effects.num_user_events
}

// == from address ==

/// Remove the object of type `T` with ID `id` from the inventory of the `account`
/// An object is in the address's inventory if the object was transferred to the `account`
/// in a previous transaction. Using `return_to_address` is similar to `transfer` and you
/// must wait until the next transaction to re-take the object.
/// Aborts if there is no object of type `T` in the inventory with ID `id`
public native fun take_from_address_by_id<T: key>(scenario: &Scenario, account: address, id: ID): T;

/// Returns the most recent object of type `T` transferred to address `account` that has not
/// been taken
public native fun most_recent_id_for_address<T: key>(account: address): Option<ID>;

/// Returns all ids of type `T` transferred to address `account`.
public native fun ids_for_address<T: key>(account: address): vector<ID>;

/// helper that returns true iff `most_recent_id_for_address` returns some
public fun has_most_recent_for_address<T: key>(account: address): bool {
    most_recent_id_for_address<T>(account).is_some()
}

/// Helper combining `take_from_address_by_id` and `most_recent_id_for_address`
/// Aborts if there is no object of type `T` in the inventory of `account`
public fun take_from_address<T: key>(scenario: &Scenario, account: address): T {
    let id_opt = most_recent_id_for_address<T>(account);
    assert!(id_opt.is_some(), EEmptyInventory);
    take_from_address_by_id(scenario, account, id_opt.destroy_some())
}

/// Return `t` to the inventory of the `account`. `transfer` can be used directly instead,
/// but this function is helpful for test cleanliness as it will abort if the object was not
/// originally taken from this account
public fun return_to_address<T: key>(account: address, t: T) {
    let id = object::id(&t);
    assert!(was_taken_from_address(account, id), ECantReturnObject);
    sui::transfer::transfer_impl(t, account)
}

/// Returns true if the object with `ID` id was in the inventory for `account`
public native fun was_taken_from_address(account: address, id: ID): bool;

// == from sender ==

/// helper for `take_from_address_by_id` that operates over the transaction sender
public fun take_from_sender_by_id<T: key>(scenario: &Scenario, id: ID): T {
    take_from_address_by_id(scenario, sender(scenario), id)
}

/// helper for `most_recent_id_for_address` that operates over the transaction sender
public fun most_recent_id_for_sender<T: key>(scenario: &Scenario): Option<ID> {
    most_recent_id_for_address<T>(sender(scenario))
}

/// helper that returns true iff `most_recent_id_for_sender` returns some
public fun has_most_recent_for_sender<T: key>(scenario: &Scenario): bool {
    most_recent_id_for_address<T>(sender(scenario)).is_some()
}

/// helper for `take_from_address` that operates over the transaction sender
public fun take_from_sender<T: key>(scenario: &Scenario): T {
    take_from_address(scenario, sender(scenario))
}

/// helper for `return_to_address` that operates over the transaction sender
public fun return_to_sender<T: key>(scenario: &Scenario, t: T) {
    return_to_address(sender(scenario), t)
}

/// Returns true if the object with `ID` id was in the inventory for the sender
public fun was_taken_from_sender(scenario: &Scenario, id: ID): bool {
    was_taken_from_address(sender(scenario), id)
}

/// Returns all ids of type `T` transferred to the sender.
public fun ids_for_sender<T: key>(scenario: &Scenario): vector<ID> {
    ids_for_address<T>(sender(scenario))
}

// == immutable ==

/// Remove the immutable object of type `T` with ID `id` from the global inventory
/// Aborts if there is no object of type `T` in the inventory with ID `id`
public native fun take_immutable_by_id<T: key>(scenario: &Scenario, id: ID): T;

/// Returns the most recent immutable object of type `T` that has not been taken
public native fun most_recent_immutable_id<T: key>(): Option<ID>;

/// helper that returns true iff `most_recent_immutable_id` returns some
public fun has_most_recent_immutable<T: key>(): bool {
    most_recent_immutable_id<T>().is_some()
}

/// Helper combining `take_immutable_by_id` and `most_recent_immutable_id`
/// Aborts if there is no immutable object of type `T` in the global inventory
public fun take_immutable<T: key>(scenario: &Scenario): T {
    let id_opt = most_recent_immutable_id<T>();
    assert!(id_opt.is_some(), EEmptyInventory);
    take_immutable_by_id(scenario, id_opt.destroy_some())
}

/// Return `t` to the global inventory
public fun return_immutable<T: key>(t: T) {
    let id = object::id(&t);
    assert!(was_taken_immutable(id), ECantReturnObject);
    sui::transfer::freeze_object_impl(t)
}

/// Returns true if the object with `ID` id was an immutable object in the global inventory
public native fun was_taken_immutable(id: ID): bool;

// == shared ==

/// Remove the shared object of type `T` with ID `id` from the global inventory
/// Aborts if there is no object of type `T` in the inventory with ID `id`
public native fun take_shared_by_id<T: key>(scenario: &Scenario, id: ID): T;

/// Returns the most recent shared object of type `T` that has not been taken
public native fun most_recent_id_shared<T: key>(): Option<ID>;

/// helper that returns true iff `most_recent_id_shared` returns some
public fun has_most_recent_shared<T: key>(): bool {
    most_recent_id_shared<T>().is_some()
}

/// Helper combining `take_shared_by_id` and `most_recent_id_shared`
/// Aborts if there is no shared object of type `T` in the global inventory
public fun take_shared<T: key>(scenario: &Scenario): T {
    let id_opt = most_recent_id_shared<T>();
    assert!(id_opt.is_some(), EEmptyInventory);
    take_shared_by_id(scenario, id_opt.destroy_some())
}

/// Return `t` to the global inventory
public fun return_shared<T: key>(t: T) {
    let id = object::id(&t);
    assert!(was_taken_shared(id), ECantReturnObject);
    sui::transfer::share_object_impl(t)
}

/// Return the IDs of the receivalbe objects that `object` owns.
public fun receivable_object_ids_for_owner_id<T: key>(object: ID): vector<ID> {
    ids_for_address<T>(object::id_to_address(&object))
}

/// Create a `Receiving<T>` receiving ticket for the most recent
/// object of type `T` that is owned by the `owner` object ID.
public fun most_recent_receiving_ticket<T: key>(owner: &ID): sui::transfer::Receiving<T> {
    let id_opt = most_recent_id_for_address<T>(object::id_to_address(owner));
    assert!(option::is_some(&id_opt), EEmptyInventory);
    let id = option::destroy_some(id_opt);
    receiving_ticket_by_id<T>(id)
}

/// Create a `Receiving<T>` receiving ticket for the object of type
/// `T` with the given `object_id`.
public fun receiving_ticket_by_id<T: key>(object_id: ID): sui::transfer::Receiving<T> {
    let version = allocate_receiving_ticket_for_object<T>(object_id);
    sui::transfer::make_receiver(object_id, version)
}

/// Deallocate a `Receiving<T>` receiving ticket. This must be done in
/// order to use the object further (unless the object was received) in a
/// test scenario.
public fun return_receiving_ticket<T: key>(ticket: sui::transfer::Receiving<T>) {
    let id = sui::transfer::receiving_id(&ticket);
    deallocate_receiving_ticket_for_object(id);
}

// == macros ==

/// Take a shared object from the global inventory and call the function `$f` on it.
///
/// ```move
/// use std::unit_test::assert_eq;
/// use sui::test_scenario;
///
/// #[test]
/// fun with_shared() {
///     let mut test = test_scenario::begin(@0);
///     test.create_system_objects();
///
///     // Take the `Clock` object from the inventory.
///     test.with_shared!<Clock>(|clock, test| {
///         assert_eq!(clock.timestamp_ms(), 0);
///     });
///
///     test.end();
/// }
/// ```
public macro fun with_shared<$T: key>($scenario: &mut Scenario, $f: |&mut $T, &mut Scenario| -> _) {
    let s = $scenario;
    let mut obj = s.take_shared<$T>();
    $f(&mut obj, s);
    return_shared(obj);
}

/// Take a shared object from the global inventory with the given `id` and call the function `$f` on it.
/// Works similarly to `with_shared` but takes an extra `id` parameter.
///
/// See `with_shared` for more details.
public macro fun with_shared_by_id<$T: key>(
    $scenario: &mut Scenario,
    $id: ID,
    $f: |&mut $T, &mut Scenario| -> _,
) {
    let s = $scenario;
    let mut obj = s.take_shared_by_id<$T>($id);
    $f(&mut obj, s);
    return_shared(obj);
}

// == natives ===

/// Returns true if the object with `ID` id was an shared object in the global inventory
native fun was_taken_shared(id: ID): bool;

/// Allocate the receiving ticket for the object of type `T` with the given
/// `object_id`. Returns the current version of object.
native fun allocate_receiving_ticket_for_object<T: key>(object_id: ID): u64;

/// Deallocate the receiving ticket for the object with the given `object_id`.
native fun deallocate_receiving_ticket_for_object(object_id: ID);

// == internal ==

// internal function that ends the transaction, realizing changes (may abort with
// `ECouldNotGenerateEffects`)
native fun end_transaction(): TransactionEffects;
