// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::in_test_configuration;
use once_cell::sync::Lazy;

#[macro_export]
macro_rules! fatal {
    ($($arg:tt)*) => {{
        tracing::error!(fatal = true, $($arg)*);
        panic!($($arg)*);
    }};
}

pub use antithesis_sdk::assert_reachable as assert_reachable_antithesis;

#[inline(always)]
pub fn crash_on_debug() -> bool {
    static CRASH_ON_DEBUG: Lazy<bool> = Lazy::new(|| {
        in_test_configuration() || std::env::var("SUI_ENABLE_DEBUG_ASSERTIONS").is_ok()
    });

    *CRASH_ON_DEBUG
}

#[macro_export]
macro_rules! debug_fatal {
    ($($arg:tt)*) => {{
        if $crate::logging::crash_on_debug() {
            $crate::fatal!($($arg)*);
        } else {
            let stacktrace = std::backtrace::Backtrace::capture();
            tracing::error!(debug_fatal = true, stacktrace = ?stacktrace, $($arg)*);
            let location = concat!(file!(), ':', line!());
            if let Some(metrics) = mysten_metrics::get_metrics() {
                metrics.system_invariant_violations.with_label_values(&[location]).inc();
            }
        }
    }};
}

#[macro_export]
macro_rules! assert_reachable {
    () => {
        $crate::logging::assert_reachable!("");
    };
    ($message:literal) => {{
        // calling in to antithesis sdk breaks determinisim in simtests (on linux only)
        if !cfg!(msim) {
            $crate::logging::assert_reachable_antithesis!($message);
        }
    }};
}

mod tests {
    #[test]
    #[should_panic]
    fn test_fatal() {
        fatal!("This is a fatal error");
    }

    #[test]
    #[should_panic]
    fn test_debug_fatal() {
        if cfg!(debug_assertions) {
            debug_fatal!("This is a debug fatal error");
        } else {
            // pass in release mode as well
            fatal!("This is a fatal error");
        }
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn test_debug_fatal_release_mode() {
        debug_fatal!("This is a debug fatal error");
    }
}
