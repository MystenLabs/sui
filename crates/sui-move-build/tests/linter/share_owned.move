// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::test1 {
    use sui::transfer;
    use sui::object::UID;

    struct Obj has key, store {
        id: UID
    }

    public entry fun arg_object(o: Obj) {
        let arg = o;
        transfer::public_share_object(arg);
    }
}


module 0x42::test2 {
    use sui::transfer;
    use sui::object::{Self, UID};

    struct Obj has key, store {
        id: UID
    }

    struct Wrapper has key, store {
        id: UID,
        i: u32,
        o: Obj,
    }

    public entry fun unpack_obj(w: Wrapper) {
        let Wrapper { id, i: _, o } = w;
        transfer::public_share_object(o);
        object::delete(id);
    }

    #[lint_allow(share_owned)]
    public entry fun unpack_obj_suppressed(w: Wrapper) {
        let Wrapper { id, i: _, o } = w;
        transfer::public_share_object(o);
        object::delete(id);
    }

    // a linter suppression should not work for regular compiler warnings
    #[linter_allow(code_suppression_should_not_work)]
    fun private_fun_should_not_be_suppressed() {}

    // a linter suppression should not work for regular compiler warnings
    #[linter_allow(category_suppression_should_not_work)]
    fun another_private_fun_should_not_be_suppressed() {}


}
