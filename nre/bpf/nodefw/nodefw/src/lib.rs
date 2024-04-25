// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod drainer;
pub mod fwmap;
pub mod server;
pub mod time;

/// var extracts environment variables at runtime with a default fallback value
/// if a default is not provided, the value is simply an empty string if not found
/// This function will return the provided default if env::var cannot find the key
/// or if the key is somehow malformed.
#[macro_export]
macro_rules! var {
    ($key:expr, $default:expr, $type:ty) => {
        match std::env::var($key) {
            Ok(val) => match val.parse::<$type>() {
                Ok(v) => v,
                Err(_) => $default,
            },
            Err(_) => $default,
        }
    };
}
