// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, hash::Hash};

pub mod binary_cache;
pub mod constants;
pub mod data_store;
pub mod gas;
pub mod linkage_context;
pub mod logging;
pub mod serialization;
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
