// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses A0=0x0 A1=0x0 A2=0x0 --accounts A

//# publish --upgradeable --sender A
module A0::m {
    use sui::object::UID;

    struct Canary has key {
        id: UID,
        addr: vector<u8>,
    }

    struct A {}

}

//# upgrade --package A0 --upgrade-capability 1,1 --sender A
module A1::m {
    use sui::object::UID;

    struct Canary has key {
        id: UID,
        addr: vector<u8>,
    }

    struct A {}
    struct B {}
}

//# upgrade --package A1 --upgrade-capability 1,1 --sender A
module A2::m {
    use std::ascii;
    use std::type_name;
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    struct Canary has key {
        id: UID,
        addr: vector<u8>,
    }

    struct A {}
    struct B {}

    entry fun canary<T>(use_original: bool, ctx: &mut TxContext) {
        let type = if (use_original) {
            type_name::get_with_original_ids<T>()
        } else {
            type_name::get<T>()
        };

        let addr = ascii::into_bytes(type_name::get_address(&type));

        transfer::transfer(
            Canary { id: object::new(ctx), addr },
            tx_context::sender(ctx),
        )
    }
}

//# run A2::m::canary --type-args A0::m::A --args true --sender A

//# run A2::m::canary --type-args A1::m::B --args true --sender A

//# run A2::m::canary --type-args A0::m::A --args false --sender A

//# run A2::m::canary --type-args A1::m::B --args false --sender A

//# view-object 4,0

//# view-object 5,0

//# view-object 6,0

//# view-object 7,0
