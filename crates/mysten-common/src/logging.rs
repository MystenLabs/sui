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

#[cfg(msim)]
pub mod intercept_debug_fatal {
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    pub struct DebugFatalCallback {
        pub pattern: String,
        pub callback: Arc<dyn Fn() + Send + Sync>,
    }

    thread_local! {
        static INTERCEPT_DEBUG_FATAL: Mutex<Option<DebugFatalCallback>> = Mutex::new(None);
    }

    pub fn register_callback(message: &str, f: impl Fn() + Send + Sync + 'static) {
        INTERCEPT_DEBUG_FATAL.with(|m| {
            *m.lock().unwrap() = Some(DebugFatalCallback {
                pattern: message.to_string(),
                callback: Arc::new(f),
            });
        });
    }

    pub fn get_callback() -> Option<DebugFatalCallback> {
        INTERCEPT_DEBUG_FATAL.with(|m| m.lock().unwrap().clone())
    }
}

#[macro_export]
macro_rules! register_debug_fatal_handler {
    ($message:literal, $f:expr) => {
        #[cfg(msim)]
        $crate::logging::intercept_debug_fatal::register_callback($message, $f);

        #[cfg(not(msim))]
        {
            // silence unused variable warnings from the body of the callback
            let _ = $f;
        }
    };
}

#[macro_export]
macro_rules! debug_fatal {
    ($($arg:tt)*) => {{
        loop {
            #[cfg(msim)]
            {
                if let Some(cb) = $crate::logging::intercept_debug_fatal::get_callback() {
                    tracing::error!($($arg)*);
                    let msg = format!($($arg)*);
                    if msg.contains(&cb.pattern) {
                        (cb.callback)();
                    }
                    break;
                }
            }

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
            break;
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
