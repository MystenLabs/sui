// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Bench a few options for event streams

//# init --addresses a=0x0 --accounts A

//# publish
module a::m {

use sui::dynamic_field::{add, borrow_mut};
use std::bcs;
use sui::hash;

public struct Obj has key {
    id: object::UID,
}

public struct HashChainHead has store {
    /// Merkle root for all events in the current checkpoint.
    root: vector<u8>,
    /// Hash of the previous version of the head object.
    prev: vector<u8>,
}

public struct MerkleMountainRange has store {
    // Index i contains either a merkle_root_of_height_2^i or an empty vector. 
    digest_vec: vector<vector<u8>>,
}

entry fun myinit(ctx: &mut TxContext) {
    let mut id = object::new(ctx);
    let head = HashChainHead {
        root: vector::empty(),
        prev: vector::empty(),
    };
    let mmr_small = MerkleMountainRange {
        digest_vec: vector::empty(),
    };
    // Simulate a large mmr with ~2^30 elements
    let mmr_large = MerkleMountainRange {
        digest_vec: vector::tabulate!(30, |x| {
            if (x == 0) { // Half the elements are empty
                vector::empty()
            } else { // The other half are full
                vector::tabulate!(32, |_| x as u8)
            }
        }),
    };
    add<u64, HashChainHead>(&mut id, 0, head);
    add<u64, MerkleMountainRange>(&mut id, 1, mmr_small);
    add<u64, MerkleMountainRange>(&mut id, 2, mmr_large);
    sui::transfer::transfer(Obj { id }, ctx.sender());
}

// Hash chains
entry fun hash_chain_bench(obj: &mut Obj) {
    let id = &mut obj.id;

    // Random fixed merkle root for benchmarks
    let new_element = vector::tabulate!(32, |_| 2);
    let head: &mut HashChainHead = borrow_mut(id, 0);

    let num_elements = 100;
    let mut i = 0;
    while (i < num_elements) {
        add_to_hash_chain(new_element, head);
        i = i + 1;
    }
}

fun add_to_hash_chain(new_val: vector<u8>, head: &mut HashChainHead) {
    let prev_bytes = bcs::to_bytes(head);
    let prev = hash::blake2b256(&prev_bytes);
    head.prev = prev;
    head.root = new_val;
}

entry fun mmr_bench(obj: &mut Obj, test_id: u64) {
    assert!(test_id == 1 || test_id == 2);
    let id = &mut obj.id;

    // A fixed merkle root for benchmarks
    let new_element = vector::tabulate!(32, |_| 2);
    let mmr: &mut MerkleMountainRange = borrow_mut(id, test_id);

    // In order to figure out the avg cost, we add a lot of elements
    let num_elements = 100;
    let mut i = 0;
    while (i < num_elements) {
        add_to_mmr(new_element, mmr);
        i = i + 1;
    }
}

fun add_to_mmr(new_val: vector<u8>, mmr: &mut MerkleMountainRange) {
    let mut i = 0;
    let mut cur = new_val;
    while (i < vector::length(&mmr.digest_vec)) {
        let r = vector::borrow_mut(&mut mmr.digest_vec, i);
        if (r.is_empty()) {
            *r = cur;
            return
        } else {
            cur = hash_two_to_one(r, cur);
            *r = vector::empty();
        };
        i = i + 1;
    };

    // Vector length insufficient. Increase by 1.
    mmr.digest_vec.push_back(cur);
}

// TODO: Add a prefix to distinguish inner hashes?
fun hash_two_to_one(e1: &mut vector<u8>, e2: vector<u8>): vector<u8> {
    e1.append(e2);
    hash::blake2b256(e1)
}

}

//# run a::m::myinit --sender A

//# bench a::m::hash_chain_bench --sender A --args object(2,0)

//# bench a::m::mmr_bench --sender A --args object(2,0) 1

//# bench a::m::mmr_bench --sender A --args object(2,0) 2