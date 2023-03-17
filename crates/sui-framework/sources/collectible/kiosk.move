// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// / Kiosk is a primitive for building open, zero-fee trading platforms
// / for assets with a high degree of customization over transfer
// / policies.
// /
// / The system has 3 main audiences:
// /
// / 1. Creators: for a type to be tradable in the Kiosk ecosystem,
// / creator (publisher) of the type needs to issue a `TransferPolicyCap`
// / which gives them a power to enforce any constraint on trades by
// / either using one of the pre-built primitives (see `sui::royalty`)
// / or by implementing a custom policy. The latter requires additional
// / support for discoverability in the ecosystem and should be performed
// / within the scope of an Application or some platform.
// /
// / - A type can not be traded in the Kiosk unless there's a policy for it.
// / - 0-royalty policy is just as easy as "freezing" the `AllowTransferCap`
// /   making it available for everyone to authorize deals "for free"
// /
// / 2. Traders: anyone can create a Kiosk and depending on whether it's
// / a shared object or some shared-wrapper the owner can trade any type
// / that has issued `TransferPolicyCap` in a Kiosk. To do so, they need
// / to make an offer, and any party can purchase the item for the amount of
// / SUI set in the offer. The responsibility to follow the transfer policy
// / set by the creator of the `T` is on the buyer.
// /
// / 3. Marketplaces: marketplaces can either watch for the offers made in
// / personal Kiosks or even integrate the Kiosk primitive and build on top
// / of it. In the custom logic scenario, the `TransferPolicyCap` can also
// / be used to implement application-specific transfer rules.
// /
// module sui::kiosk {
//     use std::option::{Self, Option};
//     use sui::object::{Self, UID, ID};
//     use sui::dynamic_object_field as dof;
//     use sui::dynamic_field as df;
//     use sui::package::{Self, Publisher};
//     use sui::transfer_request::{Self, TransferRequest};
//     use sui::tx_context::{TxContext, sender};
//     use sui::balance::{Self, Balance};
//     use sui::coin::{Self, Coin};
//     use sui::bag::{Self, Bag};
//     use sui::sui::SUI;
//     use sui::event;

//     // Collectible is a special case to avoid storing `Publisher`.
//     friend sui::collectible;

//     /// Trying to withdraw profits as owner and owner is not set.
//     const EOwnerNotSet: u64 = 0;
//     /// Trying to withdraw profits and sender is not owner.
//     const ENotOwner: u64 = 1;
//     /// Coin paid does not match the offer price.
//     const EIncorrectAmount: u64 = 2;
//     /// Incorrect arguments passed into `switch_mode` function.
//     const EIncorrectArgument: u64 = 3;
//     /// Transfer is accepted by a wrong Kiosk.
//     const EWrongTarget: u64 = 4;
//     /// Trying to withdraw higher amount than stored.
//     const ENotEnough: u64 = 5;
//     /// Trying to close a Kiosk and it has items in it.
//     const ENotEmpty: u64 = 6;
//     /// Attempt to take an item that has a `PurchaseCap` issued.
//     const EListedExclusively: u64 = 7;
//     /// `PurchaseCap` does not match the `Kiosk`.
//     const EWrongKiosk: u64 = 8;
//     /// Trying to allow a `TransferRequest` with unresolved constraints.
//     const EUnresolvedConstraints: u64 = 9;
//     /// Attempt to attach an unregistered constraint to the `TransferRequest`.
//     const EConstraintNotRegistered: u64 = 10;
//     /// Tryng to exclusively list an already listed item.
//     const EAlreadyListed: u64 = 11;
//     /// Trying to call `uid_mut` when extensions disabled
//     const EExtensionsDisabled: u64 = 12;

//     /// An object which allows selling collectibles within "kiosk" ecosystem.
//     /// By default gives the functionality to list an item openly - for anyone
//     /// to purchase proviging the guarantees for creators that every transfer
//     /// needs to be approved via the `TransferPolicy`.
//     struct Kiosk has key, store {
//         id: UID,
//         /// Balance of the Kiosk - all profits from sales go here.
//         profits: Balance<SUI>,
//         /// Always point to `sender` of the transaction.
//         /// Can be changed by calling `set_owner` with Cap.
//         owner: address,
//         /// Number of items stored in a Kiosk. Used to allow unpacking
//         /// an empty Kiosk if it was wrapped or has a single owner.
//         item_count: u32,
//         /// Whether to open the UID to public. Set to `true` by default
//         /// but the owner can switch the state if necessary.
//         allow_extensions: bool
//     }

//     /// A Capability granting the bearer a right to `place` and `take` items
//     /// from the `Kiosk` as well as to `list` them and `list_with_purchase_cap`.
//     struct KioskOwnerCap has key, store {
//         id: UID,
//         for: ID
//     }

//     /// A capability which locks an item and gives a permission to
//     /// purchase it from a `Kiosk` for any price no less than `min_price`.
//     ///
//     /// Allows exclusive listing: only bearer of the `PurchaseCap` can
//     /// purchase the asset. However, the capablity should be used
//     /// carefully as losing it would lock the asset in the `Kiosk`.
//     struct PurchaseCap<phantom T: key + store> has key, store {
//         id: UID,
//         /// ID of the `Kiosk` the cap belongs to.
//         kiosk_id: ID,
//         /// ID of the listed item.
//         item_id: ID,
//         /// Minimum price for which the item can be purchased.
//         min_price: u64
//     }

//     // === Dynamic Field keys ===

//     /// Dynamic field key for an item placed into the kiosk.
//     struct Key has store, copy, drop { id: ID }

//     /// Dynamic field key for an active offer to purchase the T. If an
//     /// item is listed without a `PurchaseCap`, exclusive is set to `false`.
//     struct Offer has store, copy, drop { id: ID, is_exclusive: bool }

//     // === Events ===

//     /// Emitted when an item was listed by the safe owner. Can be used
//     /// to track available offers anywhere on the network; the event is
//     /// type-indexed which allows for searching for offers of a specific `T`
//     struct ItemListed<phantom T: key + store> has copy, drop {
//         kiosk: ID,
//         id: ID,
//         price: u64
//     }

//     // === New Kiosk + ownership modes ===

//     /// Creates a new Kiosk without owner but with a Capability.
//     public fun new(ctx: &mut TxContext): (Kiosk, KioskOwnerCap) {
//         let kiosk = Kiosk {
//             id: object::new(ctx),
//             profits: balance::zero(),
//             owner: sender(ctx),
//             item_count: 0
//         };

//         let cap = KioskOwnerCap {
//             id: object::new(ctx),
//             for: object::id(&kiosk)
//         };

//         (kiosk, cap)
//     }

//     /// Unpacks and destroys a Kiosk returning the profits (even if "0").
//     /// Can only be performed by the bearer of the `KioskOwnerCap` in the
//     /// case where there's no items inside and a `Kiosk` is not shared.
//     public fun close_and_withdraw(
//         self: Kiosk, cap: KioskOwnerCap, ctx: &mut TxContext
//     ): Coin<SUI> {
//         let Kiosk { id, profits, owner: _, item_count } = self;
//         let KioskOwnerCap { id: cap_id, for } = cap;

//         assert!(object::uid_to_inner(&id) == for, ENotOwner);
//         assert!(item_count == 0, ENotEmpty);

//         object::delete(cap_id);
//         object::delete(id);

//         coin::from_balance(profits, ctx)
//     }

//     /// Change the owner to the transaction sender.
//     /// The change is purely cosmetical and does not affect any of the
//     /// basic kiosk functions unless some logic for this is implemented
//     /// in a third party module.
//     public fun set_owner(
//         self: &mut Kiosk, cap: &KioskOwnerCap, ctx: &TxContext
//     ) {
//         assert!(object::id(self) == cap.for, ENotOwner);
//         self.owner = sender(ctx);
//     }

//     // === Place and take from the Kiosk ===

//     /// Place any object into a Kiosk.
//     /// Performs an authorization check to make sure only owner can do that.
//     public fun place<T: key + store>(
//         self: &mut Kiosk, cap: &KioskOwnerCap, item: T
//     ) {
//         assert!(object::id(self) == cap.for, ENotOwner);
//         self.item_count = self.item_count + 1;
//         dof::add(&mut self.id, Key { id: object::id(&item) }, item)
//     }

//     /// Take any object from the Kiosk.
//     /// Performs an authorization check to make sure only owner can do that.
//     public fun take<T: key + store>(
//         self: &mut Kiosk, cap: &KioskOwnerCap, id: ID
//     ): T {
//         assert!(object::id(self) == cap.for, ENotOwner);
//         assert!(df::exists_<Offer>(&mut self.id, Offer { id, is_exclusive: true }) == false, EListedExclusively);

//         self.item_count = self.item_count - 1;
//         df::remove_if_exists<Offer, u64>(&mut self.id, Offer { id, is_exclusive: false });
//         dof::remove(&mut self.id, Key { id })
//     }

//     // === Trading functionality: List and Purchase ===

//     /// List the item by setting a price and making it available for purchase.
//     /// Performs an authorization check to make sure only owner can sell.
//     public fun list<T: key + store>(
//         self: &mut Kiosk, cap: &KioskOwnerCap, id: ID, price: u64
//     ) {
//         assert!(object::id(self) == cap.for, ENotOwner);
//         assert!(df::exists_<Offer>(&mut self.id, Offer { id, is_exclusive: true }) == false, EListedExclusively);

//         df::add(&mut self.id, Offer { id, is_exclusive: false }, price);
//         event::emit(NewOfferEvent<T> {
//             kiosk: object::id(self), id, price
//         })
//     }

//     /// Calls `place` and `list` together - simplifies the flow.
//     public fun place_and_list<T: key + store>(
//         self: &mut Kiosk, cap: &KioskOwnerCap, item: T, price: u64
//     ) {
//         let id = object::id(&item);
//         place(self, cap, item);
//         list<T>(self, cap, id, price)
//     }

//     /// Make a trade: pay the owner of the item and request a Transfer to the `target`
//     /// kiosk (to prevent item being taken by the approving party).
//     ///
//     /// Received `TransferRequest` needs to be handled by the publisher of the T,
//     /// if they have a method implemented that allows a trade, it is possible to
//     /// request their approval (by calling some function) so that the trade can be
//     /// finalized.
//     public fun purchase<T: key + store>(
//         self: &mut Kiosk, id: ID, payment: Coin<SUI>, ctx: &mut TxContext
//     ): (T, TransferRequest<T>) {
//         let price = df::remove<Offer, u64>(&mut self.id, Offer { id, is_exclusive: false });
//         let inner = dof::remove<Key, T>(&mut self.id, Key { id });

//         self.item_count = self.item_count - 1;
//         assert!(price == coin::value(&payment), EIncorrectAmount);
//         balance::join(&mut self.profits, coin::into_balance(payment));

//         (inner, transfer_request::new(price, object::id(self)))
//     }

//     // === Trading Functionality: Exclusive listing with `PurchaseCap` ===

//     /// Creates a `PurchaseCap` which gives the right to purchase an item
//     /// for any price equal or higher than the `min_price`.
//     public fun list_with_purchase_cap<T: key + store>(
//         self: &mut Kiosk, cap: &KioskOwnerCap, id: ID, min_price: u64, ctx: &mut TxContext
//     ): PurchaseCap<T> {
//         assert!(object::id(self) == cap.for, ENotOwner);
//         assert!(df::exists_<Offer>(&mut self.id, Offer { id, is_exclusive: false }) == false, EAlreadyListed);

//         let uid = object::new(ctx);
//         df::add(&mut self.id, Offer { id, is_exclusive: true }, min_price);

//         PurchaseCap<T> {
//             id: uid,
//             item_id: id,
//             kiosk_id: cap.for,
//             min_price,
//         }
//     }

//     /// Unpack the `PurchaseCap` and call `purchase`. Sets the payment amount
//     /// as the price for the listing making sure it's no less than `min_amount`.
//     public fun purchase_with_cap<T: key + store>(
//         self: &mut Kiosk, purchase_cap: PurchaseCap<T>, payment: Coin<SUI>, ctx: &mut TxContext
//     ): (T, TransferRequest<T>) {
//         let PurchaseCap { id, item_id, kiosk_id, min_price } = purchase_cap;
//         let paid = coin::value(&payment);

//         assert!(paid >= min_price, EIncorrectAmount);
//         assert!(object::id(self) == kiosk_id, EWrongKiosk);

//         df::remove<Offer, u64>(&mut self.id, Offer { id: item_id, is_exclusive: true });
//         df::add(&mut self.id, Offer { id: item_id, is_exclusive: false }, paid);
//         object::delete(id);

//         purchase<T>(self, item_id, payment, ctx)
//     }

//     /// Return the `PurchaseCap` without making a purchase; remove an active offer and
//     /// allow the item for taking. Can only be returned to its `Kiosk`, aborts otherwise.
//     public fun return_purchase_cap<T: key + store>(
//         self: &mut Kiosk, purchase_cap: PurchaseCap<T>
//     ) {
//         let PurchaseCap { id, item_id, kiosk_id, min_price: _ } = purchase_cap;

//         assert!(object::id(self) == kiosk_id, EWrongKiosk);
//         df::remove<Offer, u64>(&mut self.id, Offer { id: item_id, is_exclusive: true });
//         object::delete(id)
//     }

//     /// Withdraw profits from the Kiosk.
//     public fun withdraw(
//         self: &mut Kiosk, cap: &KioskOwnerCap, amount: Option<u64>, ctx: &mut TxContext
//     ): Coin<SUI> {
//         assert!(object::id(self) == cap.for, ENotOwner);

//         let amount = if (option::is_some(&amount)) {
//             let amt = option::destroy_some(amount);
//             assert!(amt <= balance::value(&self.profits), ENotEnough);
//             amt
//         } else {
//             balance::value(&self.profits)
//         };

//         coin::take(&mut self.profits, amount, ctx)
//     }

//     // === Kiosk fields access ===

//     /// Check whether the `KioskOwnerCap` matches the `Kiosk`.
//     public fun is_owner(self: &mut Kiosk, cap: &KioskOwnerCap): bool {
//         object::id(self) == cap.for
//     }

//     /// Access the `UID` using the `KioskOwnerCap`.
//     public fun uid_mut_as_owner(self: &mut Kiosk, cap: &KioskOwnerCap): &mut UID {
//         assert!(object::id(self) == cap.for, ENotOwner);
//         &mut self.id
//     }

//     /// Get the UID to for dynamic field access. Requires a `KioskOwnerCap`
//     /// to prevent third party attachements without owner's approval in the
//     /// shared storage scenario.
//     public fun uid_mut(self: &mut Kiosk): &mut UID {
//         assert!(self.allow_extensions, EExtensionsDisabled);
//         &mut self.id
//     }

//     /// Get the owner of the Kiosk.
//     public fun owner(self: &Kiosk): address {
//         self.owner
//     }

//     /// Get the number of items stored in a Kiosk.
//     public fun item_count(self: &Kiosk): u32 {
//         self.item_count
//     }

//     /// Get the amount of profits collected by selling items.
//     public fun profits_amount(self: &Kiosk): u64 {
//         balance::value(&self.profits)
//     }

//     // === PurchaseCap fields access ===

//     /// Get the `kiosk_id` from the `PurchaseCap`.
//     public fun purchase_cap_kiosk<T: key + store>(self: &PurchaseCap<T>): ID {
//         self.kiosk_id
//     }

//     /// Get the `Item_id` from the `PurchaseCap`.
//     public fun purchase_cap_item<T: key + store>(self: &PurchaseCap<T>): ID {
//         self.item_id
//     }

//     /// Get the `min_price` from the `PurchaseCap`.
//     public fun purchase_cap_min_price<T: key + store>(self: &PurchaseCap<T>): u64 {
//         self.min_price
//     }
// }

// #[test_only]
// module sui::kiosk_creature {
//     use sui::tx_context::{TxContext, sender};
//     use sui::object::{Self, UID};
//     use sui::transfer::transfer;
//     use sui::package::{Self, Publisher};

//     struct Creature has key, store { id: UID }
//     struct KIOSK_CREATURE has drop {}

//     // Create a publisher + 2 `Creature`s -> to sender
//     fun init(otw: KIOSK_CREATURE, ctx: &mut TxContext) {
//         transfer(package::claim(otw, ctx), sender(ctx))
//     }

//     public fun new_creature(ctx: &mut TxContext): Creature {
//         Creature { id: object::new(ctx) }
//     }

//     #[test_only]
//     public fun init_collection(ctx: &mut TxContext) {
//         init(KIOSK_CREATURE {}, ctx)
//     }

//     #[test_only]
//     public fun get_publisher(ctx: &mut TxContext): Publisher {
//         package::claim(KIOSK_CREATURE {}, ctx)
//     }

//     public fun return_creature(self: Creature) {
//         let Creature { id } = self;
//         object::delete(id)
//     }
// }

// #[test_only]
// module sui::kiosk_tests {
//     use sui::kiosk_creature::{Creature, new_creature, init_collection, get_publisher, return_creature};
//     use sui::test_scenario::{Self as ts};
//     use sui::kiosk::{Self, Kiosk, KioskOwnerCap, TransferPolicyCap};
//     use sui::package::Publisher;
//     use sui::transfer::{share_object, transfer};
//     use sui::tx_context;
//     use sui::sui::SUI;
//     use sui::object;
//     use sui::coin;
//     use sui::package;
//     use std::option;
//     use std::vector;

//     /// The price for a Creature.
//     const PRICE: u64 = 1000;

//     /// Addresses for the current testing suite.
//     fun folks(): (address, address) { (@0xA71CE, @0xB0B) }

//     #[test]
//     fun test_purchase_cap() {
//         let ctx = &mut tx_context::dummy();
//         let publisher = get_publisher(ctx);

//         let creature = new_creature(ctx);
//         let item_id = object::id(&creature);
//         let (kiosk, kiosk_cap) = kiosk::new(ctx);
//         let transfer_cap = kiosk::new_transfer_policy_cap(&publisher, ctx);

//         kiosk::place(&mut kiosk, &kiosk_cap, creature);

//         // create a PurchaseCap
//         let purchase_cap = kiosk::list_with_purchase_cap(&mut kiosk, &kiosk_cap, item_id, 10_000, ctx);

//         // use it right away to purchase a `Creature`
//         let (creature, transfer_request) = kiosk::purchase_with_cap(
//             &mut kiosk,
//             purchase_cap,
//             coin::mint_for_testing<SUI>(100_000, ctx)
//         );

//         let kiosk_id = object::id(&kiosk);
//         let (amount, from_id) = kiosk::allow_transfer(&transfer_cap, transfer_request);
//         let profits = kiosk::close_and_withdraw(kiosk, kiosk_cap, ctx);

//         assert!(amount == 100_000, 0);
//         assert!(kiosk_id == from_id, 1);
//         assert!(coin::value(&profits) == 100_000, 2);

//         kiosk::destroy_transfer_policy_cap(transfer_cap);
//         package::burn_publisher(publisher);
//         coin::burn_for_testing(profits);
//         return_creature(creature);
//     }

//     #[test]
//     fun test_purchase_cap_return() {
//         let ctx = &mut tx_context::dummy();

//         let creature = new_creature(ctx);
//         let item_id = object::id(&creature);
//         let (kiosk, kiosk_cap) = kiosk::new(ctx);

//         kiosk::place(&mut kiosk, &kiosk_cap, creature);

//         // create a PurchaseCap
//         let purchase_cap = kiosk::list_with_purchase_cap<Creature>(&mut kiosk, &kiosk_cap, item_id, 10_000, ctx);

//         kiosk::return_purchase_cap(&mut kiosk, purchase_cap);
//         let creature = kiosk::take(&mut kiosk, &kiosk_cap, item_id);
//         let profits = kiosk::close_and_withdraw(kiosk, kiosk_cap, ctx);

//         coin::burn_for_testing(profits);
//         return_creature(creature);
//     }

//     #[test]
//     fun test_placing() {
//         let (user, creator) = folks();
//         let test = ts::begin(creator);

//         // Creator creates a collection and gets a Publisher object.
//         init_collection(ts::ctx(&mut test));

//         // Creator creates a Kiosk and registers a type.
//         // No transfer policy set, TransferPolicyCap is frozen.
//         ts::next_tx(&mut test, creator); {
//             let pub = ts::take_from_address<Publisher>(&test, creator);
//             let ctx = ts::ctx(&mut test);
//             let (kiosk, kiosk_cap) = kiosk::new(ctx);
//             let allow_cap = kiosk::new_transfer_policy_cap<Creature>(&pub, ctx);

//             share_object(kiosk);
//             transfer(pub, creator);
//             sui::royalty::set_zero_policy(allow_cap);
//             transfer(kiosk_cap, creator);
//         };

//         // Get the TransferPolicyCap from the effects + Kiosk
//         let effects = ts::next_tx(&mut test, creator);
//         let cap_id = *vector::borrow(&ts::frozen(&effects), 0);
//         let kiosk_id = *vector::borrow(&ts::shared(&effects), 0);
//         let creature = new_creature(ts::ctx(&mut test));
//         let creature_id = object::id(&creature);

//         // Place an offer to sell a `creature` for a `PRICE`.
//         ts::next_tx(&mut test, creator); {
//             let kiosk = ts::take_shared_by_id<Kiosk>(&test, kiosk_id);
//             let kiosk_cap = ts::take_from_address<KioskOwnerCap>(&test, creator);

//             kiosk::place_and_list(
//                 &mut kiosk,
//                 &kiosk_cap,
//                 creature,
//                 PRICE
//             );

//             ts::return_shared(kiosk);
//             transfer(kiosk_cap, creator);
//         };

//         let effects = ts::next_tx(&mut test, creator);
//         assert!(ts::num_user_events(&effects) == 1, 0);

//         //
//         ts::next_tx(&mut test, user); {
//             let kiosk = ts::take_shared_by_id<Kiosk>(&test, kiosk_id);
//             let cap = ts::take_immutable_by_id<TransferPolicyCap<Creature>>(&test, cap_id);
//             let coin = coin::mint_for_testing<SUI>(PRICE, ts::ctx(&mut test));

//             // Is there a change the system can be tricked?
//             // Say, someone makes a purchase of 2 Creatures at the same time.
//             let (creature, request) = kiosk::purchase(&mut kiosk, creature_id, coin);
//             let (paid, from) = transfer_request::allow_transfer(&cap, request);

//             assert!(paid == PRICE, 0);
//             assert!(from == object::id(&kiosk), 0);

//             transfer(creature, user);
//             ts::return_shared(kiosk);
//             ts::return_immutable(cap);
//         };

//         ts::next_tx(&mut test, creator); {
//             let kiosk = ts::take_shared_by_id<Kiosk>(&test, kiosk_id);
//             let kiosk_cap = ts::take_from_address<KioskOwnerCap>(&test, creator);

//             let profits_1 = kiosk::withdraw(
//                 &mut kiosk,
//                 &kiosk_cap,
//                 option::some(PRICE / 2),
//                 ts::ctx(&mut test)
//             );

//             let profits_2 = kiosk::withdraw(
//                 &mut kiosk,
//                 &kiosk_cap,
//                 option::none(),
//                 ts::ctx(&mut test)
//             );

//             assert!(coin::value(&profits_1) == coin::value(&profits_2), 0);
//             transfer(profits_1, creator);
//             transfer(profits_2, creator);
//             transfer(kiosk_cap, creator);
//             ts::return_shared(kiosk);
//         };

//         ts::end(test);
//     }
// }
