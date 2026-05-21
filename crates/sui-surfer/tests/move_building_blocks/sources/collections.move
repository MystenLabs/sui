// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Exercises the framework collection types via a single shared object holding
/// one of each. Monomorphic (concrete key/value types) so the surfer can call
/// the entry functions without type arguments.
module move_building_blocks::collections {
    use sui::bag::{Self, Bag};
    use sui::linked_table::{Self, LinkedTable};
    use sui::table_vec::{Self, TableVec};
    use sui::vec_map::{Self, VecMap};
    use sui::vec_set::{Self, VecSet};

    public struct Collections has key, store {
        id: UID,
        map: VecMap<u64, u64>,
        set: VecSet<u64>,
        bag: Bag,
        tvec: TableVec<u64>,
        ltable: LinkedTable<u64, u64>,
    }

    public fun create(ctx: &mut TxContext) {
        let collections = Collections {
            id: object::new(ctx),
            map: vec_map::empty(),
            set: vec_set::empty(),
            bag: bag::new(ctx),
            tvec: table_vec::empty(ctx),
            ltable: linked_table::new(ctx),
        };
        transfer::share_object(collections);
    }

    public fun map_insert(c: &mut Collections, key: u64, value: u64) {
        if (!c.map.contains(&key)) {
            c.map.insert(key, value);
        }
    }

    public fun map_remove(c: &mut Collections, key: u64) {
        if (c.map.contains(&key)) {
            let (_, _) = c.map.remove(&key);
        }
    }

    public fun set_insert(c: &mut Collections, key: u64) {
        if (!c.set.contains(&key)) {
            c.set.insert(key);
        }
    }

    public fun set_remove(c: &mut Collections, key: u64) {
        if (c.set.contains(&key)) {
            c.set.remove(&key);
        }
    }

    public fun bag_add(c: &mut Collections, key: u64, value: u64) {
        if (!c.bag.contains(key)) {
            c.bag.add(key, value);
        }
    }

    public fun bag_remove(c: &mut Collections, key: u64) {
        if (c.bag.contains(key)) {
            let _: u64 = c.bag.remove(key);
        }
    }

    public fun tvec_push(c: &mut Collections, value: u64) {
        c.tvec.push_back(value);
    }

    public fun tvec_pop(c: &mut Collections) {
        if (!c.tvec.is_empty()) {
            let _ = c.tvec.pop_back();
        }
    }

    public fun ltable_push(c: &mut Collections, key: u64, value: u64) {
        if (!c.ltable.contains(key)) {
            c.ltable.push_back(key, value);
        }
    }

    public fun ltable_pop(c: &mut Collections) {
        if (!c.ltable.is_empty()) {
            let (_, _) = c.ltable.pop_front();
        }
    }
}
