// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use once_cell::sync::Lazy;
use tracing::warn;

pub mod logging;
pub mod metrics;
pub mod random;
pub mod random_util;
pub mod sync;

pub use random_util::tempdir;

#[inline(always)]
pub fn in_antithesis() -> bool {
    static IN_ANTITHESIS: Lazy<bool> = Lazy::new(|| {
        let in_antithesis = std::env::var("ANTITHESIS_OUTPUT_DIR").is_ok();
        if in_antithesis {
            warn!("Detected that we are running in antithesis");
        }
        in_antithesis
    });
    *IN_ANTITHESIS
}

#[inline(always)]
pub fn in_test_configuration() -> bool {
    in_antithesis() || cfg!(msim) || cfg!(debug_assertions)
}
