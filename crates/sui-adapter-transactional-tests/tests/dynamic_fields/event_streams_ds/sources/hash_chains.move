/*
/// Module: event_streams_ds
*/
module event_streams_ds::hash_chains;

// For Move coding conventions, see
// https://docs.sui.io/concepts/sui-move-concepts/conventions

use sui::dynamic_field::{add, borrow, borrow_mut};
use std::bcs;
use sui::hash;

public struct Obj has key {
    id: object::UID,
}

public struct HashChainHead has copy, store, drop {
    /// Merkle root for all events in the current checkpoint.
    root: vector<u8>,
    /// Hash of the previous version of the head object.
    prev: vector<u8>,
}

entry fun myinit(ctx: &mut TxContext) {
    let mut id = object::new(ctx);
    let head = HashChainHead {
        root: vector::empty(),
        prev: vector::empty(),
    };
    add<u64, HashChainHead>(&mut id, 0, head);
    sui::transfer::transfer(Obj { id }, ctx.sender());
}

entry fun update(obj: &mut Obj) {
    let id = &mut obj.id;

    // Random fixed merkle root for benchmarks
    let new_element = vector::tabulate!(32, |_| 2);
    let head: &mut HashChainHead = borrow_mut(id, 0);
    add_to_hash_chain(new_element, head);
}

fun add_to_hash_chain(new_val: vector<u8>, head: &mut HashChainHead) {
    let prev_bytes = bcs::to_bytes(head);
    let prev = hash::blake2b256(&prev_bytes);
    head.prev = prev;
    head.root = new_val;
}

// Testing utilities & tests

#[test_only]
public fun init_for_testing(ctx: &mut TxContext) {
    myinit(ctx)
}

#[test_only]
public fun fetch(obj: &Obj): HashChainHead {
    let id = &obj.id;
    *borrow(id, 0)
}

use std::debug::print;

#[test]
fun test_hashing() {
    let mut head = HashChainHead {
        root: vector::empty(),
        prev: vector::empty(),
    };
    let fixed_new_val = vector::tabulate!(32, |_| 2);

    // Initial head
    print<HashChainHead>(&head);

    // Round 1
    add_to_hash_chain(fixed_new_val, &mut head);
    print<HashChainHead>(&head);

    // Round 2
    add_to_hash_chain(fixed_new_val, &mut head);
    print<HashChainHead>(&head);
}