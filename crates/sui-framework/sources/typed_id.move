// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Typed wrappers around Sui object IDs
module sui::typed_id {
    use sui::id::{Self, ID};

    /// An ID of an of type `T`. See `ID` for more details
    struct TypedID<phantom T: key> has copy, drop, store {
        id: ID,
    }

    /// Get the underlying `ID` of `obj`, and remember the type
    public fun new<T: key>(obj: &T): TypedID<T> {
        TypedID { id: *id::id(obj) }
    }

    /// Borrow the inner `ID` of `typed_id`
    public fun as_id<T: key>(typed_id: &TypedID<T>): &ID {
        &typed_id.id
    }

    /// Get the inner `ID` of `typed_id`
    public fun to_id<T: key>(typed_id: TypedID<T>): ID {
        let TypedID { id } = typed_id;
        id
    }

    public fun equals_object<T: key>(typed_id: &TypedID<T>, obj: &T): bool {
        &typed_id.id == id::id(obj)
    }
}
