// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module std::internal;

/// Witness of the `T` type. Can be instantiated only by the module that defines
/// the `T`, as well as by the `Owner<T>` instance.
public struct Witness<phantom T> has drop {}

/// A storable factory for the `Witness` type. Represents ownership over
/// the `T` type, and allows delegating the ability to construct the `Witness<T>`.
public struct Owner<phantom T> has drop, store {}

/// Construct a new `Witness` for the `T` type. Aborts if the caller is not the
/// defining module of the `T` type.
public fun witness<T /* internal */>(): Witness<T> { Witness {} }

/// Construct a new `Owner` for the `T` type. Aborts if the caller is not the
/// defining module of the `T` type.
public fun new_owner<T /* internal */>(): Owner<T> { Owner {} }

/// Spawn a new `Witness` as an `Owner` of the `T` type. Unlike `witness` and
/// `new_owner`, this function does not not have an internal requirement for the
/// caller.
public fun spawn_witness<T>(_: &Owner<T>): Witness<T> { Witness {} }
