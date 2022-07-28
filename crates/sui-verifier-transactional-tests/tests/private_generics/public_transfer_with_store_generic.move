// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests modules can use transfer functions outside of the defining module, if the type
// has store. This object conditionally has key+store

//# init --addresses a=0x0 t1=0x0 t2=0x0 t3=0x0 t4=0x0 t5=0x0 t6=0x0 t7=0x0 t8=0x0 t9=0x0

//# publish
module a::m {
    struct S<T> has key, store { info: sui::object::Info, v: T }
}

//# publish
module t1::m {
    fun t(s: a::m::S<u64>) {
        sui::transfer::transfer(s, @100)
    }
    fun t_gen<T: key + store>(s: T) {
        sui::transfer::transfer(s, @100)
    }
}

//# publish
module t2::m {
    fun t(
        s: a::m::S<u64>,
        owner_info: &sui::object::Info,
    ) {
        sui::transfer::transfer_to_object_id(s, owner_info)
    }
    fun t_gen<T: key + store>(
        s: T,
        owner_info: &sui::object::Info,
    ) {
        sui::transfer::transfer_to_object_id(s, owner_info)
    }
}

//# publish
module t3::m {
    fun t(s: a::m::S<u64>) {
        sui::transfer::freeze_object(s)
    }
    fun t_gen<T: key + store>(s: T) {
        sui::transfer::freeze_object(s)
    }
}

//# publish
module t4::m {
    fun t(s: a::m::S<u64>) {
        sui::transfer::share_object(s)
    }
    fun t_gen<T: key + store>(s: T) {
        sui::transfer::share_object(s)
    }
}

//# publish
module t5::m {
    struct R has key { info: sui::object::Info }
    fun t(child: a::m::S<u64>, owner: &mut R) {
        sui::transfer::transfer_to_object(child, owner)
    }
    fun t_gen<T: key + store>(child: T, owner: &mut R) {
        sui::transfer::transfer_to_object(child, owner)
    }
}

//# publish
module t6::m {
    struct R has key { info: sui::object::Info }
    fun t(child: R, owner: &mut a::m::S<u64>) {
        sui::transfer::transfer_to_object(child, owner)
    }
    fun t_gen<T: key + store>(child: R, owner: &mut T) {
        sui::transfer::transfer_to_object(child, owner)
    }
}

//# publish
module t7::m {
    fun t(child: a::m::S<u64>, owner: &sui::object::Info) {
        sui::transfer::transfer_to_object_id(child, owner)
    }
    fun t_gen<T: key + store>(child: T, owner: &sui::object::Info) {
        sui::transfer::transfer_to_object_id(child, owner)
    }
}
