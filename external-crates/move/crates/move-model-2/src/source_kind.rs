// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{borrow::Borrow, ops::Deref};

/// Simple sealing trait that prevents other types from implementing `SourceKind`.
mod private {
    pub trait Sealed {}
}

pub trait SourceKind: private::Sealed + 'static {
    type FromSource<T>;
}

#[derive(Clone, Copy)]
pub struct WithSource;

impl private::Sealed for WithSource {}
impl SourceKind for WithSource {
    type FromSource<T> = AlwaysSome<T>;
}

#[derive(Clone, Copy)]
pub struct WithoutSource;

impl private::Sealed for WithoutSource {}
impl SourceKind for WithoutSource {
    type FromSource<T> = AlwaysNone<T>;
}

#[derive(Clone, Copy)]
pub struct AnyKind;

impl private::Sealed for AnyKind {}
impl SourceKind for AnyKind {
    type FromSource<T> = Option<T>;
}

/// Indicates a populated field of `T`. We use `Option<T>` instead of `T` so that we can safely
/// use `std::mem::transmute` into an `Option<T>` when "upcasting" `WithSource` to `AnyKind`.
#[derive(Clone, Copy)]
pub struct AlwaysSome<T>(Option<T>);

impl<T> AlwaysSome<T> {
    pub fn new(value: T) -> Self {
        Self(Some(value))
    }
}

impl<T> Borrow<T> for AlwaysSome<T> {
    fn borrow(&self) -> &T {
        self.0.as_ref().unwrap()
    }
}

impl<T> Deref for AlwaysSome<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref().unwrap()
    }
}

// Indicates an empty field of `T`. We use `Option<T>` instead of `T` so that we can safely
// use `std::mem::transmute` into an `Option<T>` when "upcasting" `WithSource` to `AnyKind`.
#[derive(Clone, Copy)]
pub struct AlwaysNone<T>(Option<T>);

impl<T> AlwaysNone<T> {
    pub fn new() -> Self {
        Self(None)
    }
}
