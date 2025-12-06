// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0
pub mod layout;
pub mod lockfile;
pub mod manifest;
pub mod package_impl;
pub mod package_lock;
pub mod paths;
pub mod root_package;
pub use package_impl::*;
pub use root_package::RootPackage;

/// Convert an async task into a single-threaded task. Copied from `sui-replay-2`
macro_rules! block_on {
    ($expr:expr) => {{
        #[allow(clippy::disallowed_methods, clippy::result_large_err)]
        {
            if tokio::runtime::Handle::try_current().is_ok() {
                // When already inside a Tokio runtime, spawn a scoped thread to
                // run a separate current-thread runtime without requiring
                // tokio::task::block_in_place (which may be unavailable).
                std::thread::scope(|scope| {
                    scope
                        .spawn(|| {
                            let rt = tokio::runtime::Builder::new_current_thread()
                                .enable_all()
                                .build()
                                .expect("failed to build Tokio runtime");
                            rt.block_on($expr)
                        })
                        .join()
                        .expect("failed to join scoped thread running nested runtime")
                })
            } else {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("failed to build Tokio runtime");
                rt.block_on($expr)
            }
        }
    }};
}

pub(crate) use block_on;
