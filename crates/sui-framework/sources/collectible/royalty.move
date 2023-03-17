// // Copyright (c) Mysten Labs, Inc.
// // SPDX-License-Identifier: Apache-2.0

// /// This module implements a set of basic primitives for Kiosk's
// /// Transfer Policies to improve discoverability and serve as a
// /// base for building on top.
// module sui::royalty {
//     use std::option::{Self, Option};
//     use sui::transfer_request::{Self, TransferRequest, TransferPolicyCap};
//     use sui::tx_context::{sender, TxContext};
//     use sui::balance::{Self, Balance};
//     use sui::object::{Self, UID, ID};
//     use sui::coin::{Self, Coin};
//     use sui::transfer;
//     use sui::sui::SUI;
//     use sui::event;

//     /// Utility constant to calculate the percentage of price to require.
//     const MAX_AMOUNT: u16 = 10_000;

//     /// For when trying to create a new RoyaltyPolicy with more than 100%.
//     ///  Or when trying to withdraw more than stored `balance`.
//     const EIncorrectAmount: u64 = 0;

//     /// A transfer policy for a single type `T` which collects a certain
//     /// fee from the `kiosk` deals and stores them for the policy issuer.
//     struct RoyaltyPolicy<phantom T: key + store> has key, store {
//         id: UID,
//         /// The `TransferPolicyCap` for the `T` which is used to call
//         /// the `kiosk::allow_transfer` and allow the trade.
//         cap: TransferPolicyCap<T>,
//         /// Percentage of the trade amount which is required for the
//         /// transfer approval. Denominated in basis points.
//         /// - 10_000 = 100%
//         /// - 100 = 1%
//         /// - 1 = 0.01%
//         amount_bp: u16,
//         /// Accumulated balance - the owner of the Policy can withdraw
//         /// at any time.
//         balance: Balance<SUI>,
//         /// Store creator address for visibility and discoverability purposes
//         owner: address
//     }

//     /// A Capability that grants the bearer the ability to change the amount of
//     /// royalties collected as well as to withdraw from the `policy.balance`.
//     struct RoyaltyCollectorCap<phantom T: key + store> has key, store {
//         id: UID,
//         /// Purely cosmetic and discovery field.
//         /// There should be only one Policy for the type `T` (although it
//         /// is not enforced anywhere by default).
//         policy_id: ID
//     }

//     // === Events ===

//     /// Event: fired when a new policy has been created for the type `T`. Meaning
//     /// that in most of the cases where a `RoyaltyPolicy` is a shared object, it
//     /// can be used in the `sui::royalty::pay` function.
//     struct PolicyCreated<phantom T: key + store> has copy, drop {
//         id: ID,
//     }

//     /// Event: fired when a free-for-all policy was issued for `T`. Since the object
//     /// is immutably shared, everyone in the ecosystem can discover and use this
//     /// object to allow `TransferRequest<T>`.
//     struct ZeroPolicyCreated<phantom T: key + store> has copy, drop {
//         id: ID
//     }

//     // === Public / Everyone ===

//     /// Perform a Royalty payment and unblock the transfer by consuming
//     /// the `TransferRequest` "hot potato". The `T` here type-locks the
//     /// RoyaltyPolicy and TransferRequest making it impossible to call this
//     /// function on a wrong type.
//     public fun pay<T: key + store>(
//         policy: &mut RoyaltyPolicy<T>,
//         transfer_request: TransferRequest<T>,
//         coin: &mut Coin<SUI>
//     ) {
//         let (paid, _from) = kiosk::allow_transfer(&policy.cap, transfer_request);
//         let amount = (((paid as u128) * (policy.amount_bp as u128) / (MAX_AMOUNT as u128)) as u64);

//         let royalty_payment = balance::split(coin::balance_mut(coin), amount);
//         balance::join(&mut policy.balance, royalty_payment);
//     }

//     // === Creator only ===

//     /// A special function used to explicitly indicate that all of the
//     /// trades can be performed freely. To achieve that, the `TransferPolicyCap`
//     /// is immutably shared making it available for free use by anyone on the network.
//     entry public fun set_zero_policy<T: key + store>(cap: TransferPolicyCap<T>) {
//         event::emit(ZeroPolicyCreated<T> { id: object::id(&cap) });
//         transfer::freeze_object(cap)
//     }

//     /// Create new `RoyaltyPolicy` for the `T` and require an `amount`
//     /// percentage of the trade amount for the transfer to be approved.
//     public fun new_royalty_policy<T: key + store>(
//         cap: TransferPolicyCap<T>,
//         amount_bp: u16,
//         ctx: &mut TxContext
//     ): (RoyaltyPolicy<T>, RoyaltyCollectorCap<T>) {
//         assert!(amount_bp <= MAX_AMOUNT && amount_bp != 0, EIncorrectAmount);

//         let policy = RoyaltyPolicy {
//             cap, amount_bp,
//             id: object::new(ctx),
//             owner: sender(ctx),
//             balance: balance::zero(),
//         };
//         let id = object::id(&policy);
//         let cap = RoyaltyCollectorCap {
//             id: object::new(ctx),
//             policy_id: id
//         };

//         event::emit(PolicyCreated<T> { id });

//         (policy, cap)
//     }

//     /// Change the amount in the `RoyaltyPolicy`.
//     public fun set_amount<T: key + store>(
//         policy: &mut RoyaltyPolicy<T>,
//         _cap: &RoyaltyCollectorCap<T>,
//         amount: u16,
//     ) {
//         assert!(amount > 0 && amount <= MAX_AMOUNT, EIncorrectAmount);
//         policy.amount_bp = amount
//     }

//     /// Change the `owner` field to the tx sender.
//     public fun set_owner<T: key + store>(
//         policy: &mut RoyaltyPolicy<T>,
//         _cap: &RoyaltyCollectorCap<T>,
//         ctx: &TxContext
//     ) {
//         policy.owner = sender(ctx)
//     }

//     /// Withdraw `amount` of SUI from the `policy.balance`.
//     public fun withdraw<T: key + store>(
//         policy: &mut RoyaltyPolicy<T>,
//         _cap: &RoyaltyCollectorCap<T>,
//         amount: Option<u64>,
//         ctx: &mut TxContext
//     ): Coin<SUI> {
//         let available = balance::value(&policy.balance);
//         let amount = if (option::is_some(&amount)) {
//             option::destroy_some(amount)
//         } else {
//             available
//         };

//         assert!(amount <= available, EIncorrectAmount);
//         coin::take(&mut policy.balance, amount, ctx)
//     }

//     /// Unpack a RoyaltyPolicy object if it is not shared (!!!) and
//     /// return the `TransferPolicyCap` and the remaining balance.
//     public fun destroy_and_withdraw<T: key + store>(
//         policy: RoyaltyPolicy<T>,
//         cap: RoyaltyCollectorCap<T>,
//         ctx: &mut TxContext
//     ): (TransferPolicyCap<T>, Coin<SUI>) {
//         let RoyaltyPolicy { id, amount_bp: _, owner: _, cap: transfer_cap, balance } = policy;
//         let RoyaltyCollectorCap { id: cap_id, policy_id: _ } = cap;

//         object::delete(cap_id);
//         object::delete(id);

//         (transfer_cap, coin::from_balance(balance, ctx))
//     }

//     // === Field access ===

//     /// Get the `amount` field.
//     public fun amount<T: key + store>(self: &RoyaltyPolicy<T>): u16 {
//         self.amount_bp
//     }

//     /// Get the `balance` field.
//     public fun balance<T: key + store>(self: &RoyaltyPolicy<T>): u64 {
//         balance::value(&self.balance)
//     }
// }
