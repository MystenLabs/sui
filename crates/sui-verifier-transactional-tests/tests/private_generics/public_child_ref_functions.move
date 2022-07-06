// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests modules can use child ref functions, even with a type that does not have store

//# init --addresses a=0x0 t1=0x0 t2=0x0

//# publish
module a::m {
    struct S has key { id: sui::id::VersionedID }
}

//# publish
module t1::m {
    use a::m::S;
    use sui::transfer::ChildRef;
    fun t(c: &ChildRef<S>, child: &S): bool {
        sui::transfer::is_child(c, child)
    }
    fun t_gen<T: key>(c: &ChildRef<T>, child: &T): bool {
        sui::transfer::is_child(c, child)
    }
}

//# publish
module t2::m {
    use a::m::S;
    use sui::transfer::ChildRef;
    fun t(id: sui::id::VersionedID, c: ChildRef<S>) {
        sui::transfer::delete_child_object(id, c)
    }
    fun t_gen<T: key>(id: sui::id::VersionedID, c: ChildRef<T>) {
        sui::transfer::delete_child_object(id, c)
    }
}
