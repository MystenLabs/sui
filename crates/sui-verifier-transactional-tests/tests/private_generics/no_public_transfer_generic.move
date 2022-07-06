// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests modules cannot use transfer functions outside of the defining module
// Note: it is not possible to make a generic type `T<...> has key, store`
// where a given instantiation`T<...>` has key but does _not_ have store

//# init --addresses test=0x0

//# publish
module test::m {
    fun t<T: key>(s: T) {
        sui::transfer::transfer(s, @100)
    }
}

//# publish
module test::m {
    fun t<T: key>(
        s: T,
        owner_id: sui::id::VersionedID,
        ctx: &mut sui::tx_context::TxContext,
    ): (sui::id::VersionedID, sui::transfer::ChildRef<T>)  {
        sui::transfer::transfer_to_object_id(s, owner_id)
    }
}

//# publish
module test::m {
    fun t<T: key>(s: T) {
        sui::transfer::freeze_object(s)
    }
}

//# publish
module test::m {
    fun t<T: key>(s: T) {
        sui::transfer::share_object(s)
    }
}

//# publish
module test::m {
    struct R has key { id: sui::id::VersionedID }
    fun t<T: key>(child: T, owner: &mut R): sui::transfer::ChildRef<T> {
        sui::transfer::transfer_to_object(child, owner)
    }
}

//# publish
module test::m {
    struct R has key { id: sui::id::VersionedID }
    fun t<T: key>(child: R, owner: &mut T): sui::transfer::ChildRef<R> {
        sui::transfer::transfer_to_object(child, owner)
    }
}

//# publish
module test::m {
    use sui::transfer::ChildRef;
    struct R has key { id: sui::id::VersionedID }
    fun transfer_child_to_object<T: key>(child: T, c: ChildRef<T>, owner: &mut R): ChildRef<T> {
        sui::transfer::transfer_child_to_object(child, c, owner)
    }
}

//# publish
module test::m {
    use sui::transfer::ChildRef;
    struct R has key { id: sui::id::VersionedID }
    fun transfer_child_to_object<T: key>(child: R, c: ChildRef<R>, owner: &mut T): ChildRef<R> {
        sui::transfer::transfer_child_to_object(child, c, owner)
    }
}

//# publish
module test::m {
    use sui::transfer::ChildRef;
    struct R has key { id: sui::id::VersionedID }
    fun transfer_child_to_object<T: key>(s: T, c: ChildRef<T>) {
        sui::transfer::transfer_child_to_address(s, c, @0x100)
    }
}
