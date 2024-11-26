// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This module facilitates the rental of NFTs using kiosks.
///
/// It allows users to list their NFTs for renting, rent NFTs for a specified duration, and return
/// them after the rental period.
module nft_rental::rentables_ext;

use kiosk::kiosk_lock_rule::Rule as LockRule;
use sui::{
    bag,
    balance::{Self, Balance},
    clock::Clock,
    coin::{Self, Coin},
    kiosk::{Kiosk, KioskOwnerCap},
    kiosk_extension,
    package::Publisher,
    sui::SUI,
    transfer_policy::{Self, TransferPolicy, TransferPolicyCap, has_rule}
};

// === Imports ===

// === Errors ===
const EExtensionNotInstalled: u64 = 0;
const ENotOwner: u64 = 1;
const ENotEnoughCoins: u64 = 2;
const EInvalidKiosk: u64 = 3;
const ERentingPeriodNotOver: u64 = 4;
const EObjectNotExist: u64 = 5;
const ETotalPriceOverflow: u64 = 6;

// === Constants ===
const PERMISSIONS: u128 = 11;
const SECONDS_IN_A_DAY: u64 = 86400;
const MAX_BASIS_POINTS: u16 = 10_000;
const MAX_VALUE_U64: u64 = 0xff_ff_ff_ff__ff_ff_ff_ff;

// === Structs ===

/// Extension Key for Kiosk Rentables extension.
public struct Rentables has drop {}

/// Struct representing a rented item.
/// Used as a key for the Rentable that's placed in the Extension's Bag.
public struct Rented has store, copy, drop { id: ID }

/// Struct representing a listed item.
/// Used as a key for the Rentable that's placed in the Extension's Bag.
public struct Listed has store, copy, drop { id: ID }

/// Promise struct for borrowing by value.
public struct Promise {
    item: Rented,
    duration: u64,
    start_date: u64,
    price_per_day: u64,
    renter_kiosk: ID,
    borrower_kiosk: ID,
}

/// A wrapper object that holds an asset that is being rented.
/// Contains information relevant to the rental period, cost and renter.
public struct Rentable<T: key + store> has store {
    object: T,
    /// Total amount of time offered for renting in days.
    duration: u64,
    /// Initially undefined, is updated once someone rents it.
    start_date: Option<u64>,
    price_per_day: u64,
    /// The kiosk id that the object was taken from.
    kiosk_id: ID,
}

/// A shared object that should be minted by every creator.
/// Defines the royalties the creator will receive from each rent invocation.
public struct RentalPolicy<phantom T> has key, store {
    id: UID,
    balance: Balance<SUI>,
    /// Note: Move does not support float numbers.
    ///
    /// If you need to represent a float, you need to determine the desired
    /// precision and use a larger integer representation.
    ///
    /// For example, percentages can be represented using basis points:
    /// 10000 basis points represent 100% and 100 basis points represent 1%.
    amount_bp: u64,
}

/// A shared object that should be minted by every creator.
/// Even for creators that do not wish to enforce royalties. Provides authorized access to an
/// empty TransferPolicy.
public struct ProtectedTP<phantom T> has key, store {
    id: UID,
    transfer_policy: TransferPolicy<T>,
    policy_cap: TransferPolicyCap<T>,
}

// === Public Functions ===

/// Enables someone to install the Rentables extension in their Kiosk.
public fun install(kiosk: &mut Kiosk, cap: &KioskOwnerCap, ctx: &mut TxContext) {
    kiosk_extension::add(Rentables {}, kiosk, cap, PERMISSIONS, ctx);
}

/// Remove the extension from the Kiosk. Can only be performed by the owner,
/// The extension storage must be empty for the transaction to succeed.
public fun remove(kiosk: &mut Kiosk, cap: &KioskOwnerCap, _ctx: &mut TxContext) {
    kiosk_extension::remove<Rentables>(kiosk, cap);
}

/// Mints and shares a ProtectedTP & a RentalPolicy object for type T.
/// Can only be performed by the publisher of type T.
public fun setup_renting<T>(publisher: &Publisher, amount_bp: u64, ctx: &mut TxContext) {
    // Creates an empty TP and shares a ProtectedTP<T> object.
    // This can be used to bypass the lock rule under specific conditions.
    // Storing inside the cap the ProtectedTP with no way to access it
    // as we do not want to modify this policy
    let (transfer_policy, policy_cap) = transfer_policy::new<T>(publisher, ctx);

    let protected_tp = ProtectedTP {
        id: object::new(ctx),
        transfer_policy,
        policy_cap,
    };

    let rental_policy = RentalPolicy<T> {
        id: object::new(ctx),
        balance: balance::zero<SUI>(),
        amount_bp,
    };

    transfer::share_object(protected_tp);
    transfer::share_object(rental_policy);
}

/// Enables someone to list an asset within the Rentables extension's Bag,
/// creating a Bag entry with the asset's ID as the key and a Rentable wrapper object as the value.
/// Requires the existance of a ProtectedTP which can only be created by the creator of type T.
/// Assumes item is already placed (& optionally locked) in a Kiosk.
public fun list<T: key + store>(
    kiosk: &mut Kiosk,
    cap: &KioskOwnerCap,
    protected_tp: &ProtectedTP<T>,
    item_id: ID,
    duration: u64,
    price_per_day: u64,
    ctx: &mut TxContext,
) {
    assert!(kiosk_extension::is_installed<Rentables>(kiosk), EExtensionNotInstalled);

    kiosk.set_owner(cap, ctx);
    kiosk.list<T>(cap, item_id, 0);

    let coin = coin::zero<SUI>(ctx);
    let (object, request) = kiosk.purchase<T>(item_id, coin);

    let (_item, _paid, _from) = protected_tp.transfer_policy.confirm_request(request);

    let rentable = Rentable {
        object,
        duration,
        start_date: option::none<u64>(),
        price_per_day,
        kiosk_id: object::id(kiosk),
    };

    place_in_bag<T, Listed>(kiosk, Listed { id: item_id }, rentable);
}

/// Allows the renter to delist an item, that is not currently being rented.
/// Places (or locks, if a lock rule is present) the object back to owner's Kiosk.
/// Creators should mint an empty TransferPolicy even if they don't want to apply any royalties.
/// If they wish at some point to enforce royalties, they can update the existing TransferPolicy.
public fun delist<T: key + store>(
    kiosk: &mut Kiosk,
    cap: &KioskOwnerCap,
    transfer_policy: &TransferPolicy<T>,
    item_id: ID,
    _ctx: &mut TxContext,
) {
    assert!(kiosk.has_access(cap), ENotOwner);

    let rentable = take_from_bag<T, Listed>(kiosk, Listed { id: item_id });

    let Rentable {
        object,
        duration: _,
        start_date: _,
        price_per_day: _,
        kiosk_id: _,
    } = rentable;

    if (has_rule<T, LockRule>(transfer_policy)) {
        kiosk.lock(cap, transfer_policy, object);
    } else {
        kiosk.place(cap, object);
    };
}

/// This enables individuals to rent a listed Rentable.
///
/// It permits anyone to borrow an item on behalf of another user, provided they have the
/// Rentables extension installed.
///
/// The Rental Policy defines the portion of the coin that will be retained as fees and added to
/// the Rental Policy's balance.
public fun rent<T: key + store>(
    renter_kiosk: &mut Kiosk,
    borrower_kiosk: &mut Kiosk,
    rental_policy: &mut RentalPolicy<T>,
    item_id: ID,
    mut coin: Coin<SUI>,
    clock: &Clock,
    ctx: &mut TxContext,
) {
    assert!(kiosk_extension::is_installed<Rentables>(borrower_kiosk), EExtensionNotInstalled);

    let mut rentable = take_from_bag<T, Listed>(renter_kiosk, Listed { id: item_id });

    let max_price_per_day = MAX_VALUE_U64 / rentable.duration;
    assert!(rentable.price_per_day <= max_price_per_day, ETotalPriceOverflow);
    let total_price = rentable.price_per_day * rentable.duration;

    let coin_value = coin.value();
    assert!(coin_value == total_price, ENotEnoughCoins);

    // Calculate fees_amount using the given basis points amount (percentage), ensuring the
    // result fits into a 64-bit unsigned integer.
    let mut fees_amount = coin_value as u128;
    fees_amount = fees_amount * (rental_policy.amount_bp as u128);
    fees_amount = fees_amount / (MAX_BASIS_POINTS as u128);

    let fees = coin.split(fees_amount as u64, ctx);

    coin::put(&mut rental_policy.balance, fees);
    transfer::public_transfer(coin, renter_kiosk.owner());
    rentable.start_date.fill(clock.timestamp_ms());

    place_in_bag<T, Rented>(borrower_kiosk, Rented { id: item_id }, rentable);
}

/// Enables the borrower to acquire the Rentable by reference from their bag.
public fun borrow<T: key + store>(
    kiosk: &mut Kiosk,
    cap: &KioskOwnerCap,
    item_id: ID,
    _ctx: &mut TxContext,
): &T {
    assert!(kiosk.has_access(cap), ENotOwner);
    let ext_storage_mut = kiosk_extension::storage_mut(Rentables {}, kiosk);
    let rentable: &Rentable<T> = &ext_storage_mut[Rented { id: item_id }];
    &rentable.object
}

/// Enables the borrower to temporarily acquire the Rentable with an agreement or promise to
/// return it.
///
/// All the information about the Rentable is stored within the promise, facilitating the
/// reconstruction of the Rentable when the object is returned.
public fun borrow_val<T: key + store>(
    kiosk: &mut Kiosk,
    cap: &KioskOwnerCap,
    item_id: ID,
    _ctx: &mut TxContext,
): (T, Promise) {
    assert!(kiosk.has_access(cap), ENotOwner);
    let borrower_kiosk = object::id(kiosk);

    let rentable = take_from_bag<T, Rented>(kiosk, Rented { id: item_id });

    let promise = Promise {
        item: Rented { id: item_id },
        duration: rentable.duration,
        start_date: *option::borrow(&rentable.start_date),
        price_per_day: rentable.price_per_day,
        renter_kiosk: rentable.kiosk_id,
        borrower_kiosk,
    };

    let Rentable {
        object,
        duration: _,
        start_date: _,
        price_per_day: _,
        kiosk_id: _,
    } = rentable;

    (object, promise)
}

/// Enables the borrower to return the borrowed item.
public fun return_val<T: key + store>(
    kiosk: &mut Kiosk,
    object: T,
    promise: Promise,
    _ctx: &mut TxContext,
) {
    assert!(kiosk_extension::is_installed<Rentables>(kiosk), EExtensionNotInstalled);

    let Promise {
        item,
        duration,
        start_date,
        price_per_day,
        renter_kiosk,
        borrower_kiosk,
    } = promise;

    let kiosk_id = object::id(kiosk);
    assert!(kiosk_id == borrower_kiosk, EInvalidKiosk);

    let rentable = Rentable {
        object,
        duration,
        start_date: option::some(start_date),
        price_per_day,
        kiosk_id: renter_kiosk,
    };

    place_in_bag(kiosk, item, rentable);
}

/// Enables the owner to reclaim their asset once the rental period has concluded.
public fun reclaim<T: key + store>(
    renter_kiosk: &mut Kiosk,
    borrower_kiosk: &mut Kiosk,
    transfer_policy: &TransferPolicy<T>,
    clock: &Clock,
    item_id: ID,
    _ctx: &mut TxContext,
) {
    assert!(kiosk_extension::is_installed<Rentables>(renter_kiosk), EExtensionNotInstalled);

    let rentable = take_from_bag<T, Rented>(borrower_kiosk, Rented { id: item_id });

    let Rentable {
        object,
        duration,
        start_date,
        price_per_day: _,
        kiosk_id,
    } = rentable;

    assert!(object::id(renter_kiosk) == kiosk_id, EInvalidKiosk);

    let start_date_ms = *option::borrow(&start_date);
    let current_timestamp = clock.timestamp_ms();
    let final_timestamp = start_date_ms + duration * SECONDS_IN_A_DAY;
    assert!(current_timestamp > final_timestamp, ERentingPeriodNotOver);

    if (transfer_policy.has_rule<T, LockRule>()) {
        kiosk_extension::lock<Rentables, T>(
            Rentables {},
            renter_kiosk,
            object,
            transfer_policy,
        );
    } else {
        kiosk_extension::place<Rentables, T>(
            Rentables {},
            renter_kiosk,
            object,
            transfer_policy,
        );
    };
}

// === Private Functions ===

fun take_from_bag<T: key + store, Key: store + copy + drop>(
    kiosk: &mut Kiosk,
    item: Key,
): Rentable<T> {
    let ext_storage_mut = kiosk_extension::storage_mut(Rentables {}, kiosk);
    assert!(bag::contains(ext_storage_mut, item), EObjectNotExist);
    bag::remove<Key, Rentable<T>>(
        ext_storage_mut,
        item,
    )
}

fun place_in_bag<T: key + store, Key: store + copy + drop>(
    kiosk: &mut Kiosk,
    item: Key,
    rentable: Rentable<T>,
) {
    let ext_storage_mut = kiosk_extension::storage_mut(Rentables {}, kiosk);
    bag::add(ext_storage_mut, item, rentable);
}

// === Test Functions ===

#[test_only]
// public fun test_take_from_bag<T: key + store>(kiosk: &mut Kiosk, item_id: ID) {
public fun test_take_from_bag<T: key + store, Key: store + copy + drop>(
    kiosk: &mut Kiosk,
    item: Key,
) {
    let rentable = take_from_bag<T, Key>(kiosk, item);

    let Rentable {
        object,
        duration: _,
        start_date: _,
        price_per_day: _,
        kiosk_id: _,
    } = rentable;

    transfer::public_share_object(object);
}

#[test_only]
public fun create_listed(id: ID): Listed {
    Listed { id }
}
