// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[macro_export]
macro_rules! fatal {
    ($($arg:tt)*) => {{
        tracing::error!($($arg)*);
        panic!($($arg)*);
    }};
}

#[macro_export]
macro_rules! debug_fatal {
    ($($arg:tt)*) => {{
        if cfg!(debug_assertions) {
            $crate::fatal!($($arg)*);
        } else {
            // TODO: Export invariant metric for alerting
            tracing::error!(debug_panic = true, $($arg)*);
        }
    }};
}
