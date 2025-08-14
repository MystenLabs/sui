// Merkle Mountain Ranges
module event_streams_ds::mmr;

use sui::dynamic_field::{add, borrow_mut};
use sui::hash;
use sui::bcs;

public struct Obj has key {
    id: object::UID,
}

public struct MerkleMountainRange has copy, store, drop {
    // Index i contains either a merkle_root_of_height_2^i or an empty vector. 
    digest_vec: vector<vector<u8> >,
}

fun init_mmr(): MerkleMountainRange {
    MerkleMountainRange {
        digest_vec: vector::empty(),
    }
}

entry fun myinit(ctx: &mut TxContext) {
    let mut id = object::new(ctx);
    let head = init_mmr();
    add<u64, MerkleMountainRange>(&mut id, 0, head);
    sui::transfer::transfer(Obj { id }, ctx.sender());
}

entry fun update(obj: &mut Obj) {
    let id = &mut obj.id;

    // Random fixed merkle root for benchmarks
    let new_element = vector::tabulate!(32, |_| 2);
    let mmr: &mut MerkleMountainRange = borrow_mut(id, 0);
    add_to_mmr(new_element, mmr);
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
            cur = hash_two_to_one_via_bcs(*r, cur);
            *r = vector::empty();
        };
        i = i + 1;
    };

    // Vector length insufficient. Increase by 1.
    mmr.digest_vec.push_back(cur);
}

// fun hash_two_to_one(e1: &mut vector<u8>, e2: vector<u8>): vector<u8> {
//     e1.append(e2);
//     hash::blake2b256(e1)
// }

// fun hash_two_to_one_owned(e1: vector<u8>, e2: vector<u8>): vector<u8> {
//     let mut e3 = vector::empty();
//     e3.append(e1);
//     e3.append(e2);
//     hash::blake2b256(&e3)
// }

public struct Tmp has drop {
    e1: vector<u8>,
    e2: vector<u8>,
}

// TODO: Add a prefix to distinguish inner hashes?
fun hash_two_to_one_via_bcs(e1: vector<u8>, e2: vector<u8>): vector<u8> {
    let tmp = Tmp {
        e1: e1,
        e2: e2,
    };
    let tmp_bytes = bcs::to_bytes(&tmp);
    hash::blake2b256(&tmp_bytes)
}

use std::debug::print;

#[test]
fun test_mmr_addition() {
    let mut mmr = init_mmr();
    let fixed_new_val = vector::tabulate!(32, |_| 2);

    // Initial head
    assert!(vector::all!(&mmr.digest_vec, |x| x.is_empty()));

    // Round 1
    add_to_mmr(fixed_new_val, &mut mmr);
    assert!(vector::map_ref!(&mmr.digest_vec, |x| x.is_empty()) == 
            vector[false]);

    // Round 2
    add_to_mmr(fixed_new_val, &mut mmr);
    assert!(vector::map_ref!(&mmr.digest_vec, |x| x.is_empty()) == 
            vector[true, false]);

    // Round 3
    add_to_mmr(fixed_new_val, &mut mmr);
    assert!(vector::map_ref!(&mmr.digest_vec, |x| x.is_empty()) == 
            vector[false, false]);

    // Round 4
    add_to_mmr(fixed_new_val, &mut mmr);
    assert!(vector::map_ref!(&mmr.digest_vec, |x| x.is_empty()) == 
            vector[true, true, false]);
    print<MerkleMountainRange>(&mmr);

    let x = hash_two_to_one_via_bcs(fixed_new_val, fixed_new_val);
    let y = hash_two_to_one_via_bcs(x, x);
    assert!(mmr.digest_vec[2] == y);
}
