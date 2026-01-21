// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![allow(unsafe_code)]

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------
/// An Arena Pointer, which allows conversion to references and const. Equality is defined as
/// pointer equality, and clone/copy are shallow.
///
/// Note that `T` here must not be mutated after initial creation, because of thread safety.
/// No additionsl should be added to this type that might allow mutation of `T` after creation.
pub struct VMPointer<T>(*const T);

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl<T> VMPointer<T> {
    #[inline]
    pub(crate) fn to_ref<'a>(&self) -> &'a T {
        to_ref(self.0)
    }

    #[inline]
    pub(crate) fn from_ref(t: &T) -> Self {
        Self(t as *const T)
    }

    #[inline]
    pub(crate) fn ptr_eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.0, other.0)
    }

    #[inline]
    pub(crate) fn ptr_clone(&self) -> Self {
        Self(self.0)
    }
}

// -------------------------------------------------------------------------------------------------
// Pointer Operations
// -------------------------------------------------------------------------------------------------

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[inline]
fn to_ref<'a, T>(value: *const T) -> &'a T {
    unsafe { &*value as &T }
}

// -------------------------------------------------------------------------------------------------
// Trait Implementations
// -------------------------------------------------------------------------------------------------

unsafe impl<T: Send> Send for VMPointer<T> {}
unsafe impl<T: Sync> Sync for VMPointer<T> {}

impl<T: ::std::fmt::Debug> ::std::fmt::Debug for VMPointer<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ptr->{:p}", self.to_ref())
    }
}

// VMPointer equality first checks pointer equality, then full equality
impl<T: PartialEq> PartialEq for VMPointer<T> {
    fn eq(&self, other: &Self) -> bool {
        self.ptr_eq(other) || self.to_ref().eq(other.to_ref())
    }
}

// VMPointer equality first checks pointer equality, then full equality
impl<T: Eq> Eq for VMPointer<T> {}

impl<T: PartialOrd> PartialOrd for VMPointer<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.to_ref().partial_cmp(other.to_ref())
    }
}

impl<T: Ord> Ord for VMPointer<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.to_ref().cmp(other.to_ref())
    }
}

impl<T: std::hash::Hash> std::hash::Hash for VMPointer<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.to_ref().hash(state)
    }
}

impl<T> std::ops::Deref for VMPointer<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.to_ref()
    }
}
