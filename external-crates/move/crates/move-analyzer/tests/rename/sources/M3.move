// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Rename::M3 {
    public fun generic<T: drop>(x: T): T {
        x
    }

    public struct Container<T> has drop {
        value: T,
    }

    public fun make_container<T: drop>(x: T): Container<T> {
        Container { value: x }
    }

    public enum Wrapper<T> has drop {
        Some(T),
        None,
    }

    public fun wrap<T: drop>(x: T): Wrapper<T> {
        Wrapper::Some(x)
    }
}
