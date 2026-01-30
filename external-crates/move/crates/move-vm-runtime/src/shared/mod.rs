// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::PartialVMResult;
use std::{collections::HashMap, hash::Hash, slice::SliceIndex};

pub mod binary_cache;
pub mod constants;
pub mod gas;
pub mod linkage_context;
pub mod logging;
pub mod types;
pub mod views;
pub mod vm_pointer;

#[macro_export]
macro_rules! try_block {
    ($($body:tt)*) => {{
        #[allow(clippy::redundant_closure_call)]
        (|| {
            $($body)*
        })()
    }};
}

#[macro_export]
macro_rules! partial_vm_error {
    ($error_name:ident $(,)?) => {{
        move_binary_format::errors::PartialVMError::new(
            move_core_types::vm_status::StatusCode::$error_name,
        )
    }};
    ($error_name:ident, $($body:tt)*) => {{
        move_binary_format::errors::PartialVMError::new(
            move_core_types::vm_status::StatusCode::$error_name,
        ).with_message(
            format!($($body)*),
        )
    }};
}

// NB: this does the lookup separately from the insertion, as otherwise would require copying the
// key to retrieve the entry and support the error case.
#[allow(clippy::map_entry)]
/// Either returns a BTreeMap of unique keys, or a repeated key if the input keys are not unique.
pub fn unique_map<Key: Hash + Eq, Value>(
    values: impl IntoIterator<Item = (Key, Value)>,
) -> Result<HashMap<Key, Value>, Key> {
    let mut map = HashMap::new();
    for (k, v) in values {
        if map.contains_key(&k) {
            return Err(k);
        } else {
            map.insert(k, v);
        }
    }
    Ok(map)
}

/// A trait for safe indexing into collections that returns a PartialVMResult as long as the
/// collection implements [`AsRef<[T]>`].
/// This is useful for avoiding panics on out-of-bounds access, and instead returning a proper
/// error.
pub trait SafeIndex<T> {
    /// Get the element at the given [`index`], or return an [`UNKNOWN_INVARIANT_VIOLATION`] error if the
    /// index is out of bounds.
    fn safe_get<'a, I>(&'a self, index: I) -> PartialVMResult<&'a I::Output>
    where
        I: SliceIndex<[T]>,
        T: 'a;
}

impl<T, C> SafeIndex<T> for C
where
    C: AsRef<[T]>,
{
    fn safe_get<'a, I>(&'a self, index: I) -> PartialVMResult<&'a I::Output>
    where
        I: SliceIndex<[T]>,
        T: 'a,
    {
        let slice = self.as_ref();
        let len = slice.len();
        slice.get(index).ok_or_else(|| {
            crate::partial_vm_error!(
                UNKNOWN_INVARIANT_VIOLATION_ERROR,
                "Index out of bounds for collection of length {}",
                len
            )
        })
    }
}
