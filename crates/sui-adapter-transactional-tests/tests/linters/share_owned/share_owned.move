// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test1=0x0 test2=0x0 test3=0x0

//# lint
module test1::share_pack {
    use sui::transfer;
    use sui::object::{Self, UID};
    use sui::tx_context::TxContext;

    struct Obj has key, store {
        id: UID
    }

    public entry fun share_fresh(ctx: &mut TxContext) {
        let o = Obj { id: object::new(ctx) };
        transfer::public_share_object(o);
    }
}

//# lint
module test2::share_arg {
    use sui::transfer;
    use sui::object::UID;

    #[no_lint]
    struct Obj has key, store {
        id: UID
    }

    public entry fun arg_object(o: Obj) {
        let arg = o;
        transfer::public_share_object(arg);
    }
}

//# lint
module test3::share_unpack {
    use sui::transfer;
    use sui::object::{Self, UID};

    #[no_lint]
    struct Obj has key, store {
        id: UID
    }

    #[no_lint]
    struct Wrapper has key, store {
        id: UID,
        o: Obj,
    }

    public entry fun unpack_obj(w: Wrapper) {
        let Wrapper { id, o } = w;
        transfer::public_share_object(o);
        object::delete(id);
    }
}
