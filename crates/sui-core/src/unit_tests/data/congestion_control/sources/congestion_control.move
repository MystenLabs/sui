// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module congestion_control::congestion_control {
    public struct Object has key, store {
        id: UID,
        value: u64,
    }

    public entry fun create_owned(ctx: &mut TxContext) {
        transfer::public_transfer(
            Object { id: object::new(ctx), value: 0 },
            tx_context::sender(ctx)
        )
    }

    public entry fun create_shared(ctx: &mut TxContext) {
        transfer::public_share_object(Object { id: object::new(ctx), value: 0 })
    }

    public entry fun increment(o1:  &mut Object, o2:  &mut Object, o3:  &mut Object) {
        let m = o1.value + 1;
        o1.value = m;
        let m = o2.value + 1;
        o2.value = m;
        let m = o3.value + 1;
        o3.value = m;
    }
}
