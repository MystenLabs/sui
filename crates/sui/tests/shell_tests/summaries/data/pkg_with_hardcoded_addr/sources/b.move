// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(unused)]
/// This is a doc comment on a module
module 0xc0ffee::b;

/// This is a doc comment on a struct
public struct X {
    /// This is a doc comment on a field
    x: u64
}

public fun f<Typename1, Typename2>(_param_1: Typename1, _param_2: Typename2): Typename1 {
    abort
}

