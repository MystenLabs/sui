// // Copyright (c) Mysten Labs, Inc.
// // SPDX-License-Identifier: Apache-2.0
//
// /// Randomness objects can only be created, set or consumed. They cannot be created and consumed
// /// in the same transaction since it might allow validators include creat and use those objects
// /// *after* seeing the randomness they depend on.
// ///
// /// - On creation, the object contains the epoch in which it was created and a unique object id.
// ///
// /// - After the object creation transaction is committed, anyone can retrieve the BLS signature on
// ///   the message "randomness:id:epoch" from validators (signed using the Threshold-BLS key of that
// ///   epoch).
// ///
// /// - Anyone that can mutate the object can set the randomness of the object by supplying the BLS
// ///   signature. This operation verifies the signature and sets the value of the randomness object
// ///   to be the hash of the signature.
// ///
// ///   Note that there is exactly one signature that could pass this verification for an object,
// ///   thus, the only options the owner of the object has after retrieving the signature is to either
// ///   set the randomness or leave it unset. Applications that use Randomness objects must make sure
// ///   they handle both options (e.g., debit the user on object creation so even if the user aborts
// ///   depending on the randomness it received, the application is not harmed).
// ///
// /// - Once set, the actual randomness value can be read/consumed.
// ///
// ///
// /// This object can be used as a shared-/owned-object, as a field or a dynamic object field.
// /// See more details below.
// ///
// module sui::randomness {
//     use std::hash::sha3_256;
//     use std::option::{Self, Option};
//     use sui::object::{Self, UID, ID, id};
//     use sui::tx_context::{Self, TxContext};
//     use sui::transfer;
//     use sui::dynamic_object_field as dof;
//
//     /// Set is called with an invalid signature.
//     const EInvalidSignature: u64 = 0;
//     /// Already set object cannot be set again.
//     const EAlreadySetObject: u64 = 1;
//     /// Unset object cannot be consumed.
//     const EUnsetObject: u64 = 2;
//
//     const ChildRef: u8 = 0;
//
//     /// Randomness object, can only be created as a keyed object.
//     struct Randomness<phantom T> has key, store {
//         id: UID,
//         // The epoch in which it was created.
//         epoch: u64,
//         // The actual randomness, initially empty.
//         value: Option<vector<u8>>
//     }
//
//     /// Wrapper for using Randomness as a field,
//     /// that links to Randomness obj with ChildRef as a dynamic object field.
//     /// This object
//     struct RandomnessRef<phantom T> has store {
//         uid: UID,
//     }
//
//     fun new<T: drop>(ctx: &mut TxContext): Randomness<T> {
//         Randomness<T> {
//             id: object::new(ctx),
//             epoch: tx_context::epoch(ctx),
//             value: option::none(),
//         }
//     }
//     // Q - is there still a a way in which apps can create Randomness<> without creating a keyed object? i hope not
//
//     public fun create_and_transfer<T: drop>(_w: T, to: address, ctx: &mut TxContext): ID {
//         let r: Randomness<T> = new(ctx);
//         let id = id(&r);
//         transfer::transfer(r, to);
//         id
//     }
//
//     public fun create_as_shared<T: drop>(_w: T, ctx: &mut TxContext): ID {
//         let r: Randomness<T> = new(ctx);
//         let id = id(&r);
//         transfer::share_object(r);
//         id
//     }
//
//     public fun create_ref<T: drop>(_w: T, ctx: &mut TxContext): RandomnessRef<T> {
//         let r: Randomness<T> = new(ctx);
//         let uid = object::new(ctx);
//         dof::add(&mut uid, ChildRef, r);
//         RandomnessRef { uid }
//     }
//
//     public fun randomness<T>(rr: &RandomnessRef<T>): &Randomness<T> {
//         dof::borrow(&rr.uid, ChildRef)
//     }
//
//     public fun randomness_mut<T>(rr: &mut RandomnessRef<T>): &mut Randomness<T> {
//         dof::borrow_mut(&mut rr.uid, ChildRef)
//     }
//
//     /// Read the epoch of the object.
//     public fun epoch<T>(r: &Randomness<T>): u64 {
//         r.epoch
//     }
//
//     /// Read the current value of the object.
//     public fun value<T>(r: &Randomness<T>): &Option<vector<u8>> {
//         &r.value
//     }
//
//     entry public fun set<T>(r: &mut Randomness<T>, sig: vector<u8>) {
//         let _ = set_and_get(r, sig);
//     }
//     /// Owner(s) can use this function for setting the randomness.
//     public fun set_and_get<T>(r: &mut Randomness<T>, sig: vector<u8>): vector<u8> {
//         assert!(option::is_none(&r.value), EAlreadySetObject);
//         // TODO: construct 'msg'
//         // TODO: next api is not available yet.
//         // assert!(verify_tbls_signature(self.epoch, msg, sig), EInvalidSignature);
//         let hashed = sha3_256(sig);
//         r.value = option::some(hashed);
//         hashed
//     }
//
//     /// Delete the object and retrieve the randomness (in case of an owned object).
//     public fun destroy_ref<T>(rr: RandomnessRef<T>) {
//         let r: Randomness<T> = dof::remove(&mut rr.uid, ChildRef);
//         destroy(r);
//         let RandomnessRef { uid } = rr;
//         object::delete(uid);
//     }
//
//     /// Delete the object and retrieve the randomness (in case of an owned object).
//     public fun destroy<T>(r: Randomness<T>) {
//         let Randomness { id, epoch: _, value: _ } = r;
//         object::delete(id);
//     }
// }
//
//
// //////////////////////////////////////////////////////////////////////
// // Examples //
//
// // scratchcard that uses a shared obj for the reward, and randomness_ref
// module sui::scratchcard_example1 {
//     use std::vector;
//     use sui::balance::{Self, Balance, zero};
//     use sui::coin::{Self, Coin};
//     use sui::object::{Self, UID};
//     use sui::randomness::{Self, RandomnessRef, destroy_ref};
//     use sui::sui::SUI;
//     use sui::tx_context::{Self, TxContext};
//     use sui::transfer;
//
//     // Make sure only the current module can access Randomness it creates.
//     struct LOTTERY_LOCK has drop {}
//
//     /// Shared object, singelton
//     struct Lottery has key {
//         id: UID,
//         balance: Balance<SUI>,
//     }
//
//     struct Ticket has key {
//         id: UID,
//         randomness_ref: RandomnessRef<LOTTERY_LOCK>,
//     }
//
//     fun init(ctx: &mut TxContext) {
//         let lottery = Lottery {
//             id: object::new(ctx),
//             balance: zero(),
//         };
//         sui::transfer::share_object(lottery);
//     }
//
//     // Ticket can win with probability 1%, and then receive 100 tokens.
//     entry fun buy_ticket(lottery: &mut Lottery, coin: Coin<SUI>, ctx: &mut TxContext) {
//         assert!(coin::value(&coin) == 1, 0);
//         balance::join(&mut lottery.balance, coin::into_balance(coin));
//         let t = Ticket {
//             id: object::new(ctx),
//             randomness_ref: randomness::create_ref(LOTTERY_LOCK {}, ctx),
//         };
//         transfer::transfer(t, tx_context::sender(ctx));
//     }
//
//     // takes a reward, if there is enough (else, can be taken later)
//     entry fun use_ticket(lottery: &mut Lottery, ticket: Ticket, sig: vector<u8>, ctx: &mut TxContext) {
//         let random_bytes = randomness::set_and_get(randomness::randomness_mut(&mut ticket.randomness_ref), sig);
//         let first_byte = vector::borrow(&random_bytes, 0);
//         if (*first_byte % 100 == 0) {
//             assert!(balance::value(&lottery.balance) > 99, 0);
//             let coin = coin::from_balance(balance::split(&mut lottery.balance, 100), ctx);
//             sui::pay::keep(coin, ctx);
//         };
//         let Ticket { id, randomness_ref} = ticket;
//         destroy_ref(randomness_ref);
//         object::delete(id);
//     }
// }
//
// ///////////////////////////////////////////////////////
// // example that uses id for related objects
//
// module sui::scratchcard_owned_example1 {
//     use std::vector;
//     use sui::balance::{Self, Balance};
//     use sui::coin::{Self, Coin};
//     use sui::object::{Self, UID, ID, id};
//     use sui::randomness::RandomnessRef;
//     use sui::randomness;
//     use sui::sui::SUI;
//     use sui::tx_context::{Self, TxContext};
//
//     // Make sure only the current module can access Randomness it creates.
//     struct LOTTERY_LOCK has drop {}
//
//     /// Shared object
//     struct Lottery has key {
//         id: UID,
//         balance: Balance<SUI>,
//         creator: address,
//     }
//
//     struct Ticket has key {
//         id: UID,
//         lottery_id: ID,
//         creator: address,
//         randomness_ref: RandomnessRef<LOTTERY_LOCK>,
//     }
//
//     entry fun create(coin: Coin<SUI>, ctx: &mut TxContext) {
//         let lottery = Lottery {
//             id: object::new(ctx),
//             balance: coin::into_balance(coin),
//             creator: tx_context::sender(ctx),
//         };
//         sui::transfer::share_object(lottery);
//     }
//
//     public fun balance(lottery: &Lottery): u64 {
//         balance::value(&lottery.balance)
//     }
//
//     public fun creator(lottery: &Lottery): address {
//         lottery.creator
//     }
//
//     // Buyer gets a randomness object and a ticket that associates the lottery with the randomness, and makes sure that
//     // the creator received the payment.
//     entry fun buy_ticket(lottery_id: ID, creator: address, coin: Coin<SUI>, ctx: &mut TxContext) {
//         assert!(coin::value(&coin) == 1, 0);
//         sui::transfer::transfer(coin, creator);
//         let randomness_ref = randomness::create_ref(LOTTERY_LOCK {}, ctx);
//         let ticket = Ticket {
//             id: object::new(ctx),
//             lottery_id,
//             creator,
//             randomness_ref,
//         };
//         sui::transfer::transfer(ticket, tx_context::sender(ctx));
//     }
//
//     // Can be called also after all the reward was taken.
//     entry fun use_ticket(lottery: &mut Lottery, ticket: Ticket, sig: vector<u8>, ctx: &mut TxContext) {
//         assert!(lottery.creator == ticket.creator, 5);
//         assert!(id(lottery) == ticket.lottery_id, 5);
//         let random_bytes = randomness::set_and_get(randomness::randomness_mut(&mut ticket.randomness_ref), sig);
//         let first_byte = vector::borrow(&random_bytes, 0);
//         if (*first_byte % 100 == 0) {
//             let coin = coin::from_balance(balance::split(&mut lottery.balance, 100), ctx);
//             sui::pay::keep(coin, ctx);
//         };
//         let Ticket { id, lottery_id:_, creator:_, randomness_ref} = ticket;
//         randomness::destroy_ref(randomness_ref);
//         object::delete(id);
//     }
// }
//
//
//
// ////////////////////////////////////
//
// // example of a lottery (1 out of n) using shared obj and randomness shared obj
// module sui::lottery_example1 {
//     use std::vector;
//     use sui::balance::{Self, Balance, zero};
//     use sui::coin::{Self, Coin};
//     use sui::object::{Self, UID, ID, id};
//     use sui::randomness::{Self, Randomness};
//     use sui::sui::SUI;
//     use sui::tx_context::{Self, TxContext};
//     use std::option;
//     use std::option::Option;
//
//     // Make sure only the current module can access Randomness it creates.
//     struct LOTTERY_LOCK has drop {}
//
//     /// Shared object
//     struct Lottery has key {
//         id: UID,
//         balance: Balance<SUI>,
//         participants: u8,
//         randomness_id: Option<ID>,
//     }
//
//     struct Ticket has key {
//         id: UID,
//         lottery_id: ID,
//         participant_id: u8,
//     }
//
//     entry fun create(ctx: &mut TxContext) {
//         let lottery = Lottery {
//             id: object::new(ctx),
//             balance: zero(),
//             participants: 0,
//             randomness_id: option::none(),
//         };
//         sui::transfer::share_object(lottery);
//     }
//
//     entry fun buy_ticket(lottery: &mut Lottery, coin: Coin<SUI>, ctx: &mut TxContext) {
//         assert!(option::is_none(&lottery.randomness_id), 1);
//         assert!(lottery.participants < 100, 0); // just to simplify the modulo below
//         assert!(coin::value(&coin) == 1, 0);
//         balance::join(&mut lottery.balance, coin::into_balance(coin));
//         let ticket = Ticket {
//             id: object::new(ctx),
//             lottery_id: id(lottery),
//             participant_id: lottery.participants,
//         };
//         lottery.participants = lottery.participants + 1;
//         sui::transfer::transfer(ticket, tx_context::sender(ctx));
//     }
//
//     // Stop selling tickets and create a randomness that will determine the winner.
//     entry fun close(lottery: &mut Lottery, ctx: &mut TxContext) {
//         let randomness_id = randomness::create_as_shared(LOTTERY_LOCK {}, ctx);
//         option::fill(&mut lottery.randomness_id, randomness_id);
//     }
//
//     entry fun use_ticket(lottery: &mut Lottery, randomness: &Randomness<LOTTERY_LOCK>, ticket: Ticket, ctx: &mut TxContext) {
//         assert!(option::is_some(randomness::value(randomness)), 11);
//         assert!(*option::borrow(&lottery.randomness_id) == id(randomness), 13);
//         let random_bytes = option::borrow(randomness::value(randomness));
//         let first_byte = vector::borrow(random_bytes, 0);
//         if (*first_byte % lottery.participants == ticket.participant_id) {
//             let amount = balance::value(&lottery.balance);
//             let coin = coin::from_balance(balance::split(&mut lottery.balance, amount), ctx);
//             sui::pay::keep(coin, ctx);
//         };
//         let Ticket { id, lottery_id:_, participant_id:_  } = ticket;
//         object::delete(id);
//     }
// }
