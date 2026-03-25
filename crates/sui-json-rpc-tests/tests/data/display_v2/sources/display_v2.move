// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module display_v2::display_v2 {
    use std::internal;
    use std::string::String;
    use sui::display_registry::{DisplayCap, DisplayRegistry};
    use sui::dynamic_field as df;

    public struct Foo has key, store {
        id: UID,
        bar: Bar,
    }

    public struct Bar has store {
        baz: Baz,
        val: u64,
    }

    public struct Baz has store {
        qux: Qux,
        val: bool,
    }

    public struct Qux has store {
        quy: Quy,
        val: String,
    }

    public struct Quy has store {
        quz: Quz,
        val: address,
    }

    public struct Quz has store {
        val: u8,
    }

    public entry fun setup_display(registry: &mut DisplayRegistry, ctx: &mut TxContext) {
        let (mut display, cap): (_, DisplayCap<Foo>) = registry.new(internal::permit<Foo>(), ctx);

        display.set(&cap, "bar", "bar is {bar.val}!");
        display.set(&cap, "baz", "baz is {bar.baz.val}?");
        display.set(&cap, "qux", "{bar.baz.qux:json}");
        display.set(&cap, "quy", "quy is {bar.baz.qux.quy.val}.");
        display.set(&cap, "qu_", "x({bar.baz.qux.val}) y({bar.baz.qux.quy.val}), z({bar.baz.qux.quy.quz.val})?!");
        display.set(&cap, "f42", "[42] is {id->[42u64] | 0x00420042u32 :hex}");

        display.share();
        transfer::public_transfer(cap, tx_context::sender(ctx));
    }

    public entry fun mint(
        v_bar: u64,
        v_qux: String,
        v_quy: address,
        recipient: address,
        ctx: &mut TxContext,
    ) {
        let quy = Quy {
            val: v_quy,
            quz: Quz { val: 43 },
        };

        let qux = Qux {
            val: v_qux,
            quy,
        };

        let baz = Baz {
            val: true,
            qux,
        };

        let bar = Bar {
            val: v_bar,
            baz,
        };

        let foo = Foo {
            id: object::new(ctx),
            bar,
        };

        transfer::public_transfer(foo, recipient);
    }

    public entry fun add_df(foo: &mut Foo) {
        df::add(&mut foo.id, 42u64, 0x42004200u32);
    }
}
