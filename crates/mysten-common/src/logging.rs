// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[macro_export]
macro_rules! fatal {
    ($($arg:tt)*) => {{
        tracing::error!(fatal = true, $($arg)*);
        panic!($($arg)*);
    }};
}

#[macro_export]
macro_rules! debug_fatal {
    ($($arg:tt)*) => {{
        if cfg!(debug_assertions) {
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
