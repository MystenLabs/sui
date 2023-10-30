// // Copyright (c) Mysten Labs, Inc.
// // SPDX-License-Identifier: Apache-2.0

// module examples::large {
//     use std::vector;
//     use sui::object::{Self, UID};
//     use sui::dynamic_field as df;
//     use sui::tx_context::TxContext;

//     /// A Large collection type which can increase the size of a Collection.
//     struct Large<phantom T: copy + drop + store> {
//         id: UID,
//         /// Stores currently attached keys
//         keys: vector<vector<u8>>
//     }

//     public fun empty<T: copy + drop + store>(ctx: &mut TxContext): Large<T> {
//         Large { id: object::new(ctx), keys: vector[] }
//     }

//     public fun clone<T: store + copy + drop>(self: &Large<T>, ctx: &mut TxContext): Large<T> {
//         let (i, id) = (0, object::new(ctx));
//         while (i < vector::length(&self.keys)) {
//             let key = *vector::borrow(&self.keys, i);
//             df::add<vector<u8>, T>(
//                 &mut id, key,
//                 *df::borrow(&self.id, key)
//             );
//             i = i + 1;
//         };

//         Large { id, keys: *&self.keys }
//     }

//     public fun drop<T: copy + drop + store>(self: Large<T>) {
//         let Large { id, keys } = self;
//         while (vector::length(&keys) > 0) {
//             let _: T = df::remove(&mut id, vector::pop_back(&mut keys));
//         };
//         object::delete(id);
//     }

//     public fun elem_key<Element>(el: &Element, len: u8): vector<u8> {
//         let (i, res, bytes) = (0, vector[], bcs::to_bytes(el));
//         while (i < len) {
//             vector::push_back(&mut res, *vector::borrow(&bytes, (i as u64)))
//         };
//         res
//     }
// }
