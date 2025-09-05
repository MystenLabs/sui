// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::mem::MaybeUninit;

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
    type FromSource<T> = T;
}

#[derive(Clone, Copy)]
pub struct WithoutSource;

impl private::Sealed for WithoutSource {}
impl SourceKind for WithoutSource {
    // We use `Uninit` to ensure it has the same size as `T` which allows for upcasting from
    // `WithoutSource`` to `AnyKind`` safely.
    type FromSource<T> = Uninit<T>;
}

pub struct Uninit<T>(MaybeUninit<T>);

impl<T> Uninit<T> {
    pub fn new() -> Self {
        Self(MaybeUninit::uninit())
    }
}

#[derive(Clone, Copy)]
pub struct AnyKind;

impl private::Sealed for AnyKind {}
impl SourceKind for AnyKind {
    type FromSource<T> = MaybeUninit<T>;
}
