// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module entry_functions::example;

public struct Foo has key {
    id: UID,
    bar: u64,
}

/// Entry functions can accept a reference to the `TxContext`
/// (mutable or immutable) as their last parameter.
entry fun share(bar: u64, ctx: &mut TxContext) {
    transfer::share_object(Foo {
        id: object::new(ctx),
        bar,
    })
}

/// Parameters passed to entry functions called in a programmable
/// transaction block (like `foo`, below) must be inputs to the
/// transaction block, and not results of previous transactions.
entry fun update(foo: &mut Foo, ctx: &TxContext) {
    foo.bar = tx_context::epoch(ctx);
}

/// Entry functions can return types that have `drop`.
entry fun bar(foo: &Foo): u64 {
    foo.bar
}

/// This function cannot be `entry` because it returns a value
/// that does not have `drop`.
public fun foo(ctx: &mut TxContext): Foo {
    Foo { id: object::new(ctx), bar: 0 }
}
