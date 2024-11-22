// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

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
        (|| {
            $($body)*
        })()
    }};
}
