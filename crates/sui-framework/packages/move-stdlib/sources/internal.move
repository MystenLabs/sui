// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Defines the `Permit` type, which can be used to constrain the logic of a
/// generic function to be authorized only by the module that defines the type
/// parameter.
///
/// ```move
/// module example::use_permit;
///
/// public struct MyType { /* ... */ }
///
/// public fun test_permit() {
///    let permit = internal::permit<MyType>();
///    /* external_module::call_with_permit(permit); */
/// }
/// ```
///
/// To write a function that is guarded by a `Permit`, require it as an argument.
///
/// ```move
/// // Silly mockup of a type registry where a type can be registered only by
/// // the module that defines the type.
/// module example::type_registry;
///
/// public fun register_type<T>(_: internal::Permit<T> /* ... */) {
///   /* ... */
/// }
/// ```
module std::internal;

/// A privileged witness of the `T` type.
/// Instances can only be created by the module that defines the type `T`.
public struct Permit<phantom T>() has drop;

/// Construct a new `Permit` for the type `T`.
/// Can only be called by the module that defines the type `T`.
public fun permit<T>(): Permit<T> { Permit() }
