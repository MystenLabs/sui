// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod binary_cache;
pub mod constants;
pub mod linkage_context;
pub mod logging;
pub mod serialization;
pub mod types;

#[macro_export]
macro_rules! try_block {
    ($($body:tt)*) => {{
        (|| {
            $($body)*
        })()
    }};
}
