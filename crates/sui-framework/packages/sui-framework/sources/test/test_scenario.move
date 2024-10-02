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

/// Begin a new multi-transaction test scenario in a context where `sender` is the tx sender
public fun begin(sender: address): Scenario {
    Scenario {
        txn_number: 0,
        ctx: tx_context::new_from_hint(sender, 0, 0, 0, 0),
    }
}

/// Advance the scenario to a new transaction where `sender` is the transaction sender
/// All objects transferred will be moved into the inventories of the account or the global
/// inventory. In other words, in order to access an object with one of the various "take"
/// functions below, e.g. `take_from_address_by_id`, the transaction must first be ended via
/// `next_tx`.
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
