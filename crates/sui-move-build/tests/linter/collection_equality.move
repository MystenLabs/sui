// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::test {
    use sui::bag::Bag;
    use sui::table::Table;
    use sui::table_vec::TableVec;



    public fun bag_neq(bag1: &Bag, bag2: &Bag): bool {
        bag1 == bag2
    }

    public fun table_neq(table1: &Table<u64, u64>, table2: &Table<u64, u64>): bool {
        table1 != table2
    }

    public fun table_vec_eq(table1: &TableVec<u64>, table2: &TableVec<u64>): bool {
        table1 == table2
    }


}
