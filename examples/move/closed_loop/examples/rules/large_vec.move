// // Copyright (c) Mysten Labs, Inc.
// // SPDX-License-Identifier: Apache-2.0

// module examples::large_vector {
//     use sui::bag::{Self, Bag};
//     use sui::bcs::to_bytes;

//     struct LargeVec has store {
//         bag: Bag,
//         /// 1 stands for 256 shards
//         /// 2 stands for 256^2 shards
//         /// 3 stands for 256^3 shards
//         /// ...and so on
//         pow_256: u8
//     }

//     public fun push_back<Element>(self: &mut LargeVec, e: Element) {
//         let key = slice_key(&e, self.pow_256);
//     }

//     fun slice_key<Element>(key: &Element, len: u8) {
//         let bytes = bcs::to_bytes(key);
//         let (i, res) = (0, vector[]);
//         while (i < (len as u64)) {
//             vector::push_back(&mut res, vector::borrow(&bytes, i));
//         };
//         res
//     }
// }
