// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module big_vector::big_vector_tests {
    use big_vector::big_vector::{Self as bv, BigVector};

    #[test]
    fun test_destroy_empty() {
        let bv = empty_for_test();

        assert!(bv.length() == 0, 0);
        assert!(bv.preorder_keys() == vector[], 0);
        assert!(bv.inorder_values() == vector[], 0);

        bv.destroy_empty()
    }

    #[test]
    #[expected_failure(abort_code = bv::ENotEmpty)]
    fun test_destroy_non_empty() {
        let mut bv = empty_for_test();
        bv.insert(42, 1);
        bv.destroy_empty();
    }

    #[test]
    fun test_drop() {
        let mut bv = empty_for_test();
        bv.insert(42, 1);
        bv.drop();
    }

    #[test]
    #[expected_failure(abort_code = bv::EExists)]
    fun test_duplicate_key() {
        let mut bv = empty_for_test();
        bv.insert(42, 1);
        bv.insert(42, 2);
        bv.drop()
    }

    #[test]
    fun test_flat_tree() {
        let mut bv = empty_for_test();
        bv.insert(42, 1);
        bv.insert(21, 2);

        assert!(bv.length() == 2, 0);
        assert!(bv.preorder_keys() == vector[vector[21, 42]], 0);
        assert!(bv.inorder_values() == vector[vector[2, 1]], 0);

        bv.drop()
    }

    #[test]
    fun test_branching() {
        let mut bv = empty_for_test();
        bv.insert(43, 1);
        bv.insert(21, 3);
        bv.insert(32, 2);

        assert!(bv.length() == 3, 0);
        assert!(
            bv.preorder_keys() ==
            vector[
                vector[43],
                vector[21, 32], vector[43],
            ],
            0
        );

        assert!(
            bv.inorder_values() ==
            vector[
                vector[3, 2],
                vector[1],
            ],
            0
        );

        bv.drop()
    }

    #[test]
    fun test_branch_multiple() {
        let bv = filled_for_test();
        assert!(bv.length() == 10, 0);

        assert!(
            bv.preorder_keys() ==
            vector[
                vector[9, 15],

                vector[7],
                vector[5],
                vector[7, 8],

                vector[10],
                vector[9],
                vector[10],

                vector[17, 20],
                vector[15],
                vector[17, 19],
                vector[20, 22],
            ],
            0
        );

        assert!(
            bv.inorder_values() ==
            vector[
                vector[5],
                vector[7, 8],
                vector[9],
                vector[10],
                vector[15],
                vector[17, 19],
                vector[20, 22],
            ],
            0
        );

        bv.drop();
    }

    #[test]
    fun test_contains() {
        let bv = filled_for_test();

        assert!(bv.contains(5), 0);
        assert!(bv.contains(7), 0);
        assert!(bv.contains(8), 0);
        assert!(bv.contains(9), 0);
        assert!(bv.contains(10), 0);
        assert!(bv.contains(15), 0);
        assert!(bv.contains(17), 0);
        assert!(bv.contains(19), 0);
        assert!(bv.contains(20), 0);
        assert!(bv.contains(22), 0);

        assert!(!bv.contains(6), 0);
        assert!(!bv.contains(11), 0);
        assert!(!bv.contains(16), 0);
        assert!(!bv.contains(21), 0);

        bv.drop()
    }

    #[test]
    /// Perform a removal that doesn't require any redistribution or
    /// merging afterwards.
    fun test_remove_simple() {
        let mut bv = filled_for_test();

        let val = bv.remove(17);
        assert!(val == 17u8, 0);

        assert!(
            bv.preorder_keys() ==
            vector[
                vector[9, 15],

                vector[7],
                vector[5],
                vector[7, 8],

                vector[10],
                vector[9],
                vector[10],

                vector[17, 20],
                vector[15],
                vector[19],
                vector[20, 22],

            ],
            0
        );

        assert!(
            bv.inorder_values() ==
            vector[
                vector[5],
                vector[7, 8],
                vector[9],
                vector[10],
                vector[15],
                vector[19],
                vector[20, 22],
            ],
            0
        );

        bv.drop()
    }

    #[test]
    /// Removing from a root that is just a leaf node, (meaning it
    /// can't redistribute or merge with neighbours, even if it wanted
    /// to).
    fun test_remove_root_leaf() {
        let mut bv: BigVector<u8> = bv::empty(
            /* max_slice_size */ 4,
            /* max_fan_out */ 4,
            &mut tx_context::dummy(),
        );

        bv.insert(1, 1);
        bv.insert(2, 2);
        bv.insert(3, 3);
        bv.insert(4, 4);

        assert!(bv.length() == 4, 0);
        assert!(bv.preorder_keys() == vector[vector[1, 2, 3, 4]], 0);
        assert!(bv.inorder_values() == vector[vector[1, 2, 3, 4]], 0);

        assert!(bv.remove(2) == 2, 0);
        assert!(bv.length() == 3, 0);
        assert!(bv.preorder_keys() == vector[vector[1, 3, 4]], 0);
        assert!(bv.inorder_values() == vector[vector[1, 3, 4]], 0);

        assert!(bv.remove(4) == 4, 0);
        assert!(bv.length() == 2, 0);
        assert!(bv.preorder_keys() == vector[vector[1, 3]], 0);
        assert!(bv.inorder_values() == vector[vector[1, 3]], 0);

        assert!(bv.remove(1) == 1, 0);
        assert!(bv.length() == 1, 0);
        assert!(bv.preorder_keys() == vector[vector[3]], 0);
        assert!(bv.inorder_values() == vector[vector[3]], 0);

        assert!(bv.remove(3) == 3, 0);
        assert!(bv.length() == 0, 0);
        assert!(bv.preorder_keys() == vector[], 0);
        assert!(bv.inorder_values() == vector[], 0);

        bv.drop()
    }

    #[test]
    /// Removing from enough elements such that the root node is
    /// replaced with its only child.
    fun test_remove_root_node() {
        let mut bv = empty_for_test();
        bv.insert(1, 1);
        bv.insert(3, 3);
        bv.insert(2, 2);
        bv.insert(4, 4);

        assert!(bv.length() == 4, 0);

        assert!(
            bv.preorder_keys() ==
            vector[
                vector[3],
                vector[1, 2],
                vector[3, 4],
            ],
            0
        );

        assert!(
            bv.inorder_values() ==
            vector[
                vector[1, 2],
                vector[3, 4],
            ],
            0
        );

        assert!(bv.remove(1) == 1, 0);
        assert!(bv.remove(2) == 2, 0);
        assert!(bv.remove(3) == 3, 0);

        assert!(bv.length() == 1, 0);
        assert!(bv.preorder_keys() == vector[vector[4]], 0);
        assert!(bv.inorder_values() == vector[vector[4]], 0);

        bv.drop()
    }

    #[test]
    /// Removal that caused a redistribution, stealing from the left
    /// neighbour
    fun test_remove_redistribute_left() {
        let mut bv = empty_for_test();

        bv.insert(1, 1);
        bv.insert(3, 3);
        bv.insert(2, 2);

        assert!(bv.remove(3) == 3, 0);
        assert!(bv.length() == 2, 0);

        assert!(
            bv.preorder_keys() ==
            vector[
                vector[2],
                vector[1],
                vector[2],
            ],
            0
        );

        assert!(
            bv.inorder_values() ==
            vector[vector[1], vector[2]],
            0
        );

        bv.drop()
    }

    #[test]
    /// Removal that caused a redistribution, stealing from the right
    /// neighbour
    fun test_remove_redistribute_right() {
        let mut bv = empty_for_test();

        bv.insert(1, 1);
        bv.insert(3, 3);
        bv.insert(4, 4);

        assert!(bv.remove(1) == 1, 0);
        assert!(bv.length() == 2, 0);

        assert!(
            bv.preorder_keys() ==
            vector[
                vector[4],
                vector[3],
                vector[4],
            ],
            0
        );

        assert!(
            bv.inorder_values() ==
            vector[vector[3], vector[4]],
            0
        );

        bv.drop()
    }

    #[test]
    /// Removal that caused a merge with the left neighbour.
    fun test_remove_merge_left() {
        // We need a slightly bigger leaf size to perform a
        // non-trivial merge.
        let mut bv: BigVector<u8> = bv::empty(
            /* max_slice_size */ 4,
            /* max_fan_out */ 4,
            &mut tx_context::dummy(),
        );

        bv.insert(1, 1);
        bv.insert(2, 2);
        bv.insert(4, 4);
        bv.insert(5, 5);
        bv.insert(3, 3);

        assert!(bv.length() == 5, 0);
        assert!(bv.depth() == 1, 0);

        assert!(
            bv.preorder_keys() ==
            vector[
                vector[4],
                vector[1, 2, 3],
                vector[4, 5],
            ],
            0
        );

        assert!(
            bv.inorder_values() ==
            vector[
                vector[1, 2, 3],
                vector[4, 5],
            ],
            0
        );

        assert!(bv.remove(2) == 2, 0);
        assert!(bv.remove(3) == 3, 0);

        assert!(bv.length() == 3, 0);
        assert!(bv.depth() == 0, 0);

        assert!(bv.preorder_keys() == vector[vector[1, 4, 5]], 0);
        assert!(bv.inorder_values() == vector[vector[1, 4, 5]], 0);

        bv.drop()
    }

    #[test]
    /// Removal that caused a merge wit hteh right neighbour.
    fun test_remove_merge_right() {
        // We need a slightly bigger leaf size to perform a
        // non-trivial merge.
        let mut bv: BigVector<u8> = bv::empty(
            /* max_slice_size */ 4,
            /* max_fan_out */ 4,
            &mut tx_context::dummy(),
        );

        bv.insert(1, 1);
        bv.insert(2, 2);
        bv.insert(3, 3);
        bv.insert(4, 4);
        bv.insert(5, 5);

        assert!(bv.length() == 5, 0);
        assert!(bv.depth() == 1, 0);

        assert!(
            bv.preorder_keys() ==
            vector[
                vector[3],
                vector[1, 2],
                vector[3, 4, 5],
            ],
            0
        );

        assert!(
            bv.inorder_values() ==
            vector[
                vector[1, 2],
                vector[3, 4, 5],
            ],
            0
        );

        assert!(bv.remove(3) == 3, 0);
        assert!(bv.remove(5) == 5, 0);

        assert!(bv.length() == 3, 0);
        assert!(bv.depth() == 0, 0);

        assert!(
            bv.preorder_keys() ==
            vector[vector[1, 2, 4]],
            0
        );

        assert!(
            bv.inorder_values() ==
            vector[vector[1, 2, 4]],
            0
        );

        bv.drop()
    }

    #[test]
    /// Removal that caused the depth of the tree to decrease.
    fun test_remove_shorten() {
        let mut bv = filled_for_test();

        assert!(bv.length() == 10, 0);
        assert!(bv.depth() == 2, 0);

        // Minimally occupy all leaves
        assert!(bv.remove(8) == 8, 0);
        assert!(bv.remove(17) == 17, 0);
        assert!(bv.remove(22) == 22, 0);

        assert!(bv.depth() == 2, 0);
        assert!(bv.length() == 7, 0);

        // Removals triggering merges
        assert!(bv.remove(7) == 7, 0);
        assert!(bv.remove(9) == 9, 0);
        assert!(bv.remove(15) == 15, 0);
        assert!(bv.remove(10) == 10, 0);

        assert!(bv.depth() == 1, 0);
        assert!(bv.length() == 3, 0);

        assert!(
            bv.preorder_keys() ==
            vector[
                vector[15, 20],
                vector[5],
                vector[19],
                vector[20],
            ],
            0
        );

        assert!(
            bv.inorder_values() ==
            vector[
                vector[5],
                vector[19],
                vector[20],
            ],
            0
        );

        bv.drop()
    }

    #[test]
    /// Removal that caused merges and redistributions to fix-up
    /// interior nodes.
    fun test_remove_compound_fix_up() {
        let mut bv = filled_for_test();

        assert!(bv.remove(9) == 9, 0);
        assert!(bv.length() == 9, 0);

        //       ( 5   )
        //     7
        //       ( 7  8)
        //  9
        //       (10   )
        //    15
        //       (15   )
        // 17
        //       (17 19)
        //    20
        //       (20 22)

        assert!(
            bv.preorder_keys() ==
            vector[
                vector[9, 17],

                vector[7],
                vector[5],
                vector[7, 8],

                vector[15],
                vector[10],
                vector[15],

                vector[20],
                vector[17, 19],
                vector[20, 22],
            ],
            0
        );

        assert!(
            bv.inorder_values() ==
            vector[
                vector[5],
                vector[7, 8],
                vector[10],
                vector[15],
                vector[17, 19],
                vector[20, 22],
            ],
            0
        );

        bv.drop()
    }

    #[test]
    fun test_borrow() {
        let bv = filled_for_test();

        assert!(bv[5] == 5, 0);
        assert!(bv[7] == 7, 0);
        assert!(bv[8] == 8, 0);
        assert!(bv[9] == 9, 0);
        assert!(bv[10] == 10, 0);
        assert!(bv[15] == 15, 0);
        assert!(bv[17] == 17, 0);
        assert!(bv[19] == 19, 0);
        assert!(bv[20] == 20, 0);
        assert!(bv[22] == 22, 0);

        bv.drop()
    }

    #[test]
    fun test_borrow_mut() {
        let mut bv = filled_for_test();

        *(&mut bv[7]) = 8;
        *(&mut bv[9]) = 10;
        *(&mut bv[15]) = 16;
        *(&mut bv[19]) = 20;
        *(&mut bv[22]) = 23;

        assert!(bv[5] == 5, 0);
        assert!(bv[7] == 8, 0);
        assert!(bv[8] == 8, 0);
        assert!(bv[9] == 10, 0);
        assert!(bv[10] == 10, 0);
        assert!(bv[15] == 16, 0);
        assert!(bv[17] == 17, 0);
        assert!(bv[19] == 20, 0);
        assert!(bv[20] == 20, 0);
        assert!(bv[22] == 23, 0);

        bv.drop()
    }

    #[test]
    fun test_slice_following() {
        let bv = filled_for_test();

        assert!(next_key(&bv, 1) == 5, 0);
        assert!(next_key(&bv, 5) == 5, 0);
        assert!(next_key(&bv, 6) == 7, 0);
        assert!(next_key(&bv, 18) == 19, 0);
        assert!(next_key(&bv, 22) == 22, 0);

        let (sr, ix) = bv.slice_following(23);
        assert!(sr.is_null(), 0);
        assert!(ix == 0, 0);

        bv.drop()
    }

    /// Create an empty vector using a dummy transaction context,
    /// smaller parameters and a fixed value type for easier testing.
    fun empty_for_test(): BigVector<u8> {
        bv::empty(
            /* max_slice_size */ 2,
            /* max_fan_out */ 4,
            &mut tx_context::dummy(),
        )
    }

    /// Create a vector with some elements using a dummy transaction
    /// context.
    fun filled_for_test(): BigVector<u8> {
        let mut bv = empty_for_test();

        bv.insert(5, 5);
        // ( 5   )

        bv.insert(10, 10);
        // ( 5 10)

        bv.insert(7, 7);
        //    ( 5  7)
        // 10
        //    (10   )

        bv.insert(20, 20);
        //    ( 5  7)
        // 10
        //    (10 20)

        bv.insert(9, 9);
        //    ( 5   )
        //  7
        //    ( 7  9)
        // 10
        //    (10 20)

        bv.insert(8, 8);
        //    ( 5   )
        //  7
        //    ( 7  8)
        //  9
        //    ( 9   )
        // 10
        //    (10 20)

        bv.insert(15, 15);
        //       ( 5   )
        //     7
        //       ( 7  8)
        //  9
        //       ( 9   )
        //    10
        //       (10 15)
        //    20
        //       (20   )

        bv.insert(17, 17);
        //       ( 5   )
        //     7
        //       ( 7  8)
        //  9
        //       ( 9   )
        //    10
        //       (10   )
        //    15
        //       (15 17)
        //    20
        //       (20   )

        bv.insert(22, 22);
        //       ( 5   )
        //     7
        //       ( 7  8)
        //  9
        //       ( 9   )
        //    10
        //       (10   )
        //    15
        //       (15 17)
        //    20
        //       (20 22)

        bv.insert(19, 19);
        //       ( 5   )
        //     7
        //       ( 7  8)
        //  9
        //       ( 9   )
        //    10
        //       (10   )
        // 15
        //       (15   )
        //    17
        //       (17 19)
        //    20
        //       (20 22)

        bv
    }

    /// Returns the next key in `self` at or after `key`, or aborts if
    /// there is none.
    fun next_key<E: store>(self: &BigVector<E>, key: u128): u128 {
        let (sr, ix) = self.slice_following(key);
        let slice = self.borrow_slice(sr);
        slice.key(ix)
    }
}
