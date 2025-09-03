// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod runtime;
pub mod verifier;

#[cfg(feature = "tracing")]
#[macro_export]
macro_rules! tracing_feature_enabled {
    ($($tt:tt)*) => {
        $($tt)*
    };
}

#[cfg(not(feature = "tracing"))]
#[macro_export]
macro_rules! tracing_feature_enabled {
    ( $( $tt:tt )* ) => {};
}

#[cfg(not(feature = "tracing"))]
#[macro_export]
macro_rules! tracing_feature_disabled {
    ($($tt:tt)*) => {
        if !cfg!(feature = "tracing") {
            $($tt)*
        }
    };
}

#[cfg(feature = "tracing")]
#[macro_export]
macro_rules! tracing_feature_disabled {
    ( $( $tt:tt )* ) => {};
}

/// Call this function to ensure Move VM tracing is disabled.
/// Note: calling panic in the tracing_feature_enabled macro elsewhere
/// may result in complaints of unreachable code.
pub fn ensure_move_vm_profiler_disabled() {
    tracing_feature_enabled! {
        panic!("Cannot run with Move VM tracing feature enabled");
    }
}
