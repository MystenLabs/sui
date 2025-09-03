// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Module: move_package
#[allow(unused)]
module move_package::move_package;

public struct APositionalStruct(u64) has copy, drop, store;

#[ext(some_random_ext_annotation)]
/// Doc comment on a struct
public struct ANamedStruct has copy, drop, store {
    /// Doc comment on a field
    a: u64,
    b: APositionalStruct,
}

public fun f<Type1, OtherType>(_x: Type1, _y: OtherType) {
    abort
}
